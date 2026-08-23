use chrono::{DateTime, Local, TimeZone};
use embedded_svc::http::client::Client;
use embedded_svc::http::{Headers, Method};
use embedded_svc::io::{Read, Write};
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::gpio::PinDriver;
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::http::client::{Configuration as HttpConfiguration, EspHttpConnection};
use esp_idf_svc::http::server::{Configuration as HttpServerConfig, EspHttpServer};
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::sntp::{EspSntp, SntpConf, SyncStatus};
use esp_idf_svc::wifi::{
    AuthMethod, BlockingWifi, ClientConfiguration, Configuration as WifiConfiguration, EspWifi,
    WifiEvent,
};
use serde::Deserialize;
use serde_json::json;
use std::env;
use std::error::Error;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[derive(Deserialize, Debug)]
struct SwitchEntry {
    // switch: bool,
    time: DateTime<Local>,
}

const WIFI_SSID: &str = env!("WIFI_SSID");
const WIFI_PW: &str = env!("WIFI_PW");
const DEVICE_ENDPOINT: &str = env!("DEVICE_ENDPOINT");

fn run_http(entry: &Arc<Mutex<SwitchEntry>>) {
    let mut httpserver = EspHttpServer::new(&HttpServerConfig::default()).unwrap();

    let entry_get = Arc::clone(entry);
    httpserver
        .fn_handler(
            "/",
            Method::Get,
            move |request| -> Result<(), esp_idf_svc::io::EspIOError> {
                let entry = entry_get.lock().unwrap();
                let message = format!("{:?}", entry);

                let mut response = request.into_ok_response()?;
                response.write(message.as_bytes())?;
                Ok(())
            },
        )
        .unwrap();

    let entry_set = Arc::clone(entry);
    httpserver
        .fn_handler(
            "/set",
            Method::Post,
            move |mut req| -> Result<(), Box<dyn Error>> {
                let len = req.content_len().unwrap_or(0) as usize;
                let mut buf = vec![0; len];
                req.read_exact(&mut buf)?;
                let mut resp = req.into_ok_response()?;

                match serde_json::from_slice::<SwitchEntry>(&buf) {
                    Ok(set) => {
                        let mut entry = entry_set.lock().unwrap();
                        *entry = SwitchEntry { time: set.time };
                        write!(resp, "accepted")?;
                    }

                    Err(e) => {
                        resp.write_all(format!("JSON error {e}").as_bytes())?;
                    }
                }

                Ok(())
            },
        )
        .unwrap();

    loop {
        thread::sleep(Duration::from_secs(1));
    }
}

fn connect_wifi<'a>(
    modem: esp_idf_svc::hal::modem::Modem<'a>,
    sysloop: &'a EspSystemEventLoop,
    nvs: EspDefaultNvsPartition,
) -> Result<BlockingWifi<EspWifi<'a>>, Box<dyn Error>> {
    let _wifi_events = sysloop.subscribe::<WifiEvent, _>(|event| {
        log::warn!("WIFI EVENT: {:?}", event);
    })?;

    let esp_wifi = EspWifi::new(modem, sysloop.clone(), Some(nvs))?;
    let mut wifi = BlockingWifi::wrap(esp_wifi, sysloop.clone())?;

    wifi.set_configuration(&WifiConfiguration::Client(ClientConfiguration {
        ssid: WIFI_SSID.try_into()?,
        password: WIFI_PW.try_into()?,
        auth_method: AuthMethod::WPA2Personal,
        ..Default::default()
    }))?;

    wifi.start()?;
    wifi.connect()?;
    wifi.wait_netif_up()?;
    log::info!("WIFI connected & ready");

    Ok(wifi)
}

fn sync_time() -> Result<(), Box<dyn Error>> {
    unsafe { std::env::set_var("TZ", "CET-1CEST,M3.5.0,M10.5.0/3") };

    let sntp = EspSntp::new(&SntpConf {
        servers: ["pool.ntp.org"],
        ..Default::default()
    })?;

    while sntp.get_sync_status() != SyncStatus::Completed {
        thread::sleep(Duration::from_secs(1));
    }

    log::info!("NTP synchronized");
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take()?;
    let sysloop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;

    let mut led = PinDriver::output(peripherals.pins.gpio2)?;

    let _ = led.set_high();

    let mut _wifi = connect_wifi(peripherals.modem, &sysloop, nvs)?;

    sync_time()?;

    let now = Local::now();
    log::info!("starting time: {}", now.format("%d.%m.%Y %H:%M:%S"));

    let _client = Client::wrap(EspHttpConnection::new(&HttpConfiguration::default())?);

    let entry = Arc::new(Mutex::new(SwitchEntry {
        time: Local.with_ymd_and_hms(2026, 8, 23, 6, 30, 0).unwrap(),
    }));

    let entry_worker = Arc::clone(&entry);

    let register = thread::spawn(move || {
        run_http(&entry);
    });

    let _ = led.set_low();

    loop {
        let now = Local::now();

        let entry = entry_worker.lock().unwrap();
        let time_target = entry.time;
        std::mem::drop(entry);

        if time_target >= now {
            thread::sleep(Duration::from_secs(60));
            continue;
        }

        let body = json!({
            "id": 1,
            "method": "Switch.Toggle",
            "params": {
                "id": 0
            }
        });

        let body = match serde_json::to_string(&body) {
            Ok(body) => body,
            Err(e) => {
                log::error!("JSON-error: {:?}", e);
                thread::sleep(Duration::from_secs(5));
                continue;
            }
        };

        let headers = [
            ("Content-Type", "application/json"),
            ("Content-Length", &body.len().to_string()),
        ];

        let mut client = match Client::wrap(EspHttpConnection::new(&HttpConfiguration::default())?)
        {
            client => client,
        };

        let mut request = match client.request(Method::Post, DEVICE_ENDPOINT, &headers) {
            Ok(request) => request,
            Err(e) => {
                log::error!("HTTP Request failed {:?}", e);
                continue;
            }
        };

        if let Err(e) = request.write_all(body.as_bytes()) {
            log::error!("error writing HTTP-Body: {:?}", e);
            continue;
        }

        let mut response = match request.submit() {
            Ok(response) => response,
            Err(e) => {
                log::error!("{:?}", e);
                continue;
            }
        };

        log::info!("HTTP-Status: {}", response.status());

        let mut buf = [0u8; 512];
        let mut response_body = Vec::new();

        loop {
            match response.read(&mut buf) {
                Ok(0) => break,

                Ok(n) => {
                    response_body.extend_from_slice(&buf[..n]);
                }

                Err(e) => {
                    log::error!("error reading response: {:?}", e);
                    break;
                }
            }
        }

        log::info!("Response: {}", String::from_utf8_lossy(&response_body));

        break ();
    }

    register.join().expect("http server failed");

    Ok(())
}

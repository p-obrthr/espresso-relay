use chrono::{DateTime, Local};
use embedded_svc::http::client::Client;
use embedded_svc::http::{Headers, Method};
use embedded_svc::io::{Read, Write};
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::gpio::PinDriver;
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::http::client::{Configuration as HttpConfiguration, EspHttpConnection};
use esp_idf_svc::http::server::{Configuration as HttpServerConfig, EspHttpServer};
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use serde::Deserialize;
use serde_json::json;
use std::env;
use std::error::Error;
use std::option::Option;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

mod logger;
use crate::logger::Logger;

mod wifi;
use crate::wifi::WifiManager;

mod time;
use crate::time::sync_time;

#[derive(Deserialize, Debug)]
struct SwitchEntry {
    // switch: bool,
    time: DateTime<Local>,
}

const DEVICE_ENDPOINT: &str = env!("DEVICE_ENDPOINT");

fn run_http(entry: &Arc<Mutex<Option<SwitchEntry>>>, logger: &Logger) {
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
                        *entry = Some(SwitchEntry { time: set.time });
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

    httpserver
        .fn_handler("/on", Method::Get, |req| -> Result<(), Box<dyn Error>> {
            let body = json!({
                "id": 1,
                "method": "Switch.Set",
                "params": {
                    "id": 0,
                    "on": true,
                }
            });

            request(&body)?;

            let mut resp = req.into_ok_response()?;
            resp.write_all(b"okay")?;

            Ok(())
        })
        .unwrap();

    httpserver
        .fn_handler("/off", Method::Get, |req| -> Result<(), Box<dyn Error>> {
            let body = json!({
                "id": 1,
                "method": "Switch.Set",
                "params": {
                    "id": 0,
                    "on": false,
                }
            });

            request(&body)?;

            let mut resp = req.into_ok_response()?;
            resp.write_all(b"okay")?;

            Ok(())
        })
        .unwrap();

    httpserver
        .fn_handler(
            "/status",
            Method::Get,
            |req| -> Result<(), Box<dyn Error>> {
                let body = json!({
                    "id": 1,
                    "method": "Switch.GetStatus",
                    "params": {
                        "id": 0
                    }
                });

                let result = request(&body)?;

                let mut resp = req.into_ok_response()?;
                resp.write_all(result.as_bytes())?;

                Ok(())
            },
        )
        .unwrap();

    let logger_get = logger.clone();

    httpserver
        .fn_handler(
            "/log",
            Method::Get,
            move |req| -> Result<(), Box<dyn Error>> {
                let messages = logger_get.get_messages();

                let body = serde_json::to_string(&messages)?;

                let mut resp = req.into_ok_response()?;
                resp.write_all(body.as_bytes())?;

                Ok(())
            },
        )
        .unwrap();

    loop {
        thread::sleep(Duration::from_secs(1));
    }
}

fn request(body: &serde_json::Value) -> Result<String, Box<dyn Error>> {
    let body = serde_json::to_vec(body)?;

    let content_length = body.len().to_string();

    let headers = [
        ("Content-Type", "application/json"),
        ("Content-Length", content_length.as_str()),
    ];

    let connection = EspHttpConnection::new(&HttpConfiguration::default())?;
    let mut client = Client::wrap(connection);

    let mut request = client.request(Method::Post, DEVICE_ENDPOINT, &headers)?;

    request.write_all(&body)?;

    let mut response = request.submit()?;

    let status = response.status();

    log::info!("Switch.GetStatus HTTP status: {}", status);

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

    let response_body = String::from_utf8(response_body)?;

    log::info!("response: {}", response_body);

    if status != 200 {
        return Err(format!(
            "Request to device failed with HTTP status {}: {}",
            status, response_body
        )
        .into());
    }

    Ok(response_body)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take()?;
    let sysloop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;

    let mut led = PinDriver::output(peripherals.pins.gpio2)?;

    let _ = led.set_high();

    let logger = Logger::new();

    let mut wifi = WifiManager::new(peripherals.modem, sysloop, Some(nvs), logger.clone())?;

    let entry: Arc<Mutex<Option<SwitchEntry>>> = Arc::new(Mutex::new(None));

    let entry_worker = Arc::clone(&entry);

    let logger_http = logger.clone();

    let _ = thread::spawn(move || {
        run_http(&entry, &logger_http);
    });

    loop {
        match wifi.check()? {
            true => {
                let _ = led.set_low();

                let mut entry = entry_worker.lock().unwrap();
                if let Some(set) = entry.as_ref()
                    && set.time < Local::now()
                {
                    request(&json!({
                        "id": 1,
                        "method": "Switch.Set",
                        "params": {
                            "id": 0,
                            "on": true,
                        }
                    }))?;

                    *entry = None;
                    std::mem::drop(entry);
                }
            }

            false => {
                if wifi.connect().and_then(|_| sync_time(&logger)).is_ok() {
                    let _ = led.set_low();
                }
            }
        }

        thread::sleep(Duration::from_secs(30));
    }
}

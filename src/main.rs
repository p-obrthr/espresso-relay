use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::gpio::PinDriver;
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use std::thread;
use std::time::Duration;

mod logger;
use crate::logger::Logger;
mod wifi;
use crate::wifi::WifiManager;
mod time;
use crate::time::sync_time;
mod api;
use crate::api::run_http;
mod switch;
use crate::switch::SwitchManager;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take()?;
    let sysloop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;

    let mut led = PinDriver::output(peripherals.pins.gpio2)?;
    led.set_high()?;

    let logger = Logger::new();
    let mut wifi = WifiManager::new(peripherals.modem, sysloop, Some(nvs), logger.clone())?;

    let switch_manager = SwitchManager::new(logger.clone());

    let switch_manager_http = switch_manager.clone();
    let logger_http = logger.clone();

    thread::spawn(move || {
        run_http(&switch_manager_http, &logger_http);
    });

    loop {
        match wifi.check()? {
            true => {
                switch_manager.check_and_switch()?;
            }

            false => {
                led.set_high()?;
                if wifi.connect().and_then(|_| sync_time(&logger)).is_ok() {
                    led.set_low()?;
                }
            }
        }

        thread::sleep(Duration::from_secs(30));
    }
}

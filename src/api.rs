use chrono::{DateTime, Local};
use embedded_svc::http::{Headers, Method};
use embedded_svc::io::{Read, Write};
use esp_idf_svc::http::server::{Configuration as HttpServerConfig, EspHttpServer};
use serde::Deserialize;
use std::error::Error;
use std::thread;
use std::time::Duration;

use crate::Logger;
use crate::switch::SwitchManager;

#[derive(Deserialize, Debug)]
pub struct SetRequest {
    pub time: DateTime<Local>,
}

pub fn run_http(switch_manager: &SwitchManager, logger: &Logger) {
    let mut httpserver = EspHttpServer::new(&HttpServerConfig::default()).unwrap();

    let switch_get = switch_manager.clone();
    httpserver
        .fn_handler(
            "/",
            Method::Get,
            move |req| -> Result<(), esp_idf_svc::io::EspIOError> {
                let time = switch_get.get_time();
                let mut response = req.into_ok_response()?;
                response.write(format!("{:?}", time).as_bytes())?;
                Ok(())
            },
        )
        .unwrap();

    let switch_set = switch_manager.clone();
    httpserver
        .fn_handler(
            "/set",
            Method::Post,
            move |mut req| -> Result<(), Box<dyn Error>> {
                let len = req.content_len().unwrap_or(0) as usize;
                let mut buf = vec![0; len];
                req.read_exact(&mut buf)?;
                let mut resp = req.into_ok_response()?;

                match serde_json::from_slice::<SetRequest>(&buf) {
                    Ok(set_req) => {
                        switch_set.set_time(set_req.time);
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

    let switch_on = switch_manager.clone();
    httpserver
        .fn_handler(
            "/on",
            Method::Get,
            move |req| -> Result<(), Box<dyn Error>> {
                let mut resp = req.into_ok_response()?;

                match switch_on.switch(true) {
                    Ok(res) => resp.write_all(res.as_bytes())?,
                    Err(e) => resp.write_all(format!("error {e}").as_bytes())?,
                }

                Ok(())
            },
        )
        .unwrap();

    let switch_off = switch_manager.clone();
    httpserver
        .fn_handler(
            "/off",
            Method::Get,
            move |req| -> Result<(), Box<dyn Error>> {
                let mut resp = req.into_ok_response()?;

                match switch_off.switch(false) {
                    Ok(res) => resp.write_all(res.as_bytes())?,
                    Err(e) => resp.write_all(format!("error {e}").as_bytes())?,
                }

                Ok(())
            },
        )
        .unwrap();

    httpserver
        .fn_handler(
            "/status",
            Method::Get,
            move |req| -> Result<(), Box<dyn Error>> {
                let mut resp = req.into_ok_response()?;

                match SwitchManager::get_status() {
                    Ok(status) => resp.write_all(status.as_bytes())?,
                    Err(e) => resp.write_all(format!("error {e}").as_bytes())?,
                }

                Ok(())
            },
        )
        .unwrap();

    httpserver
        .fn_handler(
            "/rawStatus",
            Method::Get,
            move |req| -> Result<(), Box<dyn Error>> {
                let mut resp = req.into_ok_response()?;

                match SwitchManager::get_raw_status() {
                    Ok(status) => resp.write_all(status.as_bytes())?,
                    Err(e) => resp.write_all(format!("error {e}").as_bytes())?,
                }

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

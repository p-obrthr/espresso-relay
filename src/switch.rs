use chrono::{DateTime, Local};
use embedded_svc::http::Method;
use embedded_svc::http::client::Client;
use embedded_svc::io::Write;
use esp_idf_svc::http::client::{Configuration as HttpConfiguration, EspHttpConnection};
use serde::Deserialize;
use serde_json::{Value, json};
use std::error::Error;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::Logger;

const DEVICE_ENDPOINT: &str = env!("DEVICE_ENDPOINT");

#[derive(Clone)]
pub struct SwitchManager {
    pub time: Arc<Mutex<Option<DateTime<Local>>>>,
    logger: Logger,
}

#[derive(Debug, PartialEq)]
pub enum Status {
    On,
    Standby,
    Off,
}

#[derive(Debug, Deserialize)]
struct GetStatusResponse {
    result: ResultResponse,
}

#[derive(Debug, Deserialize)]
struct ResultResponse {
    output: bool,
    current: Value,
}

impl SwitchManager {
    pub fn new(logger: Logger) -> Self {
        Self {
            time: Arc::new(Mutex::new(None)),
            logger: logger,
        }
    }

    pub fn get_time(&self) -> Option<DateTime<Local>> {
        *self.time.lock().unwrap()
    }

    pub fn get_status() -> Result<String, Box<dyn Error>> {
        let status: Status = Self::get_status_enum()?;
        Ok(format!("{:?}", status))
    }

    fn get_status_response() -> Result<GetStatusResponse, Box<dyn Error>> {
        let res = Self::get_raw_status()?;
        Ok(serde_json::from_str(&res)?)
    }

    fn get_status_enum() -> Result<Status, Box<dyn Error>> {
        let deserialized_resp: GetStatusResponse = Self::get_status_response()?;

        let result = &deserialized_resp.result;

        let mut status = Status::Standby;

        if !result.output {
            status = Status::Off;
            return Ok(status);
        }

        let current = result
            .current
            .as_f64()
            .ok_or("error deserializing current")?;

        if current > 0.0 {
            status = Status::On;
        }

        Ok(status)
    }

    pub fn get_raw_status() -> Result<String, Box<dyn Error>> {
        let body = json!({
            "id": 1,
            "method": "Switch.GetStatus",
            "params": {
                "id": 0
            }
        });

        Ok(request(&body)?)
    }

    pub fn switch(&self, to: bool) -> Result<String, Box<dyn Error>> {
        let status = Self::get_status_enum()?;

        if (status == Status::On && to) || (status == Status::Off && !to) {
            return Ok(format!("already {:?}", status));
        }

        if status == Status::Standby && to {
            Self::switch_set_req(false)?;
            thread::sleep(Duration::from_secs(1));
            Self::switch_set_req(true)?;
        } else {
            Self::switch_set_req(to)?;
        }

        let new_status = Self::get_status_enum()?;

        let message = format!("switched from {:?} to {:?}", status, new_status);
        self.logger.info(&message);

        Ok(message)
    }

    fn switch_set_req(to: bool) -> Result<(), Box<dyn Error>> {
        let body = json!({
            "id": 1,
            "method": "Switch.Set",
            "params": {
                "id": 0,
                "on": to,
            }
        });

        request(&body)?;

        Ok(())
    }

    pub fn set_time(&self, to_set: DateTime<Local>) {
        let mut time = self.time.lock().unwrap();
        *time = Some(to_set);
    }

    pub fn check_and_switch(&self) -> Result<(), Box<dyn Error>> {
        let expired = self.time.lock().unwrap().is_some_and(|t| t < Local::now());

        if expired {
            self.switch(true)?;
            *self.time.lock().unwrap() = None;
        }

        Ok(())
    }
}

fn request(body: &Value) -> Result<String, Box<dyn Error>> {
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

    let mut buf = [0u8; 512];
    let mut response_body = Vec::new();

    loop {
        match response.read(&mut buf) {
            Ok(0) => break,

            Ok(n) => {
                response_body.extend_from_slice(&buf[..n]);
            }

            Err(_e) => {
                break;
            }
        }
    }

    let response_body = String::from_utf8(response_body)?;

    if status != 200 {
        return Err(format!(
            "Request to device failed with HTTP status {}: {}",
            status, response_body
        )
        .into());
    }

    Ok(response_body)
}

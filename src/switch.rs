use chrono::{DateTime, Local};
use embedded_svc::http::Method;
use embedded_svc::http::client::Client;
use embedded_svc::io::Write;
use esp_idf_svc::http::client::{Configuration as HttpConfiguration, EspHttpConnection};
use serde_json::json;
use std::error::Error;
use std::sync::{Arc, Mutex};

const DEVICE_ENDPOINT: &str = env!("DEVICE_ENDPOINT");

#[derive(Clone)]
pub struct SwitchManager {
    pub time: Arc<Mutex<Option<DateTime<Local>>>>,
}

impl SwitchManager {
    pub fn new() -> Self {
        Self {
            time: Arc::new(Mutex::new(None)),
        }
    }

    pub fn get_time(&self) -> Option<DateTime<Local>> {
        *self.time.lock().unwrap()
    }

    pub fn get_status(&self) -> Result<String, Box<dyn Error>> {
        let body = json!({
            "id": 1,
            "method": "Switch.GetStatus",
            "params": {
                "id": 0
            }
        });

        Ok(request(&body)?)
    }

    pub fn set_on(&self) -> Result<(), Box<dyn Error>> {
        let body = json!({
            "id": 1,
            "method": "Switch.Set",
            "params": {
                "id": 0,
                "on": true,
            }
        });

        request(&body)?;

        Ok(())
    }

    pub fn set_off(&self) -> Result<(), Box<dyn Error>> {
        let body = json!({
            "id": 1,
            "method": "Switch.Set",
            "params": {
                "id": 0,
                "on": false,
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
            self.set_on()?;
            *self.time.lock().unwrap() = None;
        }

        Ok(())
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

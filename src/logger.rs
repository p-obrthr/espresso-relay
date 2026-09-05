use chrono::{DateTime, Local};
use serde::Serialize;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct Logger {
    messages: Arc<Mutex<VecDeque<LogMessage>>>,
}

#[derive(Debug, Serialize, Clone)]
pub struct LogMessage {
    time: DateTime<Local>,
    message: String,
}

const MESSAGE_CAPACITY: usize = 30;

impl Logger {
    pub fn new() -> Self {
        Self {
            messages: Arc::new(Mutex::new(VecDeque::with_capacity(MESSAGE_CAPACITY))),
        }
    }

    pub fn get_messages(&self) -> Vec<LogMessage> {
        let messages = self.messages.lock().unwrap();
        messages.iter().cloned().collect()
    }

    pub fn log_message(&self, time: DateTime<Local>, message: &str) {
        let mut messages = self.messages.lock().unwrap();

        if messages.len() >= MESSAGE_CAPACITY {
            messages.pop_front();
        }

        messages.push_back(LogMessage {
            time: time,
            message: message.to_string(),
        });
    }

    pub fn info(&self, message: &str) {
        let now = Local::now();
        log::info!("{:?}: {}", now, message);
        self.log_message(now, message);
    }

    #[allow(dead_code)]
    pub fn error(&self, message: &str) {
        let now = Local::now();
        log::error!("{:?}: {}", now, message);
        self.log_message(now, message);
    }
}

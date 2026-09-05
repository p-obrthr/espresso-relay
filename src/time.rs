use esp_idf_svc::sntp::{EspSntp, SntpConf, SyncStatus};
use std::error::Error;
use std::thread;
use std::time::Duration;

use crate::logger::Logger;

pub fn sync_time(logger: &Logger) -> Result<(), Box<dyn Error>> {
    unsafe { std::env::set_var("TZ", "CET-1CEST,M3.5.0,M10.5.0/3") };

    let sntp = EspSntp::new(&SntpConf {
        servers: ["pool.ntp.org"],
        ..Default::default()
    })?;

    while sntp.get_sync_status() != SyncStatus::Completed {
        thread::sleep(Duration::from_secs(1));
    }

    logger.info("TIME: synchronized");

    Ok(())
}

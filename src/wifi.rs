use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::modem::Modem;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::wifi::{
    AuthMethod, BlockingWifi, ClientConfiguration, Configuration as WifiConfiguration, EspWifi,
};
use std::error::Error;

use crate::Logger;

const WIFI_SSID: &str = env!("WIFI_SSID");
const WIFI_PW: &str = env!("WIFI_PW");

pub struct WifiManager<'a> {
    wifi: BlockingWifi<EspWifi<'a>>,
    logger: Logger,
}

impl<'a> WifiManager<'a> {
    pub fn new(
        modem: Modem<'a>,
        sys_loop: EspSystemEventLoop,
        nvs: Option<EspDefaultNvsPartition>,
        logger: Logger,
    ) -> Result<Self, Box<dyn Error>> {
        let wifi = BlockingWifi::wrap(EspWifi::new(modem, sys_loop.clone(), nvs)?, sys_loop)?;

        Ok(Self {
            wifi: wifi,
            logger: logger,
        })
    }

    pub fn check(&mut self) -> Result<bool, Box<dyn Error>> {
        if self.wifi.is_connected()? {
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn connect(&mut self) -> Result<(), Box<dyn Error>> {
        self.logger.info("WIFI: try connecting");

        self.wifi
            .set_configuration(&WifiConfiguration::Client(ClientConfiguration {
                ssid: WIFI_SSID.try_into()?,
                password: WIFI_PW.try_into()?,
                auth_method: AuthMethod::WPA2Personal,
                ..Default::default()
            }))?;

        self.wifi.start()?;
        self.wifi.connect()?;
        self.wifi.wait_netif_up()?;

        let ip = self.get_ip()?;

        self.logger
            .info(&format!("WIFI: connected & ready under IP: {}", ip));

        Ok(())
    }

    pub fn get_ip(&mut self) -> Result<String, Box<dyn Error>> {
        let ip_info = self.wifi.wifi().sta_netif().get_ip_info()?;
        Ok(ip_info.ip.to_string())
    }
}

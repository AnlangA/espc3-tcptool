//! WiFi module for ESP32
//!
//! This module provides functionality for configuring and managing WiFi on ESP32.

use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    nvs::EspDefaultNvsPartition,
    wifi::{AccessPointConfiguration, AuthMethod, ClientConfiguration, Configuration, EspWifi},
};
use log::{info, warn, error};
use std::time::Duration;

use crate::config::WiFiConfig;
use crate::error::{Error, Result};

/// WiFi Manager for ESP32
///
/// Manages WiFi configuration and connection for ESP32 in mixed mode (AP + STA)
pub struct WiFiManager {
    /// The ESP WiFi driver
    wifi: Box<EspWifi<'static>>,
    /// WiFi configuration
    config: WiFiConfig,
}

impl WiFiManager {
    /// Create a new WiFi manager with the given configuration
    pub fn new(config: WiFiConfig) -> Result<Self> {
        let nvs = EspDefaultNvsPartition::take().map_err(|e| Error::WiFiError(format!("Failed to take NVS partition: {}", e)))?;
        let sysloop = EspSystemEventLoop::take().map_err(|e| Error::WiFiError(format!("Failed to take system event loop: {}", e)))?;

        // Create WiFi driver
        let modem = unsafe { esp_idf_svc::hal::modem::Modem::new() };
        let wifi = Box::new(EspWifi::new(
            modem,
            sysloop.clone(),
            Some(nvs),
        ).map_err(|e| Error::WiFiError(format!("Failed to create WiFi driver: {}", e)))?);

        Ok(Self {
            wifi,
            config,
        })
    }

    /// Configure WiFi in mixed mode (AP + STA)
    pub fn configure_mixed_mode(&mut self) -> Result<()> {
        info!("Setting up WiFi AP with SSID: {}", self.config.ap_ssid);

        self.wifi.set_configuration(&Configuration::Mixed(
            ClientConfiguration {
                ssid: self.config.client_ssid.clone(),
                password: self.config.client_password.clone(),
                auth_method: AuthMethod::WPA2Personal,
                ..Default::default()
            },
            AccessPointConfiguration {
                ssid: self.config.ap_ssid.clone(),
                password: self.config.ap_password.clone(),
                auth_method: AuthMethod::WPA2Personal,
                channel: self.config.ap_channel,
                max_connections: self.config.ap_max_connections,
                ..Default::default()
            },
        )).map_err(|e| Error::WiFiError(format!("Failed to set WiFi configuration: {}", e)))?;

        Ok(())
    }

    /// Start WiFi and connect to the configured network
    pub fn start(&mut self) -> Result<()> {
        // Start WiFi
        self.wifi.start().map_err(|e| Error::WiFiError(format!("Failed to start WiFi: {}", e)))?;
        info!("WiFi started");

        // Wait a bit for WiFi to initialize
        std::thread::sleep(Duration::from_secs(1));

        // Connect to client network if in mixed mode
        if let Configuration::Mixed(_, _) = self.wifi.get_configuration().map_err(|e| Error::WiFiError(format!("Failed to get WiFi configuration: {}", e)))? {
            match self.wifi.connect() {
                Ok(_) => info!("WiFi client connected"),
                Err(e) => warn!("WiFi client connection failed: {:?} (continuing in AP-only mode)", e),
            };
        }

        info!("WiFi mixed mode configured");

        // Print the AP IP address for connecting to the TCP server
        if let Some(ap_info) = self.wifi.ap_netif().get_ip_info().ok() {
            info!("AP IP address: {}", ap_info.ip);
            info!("Connect to WiFi SSID '{}' with password '{}'", self.config.ap_ssid, self.config.ap_password);
            info!("Then connect to TCP server at {}:8080", ap_info.ip);
        } else {
            error!("Failed to get AP IP address. Check WiFi configuration.");
        }

        Ok(())
    }

    /// Get the underlying WiFi driver
    pub fn wifi(&self) -> &EspWifi<'static> {
        &self.wifi
    }
}

/// Configure WiFi in mixed mode (AP + STA) with default configuration
///
/// This is a convenience function for backward compatibility
pub fn configure_wifi_mixed_mode() -> anyhow::Result<Box<EspWifi<'static>>> {
    let nvs = EspDefaultNvsPartition::take()?;
    let sysloop = EspSystemEventLoop::take()?;

    // Create WiFi driver
    let modem = unsafe { esp_idf_svc::hal::modem::Modem::new() };
    let mut wifi = Box::new(EspWifi::new(
        modem,
        sysloop.clone(),
        Some(nvs)
    )?);

    // Configure mixed mode with default values
    let client_ssid: heapless::String<32> = heapless::String::try_from("your_wifi_ssid").unwrap();
    let client_pass: heapless::String<64> = heapless::String::try_from("your_wifi_password").unwrap();
    let ap_ssid: heapless::String<32> = heapless::String::try_from("ESP32-AP").unwrap();
    let ap_pass: heapless::String<64> = heapless::String::try_from("password123").unwrap();

    info!("Setting up WiFi AP with SSID: {}", ap_ssid);

    wifi.set_configuration(&Configuration::Mixed(
        ClientConfiguration {
            ssid: client_ssid,
            password: client_pass,
            auth_method: AuthMethod::WPA2Personal,
            ..Default::default()
        },
        AccessPointConfiguration {
            ssid: ap_ssid,
            password: ap_pass,
            auth_method: AuthMethod::WPA2Personal,
            channel: 6,  // Use channel 6 for better compatibility
            max_connections: 10,
            ..Default::default()
        },
    ))?;

    // Start WiFi
    wifi.start()?;
    info!("WiFi started");

    // Wait a bit for WiFi to initialize
    std::thread::sleep(Duration::from_secs(1));

    // Connect to client network if in mixed mode
    if let Configuration::Mixed(_, _) = wifi.get_configuration()? {
        match wifi.connect() {
            Ok(_) => info!("WiFi client connected"),
            Err(e) => warn!("WiFi client connection failed: {:?} (continuing in AP-only mode)", e),
        };
    }

    info!("WiFi mixed mode configured");

    // Print the AP IP address for connecting to the TCP server
    if let Some(ap_info) = wifi.ap_netif().get_ip_info().ok() {
        info!("AP IP address: {}", ap_info.ip);
        info!("Connect to WiFi SSID 'ESP32-AP' with password 'password123'");
        info!("Then connect to TCP server at {}:8080", ap_info.ip);
    } else {
        error!("Failed to get AP IP address. Check WiFi configuration.");
    }

    Ok(wifi)
}

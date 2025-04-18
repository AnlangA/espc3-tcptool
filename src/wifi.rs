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

        // 等待AP模式完全初始化，使用更强的重试机制
        let mut retry_count = 0;
        let max_retries = 10;  // 增加重试次数
        let mut ap_ip = None;

        // 尝试获取AP IP地址
        while retry_count < max_retries {
            if let Some(ap_info) = self.wifi.ap_netif().get_ip_info().ok() {
                if ap_info.ip.is_unspecified() || ap_info.ip.is_loopback() {
                    // IP地址无效，继续重试
                    retry_count += 1;
                    warn!("Invalid AP IP address: {}, retrying... ({}/{})", ap_info.ip, retry_count, max_retries);
                } else {
                    // 有效IP地址
                    info!("AP IP address: {}", ap_info.ip);
                    ap_ip = Some(ap_info.ip);
                    break;
                }
            } else {
                retry_count += 1;
                warn!("Waiting for AP IP address... (attempt {}/{})", retry_count, max_retries);
            }

            // 使用指数退避策略增加等待时间
            let wait_time = std::cmp::min(100 * (1 << retry_count), 1000); // 最多等待1秒
            std::thread::sleep(Duration::from_millis(wait_time));
        }

        // 显示WiFi状态信息
        info!("==================================================");
        info!("WiFi Status");
        info!("==================================================");

        if let Some(ip) = ap_ip {
            info!("Access Point Mode: READY");
            info!("SSID: {}", self.config.ap_ssid);
            info!("Password: {}", self.config.ap_password);
            info!("IP Address: {}", ip);
            info!("TCP Server Port: 8080");
            info!("Connection Instructions:");
            info!("1. Connect to WiFi network '{}'", self.config.ap_ssid);
            info!("2. Use password '{}'", self.config.ap_password);
            info!("3. Connect to TCP server at {}:8080", ip);
        } else {
            error!("Access Point Mode: FAILED");
            error!("Could not obtain valid IP address after {} attempts", max_retries);
            error!("Fallback Connection Instructions:");
            error!("1. Try connecting to SSID '{}' with password '{}'", self.config.ap_ssid, self.config.ap_password);
            error!("2. Try connecting to TCP server at 192.168.4.1:8080");

            // 尝试重新启动WiFi
            warn!("Attempting to restart WiFi...");
            if let Err(e) = self.wifi.stop() {
                error!("Failed to stop WiFi: {}", e);
            } else if let Err(e) = self.wifi.start() {
                error!("Failed to restart WiFi: {}", e);
            } else {
                info!("WiFi restarted successfully");
            }
        }
        info!("==================================================");

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

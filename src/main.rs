use esp_idf_svc::{
    wifi::{AccessPointConfiguration, AuthMethod, ClientConfiguration, Configuration, EspWifi},
    nvs::EspDefaultNvsPartition,
    eventloop::EspSystemEventLoop,
};

use esp_idf_sys as _; // If using the `binstart` feature of `esp-idf-sys`, always keep this module imported
use log::info;

fn main() -> anyhow::Result<()> {
    // Initialize the ESP-IDF system
    esp_idf_sys::link_patches();

    // Configure logging
    esp_idf_svc::log::EspLogger::initialize_default();

    // Configure WiFi in mixed mode
    let _wifi = configure_wifi_mixed_mode()?;

    // Keep the program running
    loop {
        std::thread::sleep(std::time::Duration::from_secs(10));
        info!("ESP32 is running...");
    }
}

fn configure_wifi_mixed_mode() -> anyhow::Result<Box<EspWifi<'static>>> {
    let nvs = EspDefaultNvsPartition::take()?;
    let sysloop = EspSystemEventLoop::take()?;

    // Create WiFi driver
    let modem = unsafe { esp_idf_svc::hal::modem::Modem::new() };
    let mut wifi = Box::new(EspWifi::new(
        modem,
        sysloop.clone(),
        Some(nvs)
    )?);

    // 配置混合模式 (Configure mixed mode)
    let client_ssid: heapless::String<32> = heapless::String::try_from("无常道心").unwrap();
    let client_pass: heapless::String<64> = heapless::String::try_from("houbo19990923").unwrap();
    let ap_ssid: heapless::String<32> = heapless::String::try_from("ESP32热点").unwrap();
    let ap_pass: heapless::String<64> = heapless::String::try_from("password123").unwrap();

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
            channel: 1,
            max_connections: 4,
            ..Default::default()
        },
    ))?;

    wifi.start()?;
    info!("WiFi started");

    if let Configuration::Mixed(_, _) = wifi.get_configuration()? {
        wifi.connect()?;
        info!("WiFi client connected");
    }

    info!("WiFi混合模式已配置 (WiFi mixed mode configured)");

    Ok(wifi)
}

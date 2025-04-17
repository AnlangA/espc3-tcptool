use esp_idf_svc::{
    wifi::{AccessPointConfiguration, AuthMethod, ClientConfiguration, Configuration, EspWifi},
    nvs::EspDefaultNvsPartition,
    eventloop::EspSystemEventLoop,
};
use log::info;
use anyhow;

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

    // 配置混合模式 (Configure mixed mode)
    let client_ssid: heapless::String<32> = heapless::String::try_from("你的WiFi名称").unwrap();
    let client_pass: heapless::String<64> = heapless::String::try_from("你的WiFi密码").unwrap();
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

    // Print the AP IP address for connecting to the TCP server
    if let Some(ap_info) = wifi.ap_netif().get_ip_info().ok() {
        info!("AP IP address: {}", ap_info.ip);
    }

    Ok(wifi)
}

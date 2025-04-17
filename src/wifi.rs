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
    std::thread::sleep(std::time::Duration::from_secs(1));

    // Connect to client network if in mixed mode
    if let Configuration::Mixed(_, _) = wifi.get_configuration()? {
        match wifi.connect() {
            Ok(_) => info!("WiFi client connected"),
            Err(e) => info!("WiFi client connection failed: {:?} (continuing in AP-only mode)", e),
        };
    }

    info!("WiFi混合模式已配置 (WiFi mixed mode configured)");

    // Print the AP IP address for connecting to the TCP server
    if let Some(ap_info) = wifi.ap_netif().get_ip_info().ok() {
        info!("AP IP address: {}", ap_info.ip);
        info!("Connect to WiFi SSID 'ESP32-AP' with password 'password123'");
        info!("Then connect to TCP server at {}:8080", ap_info.ip);
    } else {
        info!("Failed to get AP IP address. Check WiFi configuration.");
    }

    Ok(wifi)
}

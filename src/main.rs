use esp_idf_sys as _; // If using the `binstart` feature of `esp-idf-sys`, always keep this module imported
use log::{info, error};
use std::thread;
use std::sync::Arc;
use std::time::Duration;
use esp_idf_hal::peripherals::Peripherals;

// Import our library modules
use espc3::{
    config::{AppConfig, create_config},
    error::Result,
    tcp_client_manager::TcpClientManager,
    tcp_server::TcpServer,
    uart::UartManager,
    wifi::WiFiManager,
};

// 不再需要导入旧的兼容性函数

fn main() -> anyhow::Result<()> {
    // Initialize the ESP-IDF system
    esp_idf_sys::link_patches();

    // Configure logging
    esp_idf_svc::log::EspLogger::initialize_default();
    info!("ESP32 starting up...");

    // Create application configuration
    let config = create_config();
    info!("Configuration loaded");

    // Get peripherals
    let peripherals = Peripherals::take()?;
    info!("Peripherals initialized");

    // Use the new object-oriented API
    if let Err(e) = run_with_new_api(peripherals, config) {
        error!("Error running application: {}", e);
        return Err(e.into());
    }

    // Note: If you want to use the legacy API instead, comment out the line above
    // and uncomment the line below:
    // run_with_legacy_api(peripherals);

    Ok(())
}

/// Run the application using the new object-oriented API
fn run_with_new_api(peripherals: Peripherals, config: AppConfig) -> Result<()> {
    // Initialize WiFi
    let mut wifi_manager = WiFiManager::new(config.wifi)?;
    info!("WiFi manager created");

    // Configure and start WiFi
    wifi_manager.configure_mixed_mode()?;
    wifi_manager.start()?;

    // WiFi已经在start方法中等待初始化完成
    info!("WiFi initialization complete");

    // Create shared TCP client manager
    let client_manager = Arc::new(TcpClientManager::new());
    info!("TCP client manager created");

    // Initialize UART
    let uart_manager = Arc::new(UartManager::new(
        peripherals.uart1,
        peripherals.pins.gpio21,
        peripherals.pins.gpio20,
        config.uart,
    )?);
    info!("UART manager created");

    // Start UART forwarding service
    UartManager::start_forwarding(Arc::clone(&uart_manager), Arc::clone(&client_manager))?;
    info!("UART forwarding service started");

    // 创建并运行TCP服务器
    info!("Starting TCP server on port {}...", config.tcp_server.port);
    let tcp_server = Arc::new(TcpServer::new(
        config.tcp_server,
        Arc::clone(&client_manager),
        Arc::clone(&uart_manager),
    ));

    // 使用命名线程和更大的栈空间
    let server_arc = Arc::clone(&tcp_server);
    let _server_thread = thread::Builder::new()
        .name("tcp_server".into())
        .stack_size(8192) // 增加栈大小以防止栈溢出
        .spawn(move || {
            info!("TCP server thread started");
            if let Err(e) = server_arc.run() {
                error!("TCP server error: {:?}", e);
            }
        })
        .expect("Failed to spawn TCP server thread");

    // 给TCP服务器时间启动
    thread::sleep(Duration::from_millis(100));
    info!("TCP server started and ready for connections");

    info!("==================================================");
    info!("ESP32 is running with TCP server and UART forwarding service");
    info!("TCP Server Port: 8080");
    info!("UART Baudrate: 115200");
    info!("==================================================");

    // 保持程序运行并定期检查状态
    let mut last_client_count = 0;
    loop {
        thread::sleep(Duration::from_secs(5));

        // 检查客户端连接状态
        if let Ok(current_client_count) = client_manager.client_count() {
            if current_client_count != last_client_count {
                if current_client_count > 0 {
                    info!("Currently {} TCP client(s) connected", current_client_count);
                } else {
                    info!("No TCP clients connected. Waiting for connections...");
                }
                last_client_count = current_client_count;
            }
        }
    }
}

// 旧的兼容性函数已删除

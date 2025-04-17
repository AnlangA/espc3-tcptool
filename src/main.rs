use esp_idf_sys as _; // If using the `binstart` feature of `esp-idf-sys`, always keep this module imported
use log::{info, error};
use std::thread;
use std::sync::Arc;
use esp_idf_hal::peripherals::Peripherals;

// 导入我们的库模块
use espc3::{configure_wifi_mixed_mode, run_tcp_server, initialize_uart_forwarding, create_tcp_client_manager};

fn main() -> anyhow::Result<()> {
    // Initialize the ESP-IDF system
    esp_idf_sys::link_patches();

    // Configure logging
    esp_idf_svc::log::EspLogger::initialize_default();

    // 获取外设
    let peripherals = Peripherals::take()?;

    // Configure WiFi in mixed mode
    let _wifi = configure_wifi_mixed_mode()?;

    // Give WiFi some time to fully initialize
    std::thread::sleep(std::time::Duration::from_secs(2));
    info!("WiFi initialization complete, starting TCP server...");

    // 创建共享的TCP客户端管理器
    let client_manager = create_tcp_client_manager();
    info!("Created shared TCP client manager");

    // Start TCP server in a separate thread with the shared client manager
    let server_client_manager = Arc::clone(&client_manager);
    thread::spawn(move || {
        if let Err(e) = run_tcp_server(server_client_manager) {
            error!("TCP server error: {:?}", e);
        }
    });

    // Give TCP server time to start
    std::thread::sleep(std::time::Duration::from_millis(500));

    // 初始化UART1 (TX:GPIO21, RX:GPIO20, 波特率:115200)
    initialize_uart_forwarding(
        peripherals.uart1,
        peripherals.pins.gpio21,
        peripherals.pins.gpio20,
        115_200, // 波特率设置为115200
        Arc::clone(&client_manager) // 传递共享的客户端管理器
    )?;

    info!("ESP32 is running with TCP server and UART service (forwarding UART data to TCP clients)...");

    // Keep the program running
    loop {
        std::thread::sleep(std::time::Duration::from_secs(10));
        info!("ESP32 still running...");
    }
}

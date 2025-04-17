use esp_idf_sys as _; // If using the `binstart` feature of `esp-idf-sys`, always keep this module imported
use log::{info, error};
use std::thread;
use esp_idf_hal::peripherals::Peripherals;

// 导入我们的库模块
use espc3::{configure_wifi_mixed_mode, run_tcp_server, initialize_uart_echo};

fn main() -> anyhow::Result<()> {
    // Initialize the ESP-IDF system
    esp_idf_sys::link_patches();

    // Configure logging
    esp_idf_svc::log::EspLogger::initialize_default();

    // 获取外设
    let peripherals = Peripherals::take()?;

    // Configure WiFi in mixed mode
    let _wifi = configure_wifi_mixed_mode()?;

    // Start TCP server in a separate thread
    thread::spawn(|| {
        if let Err(e) = run_tcp_server() {
            error!("TCP server error: {:?}", e);
        }
    });

    // 初始化UART1 (TX:GPIO21, RX:GPIO20, 波特率:115200)
    initialize_uart_echo(
        peripherals.uart1,
        peripherals.pins.gpio21,
        peripherals.pins.gpio20,
        115_200 // 波特率设置为115200
    )?;

    info!("ESP32 is running with TCP echo server and UART echo service...");

    // Keep the program running
    loop {
        std::thread::sleep(std::time::Duration::from_secs(10));
        info!("ESP32 still running...");
    }
}

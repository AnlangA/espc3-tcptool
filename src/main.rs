use esp_idf_sys as _; // If using the `binstart` feature of `esp-idf-sys`, always keep this module imported
use log::{info, error};
use std::thread;

// 导入我们的库模块
use espc3::{configure_wifi_mixed_mode, run_tcp_server};

fn main() -> anyhow::Result<()> {
    // Initialize the ESP-IDF system
    esp_idf_sys::link_patches();

    // Configure logging
    esp_idf_svc::log::EspLogger::initialize_default();

    // Configure WiFi in mixed mode
    let _wifi = configure_wifi_mixed_mode()?;

    // Start TCP server in a separate thread
    thread::spawn(|| {
        if let Err(e) = run_tcp_server() {
            error!("TCP server error: {:?}", e);
        }
    });

    info!("ESP32 is running with TCP echo server...");

    // Keep the program running
    loop {
        std::thread::sleep(std::time::Duration::from_secs(10));
        info!("ESP32 still running...");
    }
}

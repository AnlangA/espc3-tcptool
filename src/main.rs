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

// Import the legacy functions
use espc3::{configure_wifi_mixed_mode, run_tcp_server, initialize_uart_forwarding, create_tcp_client_manager};

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

    // Give WiFi some time to fully initialize
    thread::sleep(Duration::from_secs(2));
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

    // Create and run TCP server in a separate thread
    let tcp_server = Arc::new(TcpServer::new(
        config.tcp_server,
        Arc::clone(&client_manager),
        Arc::clone(&uart_manager),
    ));

    let server_arc = Arc::clone(&tcp_server);
    thread::spawn(move || {
        if let Err(e) = server_arc.run() {
            error!("TCP server error: {:?}", e);
        }
    });

    // Give TCP server time to start
    thread::sleep(Duration::from_millis(500));
    info!("TCP server started");

    info!("ESP32 is running with TCP server and UART forwarding service");

    // Keep the program running
    loop {
        thread::sleep(Duration::from_secs(10));
        info!("ESP32 still running...");
    }
}

/// Run the application using the legacy API for backward compatibility
#[allow(dead_code)]
fn run_with_legacy_api(peripherals: Peripherals) -> anyhow::Result<()> {
    // Configure WiFi in mixed mode
    let _wifi = configure_wifi_mixed_mode()?;

    // Give WiFi some time to fully initialize
    std::thread::sleep(Duration::from_secs(2));
    info!("WiFi initialization complete, starting TCP server...");

    // Create shared TCP client manager
    let client_manager = create_tcp_client_manager();
    info!("Created shared TCP client manager");

    // Initialize UART1 (TX:GPIO21, RX:GPIO20, baudrate:115200)
    // and get shared UART manager
    let uart_manager = initialize_uart_forwarding(
        peripherals.uart1,
        peripherals.pins.gpio21,
        peripherals.pins.gpio20,
        115_200, // baudrate set to 115200
        Arc::clone(&client_manager) // pass shared client manager
    )?;
    info!("UART initialized and forwarding service started");

    // Start TCP server in a separate thread with the shared client manager and UART manager
    let server_client_manager = Arc::clone(&client_manager);
    let server_uart_manager = Arc::clone(&uart_manager);
    thread::spawn(move || {
        if let Err(e) = run_tcp_server(server_client_manager, server_uart_manager) {
            error!("TCP server error: {:?}", e);
        }
    });

    // Give TCP server time to start
    std::thread::sleep(Duration::from_millis(500));

    info!("ESP32 is running with TCP server and UART service (forwarding UART data to TCP clients)...");

    // Keep the program running
    loop {
        std::thread::sleep(Duration::from_secs(10));
        info!("ESP32 still running...");
    }
}

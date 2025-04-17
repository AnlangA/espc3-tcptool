//! ESP32 WiFi Access Point with TCP Server and UART Forwarding
//!
//! This crate provides functionality for running an ESP32 as a WiFi access point
//! with a TCP server that forwards data between TCP clients and UART.

// Export modules
pub mod config;
pub mod error;
pub mod tcp_client_manager;
pub mod tcp_server;
pub mod uart;
pub mod wifi;

// Re-export public interfaces for easier access from crate root
pub use config::{AppConfig, create_config};
pub use error::{Error, Result};
pub use tcp_client_manager::{TcpClientManager, create_tcp_client_manager};
pub use tcp_server::{TcpServer, run_tcp_server};
pub use uart::{UartManager, initialize_uart_forwarding};
pub use wifi::{WiFiManager, configure_wifi_mixed_mode};

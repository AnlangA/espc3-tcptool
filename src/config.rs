use heapless::String;

/// WiFi configuration
#[derive(Debug, Clone)]
pub struct WiFiConfig {
    /// SSID for client mode
    pub client_ssid: String<32>,
    /// Password for client mode
    pub client_password: String<64>,
    /// SSID for access point mode
    pub ap_ssid: String<32>,
    /// Password for access point mode
    pub ap_password: String<64>,
    /// WiFi channel for access point mode
    pub ap_channel: u8,
    /// Maximum number of connections for access point mode
    pub ap_max_connections: u16,
}

impl Default for WiFiConfig {
    fn default() -> Self {
        Self {
            client_ssid: String::try_from("your_wifi_ssid").unwrap_or_default(),
            client_password: String::try_from("your_wifi_password").unwrap_or_default(),
            ap_ssid: String::try_from("ESP32-AP").unwrap_or_default(),
            ap_password: String::try_from("password123").unwrap_or_default(),
            ap_channel: 6,
            ap_max_connections: 10,
        }
    }
}

/// TCP server configuration
#[derive(Debug, Clone)]
pub struct TcpServerConfig {
    /// Bind address for the TCP server
    pub bind_address: &'static str,
    /// Port for the TCP server
    pub port: u16,
    /// Buffer size for TCP operations
    pub buffer_size: usize,
}

impl Default for TcpServerConfig {
    fn default() -> Self {
        Self {
            bind_address: "0.0.0.0",
            port: 8080,
            buffer_size: 1024,
        }
    }
}

/// UART configuration
#[derive(Debug, Clone)]
pub struct UartConfig {
    /// Baud rate for UART
    pub baudrate: u32,
    /// Buffer size for UART operations
    pub buffer_size: usize,
    /// Sleep duration between UART polling in milliseconds
    pub poll_interval_ms: u64,
}

impl Default for UartConfig {
    fn default() -> Self {
        Self {
            baudrate: 115_200,
            buffer_size: 512,  // 增加缓冲区大小以减少读取次数
            poll_interval_ms: 1, // 减少轮询间隔以降低延迟
        }
    }
}

/// Application configuration
#[derive(Debug, Clone)]
pub struct AppConfig {
    /// WiFi configuration
    pub wifi: WiFiConfig,
    /// TCP server configuration
    pub tcp_server: TcpServerConfig,
    /// UART configuration
    pub uart: UartConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            wifi: WiFiConfig::default(),
            tcp_server: TcpServerConfig::default(),
            uart: UartConfig::default(),
        }
    }
}

/// Create a new application configuration with default values
pub fn create_config() -> AppConfig {
    AppConfig::default()
}

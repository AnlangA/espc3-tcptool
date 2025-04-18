//! TCP Server module
//!
//! This module provides functionality for running a TCP server that forwards data between
//! TCP clients and UART.
//!
//! It also supports command processing for controlling UART settings, such as changing
//! the baud rate via TCP client commands.

use log::{info, error, debug, trace};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::config::TcpServerConfig;
use crate::error::{Error, Result};
use crate::tcp_client_manager::TcpClientManager;
use crate::uart::UartManager;

/// TCP Server
///
/// Manages a TCP server that accepts connections and forwards data between clients and UART.
pub struct TcpServer {
    /// TCP server configuration
    config: TcpServerConfig,
    /// Client manager for handling client connections
    client_manager: Arc<TcpClientManager>,
    /// UART manager for sending/receiving data from UART
    uart_manager: Arc<UartManager>,
}

impl TcpServer {
    /// Create a new TCP server with the given configuration and managers
    pub fn new(
        config: TcpServerConfig,
        client_manager: Arc<TcpClientManager>,
        uart_manager: Arc<UartManager>,
    ) -> Self {
        Self {
            config,
            client_manager,
            uart_manager,
        }
    }

    /// Check if the received data is a command
    ///
    /// Commands start with "AT+" prefix
    fn is_command(data: &[u8]) -> bool {
        // 检查数据长度是否足够
        if data.len() < 3 {
            return false;
        }

        // 检查是否以AT+开头
        if data[0] == b'A' && data[1] == b'T' && data[2] == b'+' {
            return true;
        }

        false
    }

    /// Process a command from a client
    ///
    /// Currently supported commands:
    /// - AT+BAUD=<rate>: Change UART baud rate
    /// - AT+BAUD?: Query current UART baud rate
    fn process_command(
        data: &[u8],
        uart_manager: &Arc<UartManager>,
        stream_arc: &Arc<Mutex<TcpStream>>,
        peer_addr: &std::net::SocketAddr
    ) -> Result<()> {
        // 将命令转换为字符串
        let cmd_str = match std::str::from_utf8(data) {
            Ok(s) => s.trim(),
            Err(_) => {
                // 发送错误响应
                let response = "ERROR: Invalid command format (not UTF-8)\r\n";
                Self::send_response(stream_arc, response, peer_addr)?;
                return Err(Error::TcpError("Invalid command format (not UTF-8)".to_string()));
            }
        };

        info!("Received command from client {}: {}", peer_addr, cmd_str);

        // 处理波特率设置命令
        if cmd_str.starts_with("AT+BAUD=") {
            // 等待一小段时间，确保客户端准备好接收数据
            thread::sleep(Duration::from_millis(20));

            info!("Processing AT+BAUD= command from client {}", peer_addr);

            // 提取波特率值
            let baud_str = &cmd_str[8..];
            match baud_str.parse::<u32>() {
                Ok(baudrate) => {
                    // 尝试设置新的波特率
                    match uart_manager.as_ref().set_baudrate(baudrate) {
                        Ok(_) => {
                            // 发送成功响应
                            let response = format!("OK: Baudrate changed to {}\r\n", baudrate);
                            if let Err(e) = Self::send_response(stream_arc, &response, peer_addr) {
                                error!("Failed to send baudrate change response to client {}: {}", peer_addr, e);
                                return Err(e);
                            }
                            info!("Successfully changed baudrate to {} for client {}", baudrate, peer_addr);
                        },
                        Err(e) => {
                            // 发送错误响应
                            let response = format!("ERROR: Failed to set baudrate: {}\r\n", e);
                            if let Err(e) = Self::send_response(stream_arc, &response, peer_addr) {
                                error!("Failed to send baudrate error response to client {}: {}", peer_addr, e);
                                return Err(e);
                            }
                        }
                    }
                },
                Err(_) => {
                    // 波特率解析失败
                    let response = format!("ERROR: Invalid baudrate value: {}\r\n", baud_str);
                    if let Err(e) = Self::send_response(stream_arc, &response, peer_addr) {
                        error!("Failed to send invalid baudrate response to client {}: {}", peer_addr, e);
                        return Err(e);
                    }
                }
            }
        }
        // 处理波特率查询命令
        else if cmd_str.starts_with("AT+BAUD?") {
            // 等待一小段时间，确保客户端准备好接收数据
            thread::sleep(Duration::from_millis(20));

            info!("Processing AT+BAUD? command from client {}", peer_addr);

            // 获取当前波特率
            let current_baudrate = uart_manager.as_ref().get_baudrate();
            let response = format!("Current baudrate: {}\r\n", current_baudrate);
            if let Err(e) = Self::send_response(stream_arc, &response, peer_addr) {
                error!("Failed to send baudrate query response to client {}: {}", peer_addr, e);
                return Err(e);
            }
            info!("Successfully sent current baudrate {} to client {}", current_baudrate, peer_addr);
        }
        // 处理帮助命令
        else if cmd_str.starts_with("AT+HELP") {
            // 等待一小段时间，确保客户端准备好接收数据
            thread::sleep(Duration::from_millis(20));

            info!("Processing AT+HELP command from client {}", peer_addr);

            let help_text = String::from("\r\nAvailable commands:\r\n")
                + "  AT+BAUD=<rate>  - Change UART baud rate\r\n"
                + "  AT+BAUD?       - Query current UART baud rate\r\n"
                + "  AT+HELP        - Show this help message\r\n"
                + "\r\nSupported baud rates: 9600, 19200, 38400, 57600, 115200, 230400, 460800, 921600, 1500000\r\n";

            // 发送响应
            if let Err(e) = Self::send_response(stream_arc, &help_text, peer_addr) {
                error!("Failed to send help text to client {}: {}", peer_addr, e);
                return Err(e);
            }
            info!("Successfully sent help text to client {}", peer_addr);
        }
        // 未知命令
        else {
            // 等待一小段时间，确保客户端准备好接收数据
            thread::sleep(Duration::from_millis(20));

            info!("Processing unknown command '{}' from client {}", cmd_str, peer_addr);

            let response = format!("ERROR: Unknown command: {}\r\nType AT+HELP for available commands\r\n", cmd_str);
            if let Err(e) = Self::send_response(stream_arc, &response, peer_addr) {
                error!("Failed to send unknown command response to client {}: {}", peer_addr, e);
                return Err(e);
            }
            info!("Successfully sent unknown command response to client {}", peer_addr);
        }

        Ok(())
    }

    /// Send a response to a client
    fn send_response(
        stream_arc: &Arc<Mutex<TcpStream>>,
        response: &str,
        peer_addr: &std::net::SocketAddr
    ) -> Result<()> {
        // 尝试获取流锁
        let mut stream = match stream_arc.lock() {
            Ok(guard) => guard,
            Err(_) => return Err(Error::TcpError(format!("Failed to lock stream for client {}", peer_addr))),
        };

        // 尝试将流设置为阻塞模式，以确保数据发送完成
        let _ = stream.set_nonblocking(false);

        // 写入响应数据
        match stream.write_all(response.as_bytes()) {
            Ok(_) => {
                // 立即刷新数据，确保数据被发送
                if let Err(e) = stream.flush() {
                    error!("Failed to flush response to client {}: {}", peer_addr, e);
                    return Err(Error::TcpError(format!("Failed to flush response to client {}: {}", peer_addr, e)));
                }

                // 恢复非阻塞模式
                let _ = stream.set_nonblocking(true);

                info!("Sent response to client {}: {}", peer_addr, response.trim());
                Ok(())
            },
            Err(e) => {
                error!("Failed to send response to client {}: {}", peer_addr, e);
                Err(Error::TcpError(format!("Failed to send response to client {}: {}", peer_addr, e)))
            }
        }
    }

    /// Run the TCP server
    ///
    /// This method starts the TCP server and accepts connections.
    pub fn run(&self) -> Result<()> {
        // 创建一个绑定到指定地址和端口的TCP监听器
        let bind_address = format!("{}:{}", self.config.bind_address, self.config.port);

        // 尝试绑定到指定地址和端口
        info!("Attempting to bind TCP server to {}", bind_address);
        let listener = match TcpListener::bind(&bind_address) {
            Ok(l) => {
                info!("Successfully bound to {}", bind_address);
                l
            },
            Err(e) => {
                // 如果绑定失败，尝试备选地址
                error!("Failed to bind to {}: {}", bind_address, e);

                // 尝试备选地址
                let alt_bind_address = format!("192.168.4.1:{}", self.config.port);
                info!("Trying alternative bind address: {}", alt_bind_address);

                match TcpListener::bind(&alt_bind_address) {
                    Ok(l) => {
                        info!("Successfully bound to alternative address: {}", alt_bind_address);
                        l
                    },
                    Err(e2) => {
                        // 如果备选地址也失败，尝试使用不同端口
                        error!("Failed to bind to alternative address {}: {}", alt_bind_address, e2);

                        let fallback_port = self.config.port + 1;
                        let fallback_address = format!("0.0.0.0:{}", fallback_port);
                        info!("Trying fallback address with different port: {}", fallback_address);

                        TcpListener::bind(&fallback_address)
                            .map_err(|e3| Error::TcpError(format!("Failed to bind to any address: {}, {}, {}", e, e2, e3)))?
                    }
                }
            }
        };

        info!("TCP server successfully bound and listening");

        // 设置套接字选项以提高可靠性
        if let Err(e) = listener.set_nonblocking(false) {
            error!("Failed to set TCP listener to blocking mode: {}", e);
            // 即使设置模式失败也继续
        } else {
            info!("TCP server set to blocking mode");
        }

        // Accept connections and process them
        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    // Clone the managers for this thread
                    let client_manager = Arc::clone(&self.client_manager);
                    let uart_manager = Arc::clone(&self.uart_manager);
                    let buffer_size = self.config.buffer_size;

                    // Handle each client in a new thread
                    thread::spawn(move || {
                        if let Err(e) = Self::handle_client(stream, client_manager, uart_manager, buffer_size) {
                            error!("Error handling client: {}", e);
                        }
                    });
                }
                Err(e) => {
                    error!("Connection failed: {}", e);
                }
            }
        }

        Ok(())
    }

    /// Handle a client connection
    ///
    /// This method handles a client connection, reading data from the client and forwarding it to UART.
    /// It also handles receiving data from UART and sending it to the client.
    fn handle_client(
        stream: TcpStream,
        client_manager: Arc<TcpClientManager>,
        uart_manager: Arc<UartManager>,
        buffer_size: usize,
    ) -> Result<()> {
        // 创建一个结构体来存储客户端的数据交互时间
        struct ClientData {
            last_interaction: std::time::Instant,
        }

        // 创建客户端数据实例
        let mut client_data = ClientData {
            last_interaction: std::time::Instant::now(),
        };
        let peer_addr = stream.peer_addr()
            .map_err(|e| Error::TcpError(format!("Failed to get peer address: {}", e)))?;

        info!("New client connected: {}", peer_addr);

        // 检查客户端是否已经连接
        if client_manager.is_client_connected(&peer_addr) {
            info!("Client {} is already connected, updating connection", peer_addr);
        } else {
            // 注册客户端地址
            client_manager.register_client(peer_addr);
            debug!("Registered new client {} with manager", peer_addr);
        }

        // Wrap the stream in an Arc<Mutex<>> for thread-safe sharing
        let stream_arc = Arc::new(Mutex::new(stream));

        // Add the client to the manager
        client_manager.add_client(peer_addr, Arc::clone(&stream_arc))?;
        debug!("Added client stream to manager for {}", peer_addr);

        // Get the stream lock for setting options
        let stream_guard = stream_arc.lock()
            .map_err(|_| Error::TcpError("Failed to lock stream".to_string()))?;

        // Set non-blocking mode so we don't block if there's no data
        if let Err(e) = stream_guard.set_nonblocking(true) {
            error!("Failed to set non-blocking mode for client {}: {}", peer_addr, e);
            // Continue even if setting the mode fails
        }

        // 设置 TCP 的缓冲区大小，提高性能
        if let Err(e) = stream_guard.set_nodelay(true) {
            error!("Failed to set TCP_NODELAY for client {}: {}", peer_addr, e);
            // Continue even if setting the option fails
        }
        debug!("Client {} ready for reading", peer_addr);

        // Release the lock so other threads can use the stream
        drop(stream_guard);

        // 初始化缓冲区
        let mut buffer = vec![0; buffer_size];
        debug!("Starting to read from client {}", peer_addr);

        // 等待一小段时间，确保客户端已准备好接收数据
        thread::sleep(Duration::from_millis(10));

        // 发送欢迎消息
        let welcome_msg = format!(
            "Welcome to ESP32 UART-TCP Bridge! Your client ID: {}\r\n\
            Type AT+HELP for available commands\r\n\
            Current UART baudrate: {}\r\n",
            peer_addr,
            uart_manager.as_ref().get_baudrate()
        );
        if let Ok(mut stream) = stream_arc.lock() {
            match stream.write_all(welcome_msg.as_bytes()) {
                Ok(_) => {
                    // 立即刷新数据，确保数据被发送
                    if let Err(e) = stream.flush() {
                        error!("Failed to flush welcome message to client {}: {}", peer_addr, e);
                    } else {
                        info!("Sent welcome message to client {}", peer_addr);
                    }
                },
                Err(e) => {
                    error!("Failed to send welcome message to client {}: {}", peer_addr, e);
                }
            }
        }
        
        loop {

            // 获取流锁进行读取
            let mut stream = match stream_arc.lock() {
                Ok(guard) => guard,
                Err(e) => {
                    error!("Failed to lock stream for client {}: {}", peer_addr, e);
                    break;
                }
            };

            // Read data from the client
            match stream.read(&mut buffer) {
                Ok(0) => {
                    // Connection closed by client
                    info!("Client {} disconnected", peer_addr);
                    // Remove the client from the manager
                    client_manager.remove_client(&peer_addr)?;
                    debug!("Removed client {} from manager", peer_addr);
                    break;
                }
                Ok(n) => {
                    // Send the received data to UART
                    if n > 0 {
                        // 更新最后一次数据交互时间
                        client_data.last_interaction = std::time::Instant::now();

                        // 使用trace级别记录详细日志，减少日志开销
                        if log::log_enabled!(log::Level::Trace) {
                            let hex_str: String = buffer[0..n].iter()
                                .map(|b| format!("{:02X} ", b))
                                .collect();
                            trace!("TCP -> UART: {} bytes from {} (hex): {}", n, peer_addr, hex_str);
                        } else {
                            debug!("TCP -> UART: {} bytes from {}", n, peer_addr);
                        }

                        // 检查是否是命令
                        if Self::is_command(&buffer[0..n]) {
                            // 释放流锁，以便在命令处理过程中可以重新获取锁
                            drop(stream);

                            // 等待一小段时间，确保客户端准备好接收数据
                            thread::sleep(Duration::from_millis(10));

                            // 处理命令
                            if let Err(e) = Self::process_command(&buffer[0..n], &uart_manager, &stream_arc, &peer_addr) {
                                error!("Error processing command from client {}: {}", peer_addr, e);
                            }
                        } else {
                            // 直接发送数据到UART，不做中间处理
                            if let Err(e) = uart_manager.send_data(&buffer[0..n]) {
                                error!("Error sending data to UART: {}", e);
                            }
                        }
                    }
                }
                Err(e) => {
                    // Check if it's a "would block" error (no data available)
                    let error_string = format!("{:?}", e);
                    if error_string.contains("WouldBlock") || error_string.contains("TimedOut") {
                        // This is just no data available, not an error, don't disconnect
                        // 使用更短的睡眠时间，减少延迟
                        thread::sleep(Duration::from_millis(1));
                        continue;
                    } else {
                        // Real error, disconnect
                        error!("Error reading from client {}: {}", peer_addr, e);
                        // Remove the client from the manager
                        client_manager.remove_client(&peer_addr)?;
                        debug!("Removed client {} from manager due to error", peer_addr);
                        break;
                    }
                }
            }
        }

        Ok(())
    }
}

/// Run a TCP server with the given client manager and UART manager
///
/// This is a convenience function for backward compatibility
pub fn run_tcp_server(client_manager: Arc<TcpClientManager>, uart_manager: Arc<UartManager>) -> anyhow::Result<()> {
    // Create a TCP server with default configuration
    let config = crate::config::TcpServerConfig::default();
    let server = TcpServer::new(config, client_manager, uart_manager);

    // Run the server
    server.run()?;

    Ok(())
}

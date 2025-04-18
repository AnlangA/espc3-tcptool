//! TCP Server module
//!
//! This module provides functionality for running a TCP server that forwards data between
//! TCP clients and UART.

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
    fn handle_client(
        stream: TcpStream,
        client_manager: Arc<TcpClientManager>,
        uart_manager: Arc<UartManager>,
        buffer_size: usize,
    ) -> Result<()> {
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
        debug!("Client {} ready for reading", peer_addr);

        // Release the lock so other threads can use the stream
        drop(stream_guard);

        // 初始化缓冲区
        let mut buffer = vec![0; buffer_size];
        debug!("Starting to read from client {}", peer_addr);

        // 发送欢迎消息
        let welcome_msg = format!("Welcome to ESP32 UART-TCP Bridge! Your client ID: {}\r\n", peer_addr);
        if let Ok(mut stream) = stream_arc.lock() {
            if let Err(e) = stream.write_all(welcome_msg.as_bytes()) {
                error!("Failed to send welcome message to client {}: {}", peer_addr, e);
            } else {
                debug!("Sent welcome message to client {}", peer_addr);
            }
        }

        // 初始化心跳计时器
        let mut last_heartbeat = std::time::Instant::now();
        let heartbeat_interval = Duration::from_secs(30); // 30秒发送一次心跳

        loop {
            // 检查是否需要发送心跳包
            let now = std::time::Instant::now();
            if now.duration_since(last_heartbeat) >= heartbeat_interval {
                if let Ok(mut stream) = stream_arc.lock() {
                    // 发送心跳包以保持连接
                    if let Err(e) = stream.write_all(b"\r\n") {
                        error!("Failed to send heartbeat to client {}: {}", peer_addr, e);
                    } else {
                        trace!("Sent heartbeat to client {}", peer_addr);
                    }
                }
                last_heartbeat = now;
            }

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
                        // 使用trace级别记录详细日志，减少日志开销
                        if log::log_enabled!(log::Level::Trace) {
                            let hex_str: String = buffer[0..n].iter()
                                .map(|b| format!("{:02X} ", b))
                                .collect();
                            trace!("TCP -> UART: {} bytes from {} (hex): {}", n, peer_addr, hex_str);
                        } else {
                            debug!("TCP -> UART: {} bytes from {}", n, peer_addr);
                        }

                        // 直接发送数据到UART，不做中间处理
                        if let Err(e) = uart_manager.send_data(&buffer[0..n]) {
                            error!("Error sending data to UART: {}", e);
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

//! TCP Server module
//!
//! This module provides functionality for running a TCP server that forwards data between
//! TCP clients and UART.

use log::{info, error, debug, trace};
use std::io::Read;
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
        // Create a TCP listener bound to the configured address and port
        let bind_address = format!("{}:{}", self.config.bind_address, self.config.port);
        let listener = TcpListener::bind(&bind_address)
            .map_err(|e| Error::TcpError(format!("Failed to bind to {}: {}", bind_address, e)))?;

        info!("TCP server listening on {}", bind_address);

        // Set socket options for better reliability
        if let Err(e) = listener.set_nonblocking(false) {
            error!("Failed to set TCP listener to blocking mode: {}", e);
            // Continue even if setting the mode fails
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

        // Register the client address
        client_manager.register_client(peer_addr);
        debug!("Registered client {} with manager", peer_addr);

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

        // Buffer for reading data
        let mut buffer = vec![0; buffer_size];
        debug!("Starting to read from client {}", peer_addr);

        loop {
            // Get the stream lock for reading
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

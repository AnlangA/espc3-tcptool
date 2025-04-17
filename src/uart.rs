//! UART module
//!
//! This module provides functionality for UART communication and forwarding data between
//! UART and TCP clients.

use esp_idf_hal::gpio;
use esp_idf_hal::uart::{UartDriver, config};
use esp_idf_hal::prelude::*;
use esp_idf_hal::delay::BLOCK;
use esp_idf_hal::peripheral::Peripheral;
use log::{info, error, debug, trace};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::config::UartConfig;
use crate::error::{Error, Result};
use crate::tcp_client_manager::TcpClientManager;

/// UART Manager
///
/// Manages UART communication and provides methods for sending and receiving data.
pub struct UartManager {
    /// UART driver
    uart: Mutex<UartDriver<'static>>,
    /// UART configuration
    config: UartConfig,
}

impl UartManager {
    /// Create a new UART manager with the given configuration
    pub fn new(
        uart: impl Peripheral<P = esp_idf_hal::uart::UART1> + 'static,
        tx_pin: impl Peripheral<P = impl gpio::OutputPin> + 'static,
        rx_pin: impl Peripheral<P = impl gpio::InputPin> + 'static,
        config: UartConfig,
    ) -> Result<Self> {
        // Configure UART
        let uart_config = config::Config::new().baudrate(Hertz(config.baudrate));

        // Create UART driver
        let uart = UartDriver::new(
            uart,
            tx_pin,
            rx_pin,
            Option::<gpio::Gpio0>::None, // RTS pin (not used)
            Option::<gpio::Gpio1>::None, // CTS pin (not used)
            &uart_config,
        ).map_err(|e| Error::UartError(format!("Failed to create UART driver: {}", e)))?;

        info!("UART initialized with baudrate: {}", config.baudrate);

        Ok(Self {
            uart: Mutex::new(uart),
            config,
        })
    }

    /// Send data to UART
    /// Optimized for low latency
    pub fn send_data(&self, data: &[u8]) -> Result<()> {
        // 如果没有数据，直接返回
        if data.is_empty() {
            return Ok(());
        }

        // 尽量减少锁的持有时间
        {
            let uart = self.uart.lock().map_err(|_| Error::UartError("Failed to lock UART".to_string()))?;
            uart.write(data).map_err(|e| Error::UartError(format!("Failed to write to UART: {}", e)))?;
        }

        // 只在trace级别记录详细日志
        if log::log_enabled!(log::Level::Trace) {
            trace!("UART sent {} bytes", data.len());
        }

        Ok(())
    }

    /// Receive data from UART (non-blocking)
    /// Optimized for low latency
    pub fn receive_data(&self, buffer: &mut [u8]) -> Result<usize> {
        // 尽量减少锁的持有时间
        let result = {
            let uart = self.uart.lock().map_err(|_| Error::UartError("Failed to lock UART".to_string()))?;
            match uart.read(buffer, 0) {
                Ok(len) => Ok(len),
                Err(e) => {
                    // 检查错误类型，如果是超时错误，不记录
                    // 超时错误通常意味着没有数据可读
                    let error_string = format!("{:?}", e);
                    if error_string.contains("TIMEOUT") {
                        // 返回0表示没有数据
                        Ok(0)
                    } else {
                        // 只记录非超时错误
                        Err(Error::UartError(format!("Failed to read from UART: {}", e)))
                    }
                }
            }
        };

        // 只在出错时记录日志，减少日志开销
        if let Err(ref e) = result {
            error!("UART receive error: {}", e);
        }

        result
    }

    /// Receive data from UART (blocking)
    /// Optimized for low latency
    pub fn receive_data_blocking(&self, buffer: &mut [u8]) -> Result<usize> {
        // 尽量减少锁的持有时间
        let result = {
            let uart = self.uart.lock().map_err(|_| Error::UartError("Failed to lock UART".to_string()))?;
            match uart.read(buffer, BLOCK) {
                Ok(len) => Ok(len),
                Err(e) => {
                    // 即使在阻塞模式下，也可能出现超时
                    let error_string = format!("{:?}", e);
                    if error_string.contains("TIMEOUT") {
                        Ok(0)
                    } else {
                        Err(Error::UartError(format!("Failed to read from UART: {}", e)))
                    }
                }
            }
        };

        // 只在出错时记录日志，减少日志开销
        if let Err(ref e) = result {
            error!("UART receive error: {}", e);
        }

        result
    }

    /// Start UART forwarding service
    ///
    /// This method starts a thread that reads data from UART and forwards it to TCP clients.
    /// Optimized for low latency.
    pub fn start_forwarding(self_arc: Arc<Self>, client_manager: Arc<TcpClientManager>) -> Result<()> {
        let uart_manager = Arc::clone(&self_arc);
        let config = uart_manager.config.clone();

        // 使用高优先级线程处理UART数据
        let builder = thread::Builder::new().name("uart_forwarding".into());
        builder.spawn(move || {
            // 预分配缓冲区以避免运行时分配
            let mut buffer = vec![0u8; config.buffer_size];
            let poll_interval = Duration::from_millis(config.poll_interval_ms);

            // 记录上次有数据的时间，用于自适应轮询
            let mut last_data_time = std::time::Instant::now();
            let mut adaptive_interval = poll_interval;

            loop {
                // 使用非阻塞模式读取数据
                match uart_manager.receive_data(&mut buffer) {
                    Ok(len) => {
                        if len > 0 {
                            // 有数据时立即广播到所有TCP客户端，不做中间处理
                            if let Err(e) = client_manager.broadcast(&buffer[0..len]) {
                                error!("Error broadcasting data to clients: {}", e);
                            }

                            // 更新最后收到数据的时间
                            last_data_time = std::time::Instant::now();

                            // 当有数据时使用最短轮询间隔，减少延迟
                            adaptive_interval = poll_interval;

                            // 只在trace级别记录详细数据，减少日志开销
                            if log::log_enabled!(log::Level::Trace) {
                                let hex_str: String = buffer[0..len].iter()
                                    .map(|b| format!("{:02X} ", b))
                                    .collect();
                                trace!("UART -> TCP: {} bytes (hex): {}", len, hex_str);
                            } else {
                                // 只记录长度信息
                                debug!("UART -> TCP: {} bytes", len);
                            }
                        } else {
                            // 如果长时间没有数据，可以增加轮询间隔以减少CPU使用
                            let elapsed = last_data_time.elapsed();
                            if elapsed > Duration::from_millis(100) {
                                // 最多增加到5ms，保证响应性
                                adaptive_interval = Duration::from_millis(
                                    (config.poll_interval_ms).min(5)
                                );
                            }
                        }
                    }
                    Err(e) => {
                        // 只记录非超时错误
                        error!("UART receive error in forwarding service: {}", e);
                    }
                }

                // 使用自适应的轮询间隔
                thread::sleep(adaptive_interval);
            }
        }).map_err(|e| Error::UartError(format!("Failed to spawn UART forwarding thread: {}", e)))?;

        info!("UART to TCP forwarding service started with optimized latency");
        Ok(())
    }
}

/// Create a new UART manager with the given configuration
pub fn create_uart_manager(
    uart: impl Peripheral<P = esp_idf_hal::uart::UART1> + 'static,
    tx_pin: impl Peripheral<P = impl gpio::OutputPin> + 'static,
    rx_pin: impl Peripheral<P = impl gpio::InputPin> + 'static,
    baudrate: u32,
) -> Result<Arc<UartManager>> {
    let config = UartConfig {
        baudrate,
        ..UartConfig::default()
    };

    let uart_manager = UartManager::new(uart, tx_pin, rx_pin, config)?;
    Ok(Arc::new(uart_manager))
}

/// Initialize UART and start forwarding service
///
/// This is a convenience function for backward compatibility
pub fn initialize_uart_forwarding(
    uart: impl Peripheral<P = esp_idf_hal::uart::UART1> + 'static,
    tx_pin: impl Peripheral<P = impl gpio::OutputPin> + 'static,
    rx_pin: impl Peripheral<P = impl gpio::InputPin> + 'static,
    baudrate: u32,
    client_manager: Arc<TcpClientManager>,
) -> anyhow::Result<Arc<UartManager>> {
    // Create UART manager with default configuration
    let uart_manager = create_uart_manager(uart, tx_pin, rx_pin, baudrate)?;

    // Start UART forwarding service
    UartManager::start_forwarding(Arc::clone(&uart_manager), client_manager)?;

    Ok(uart_manager)
}

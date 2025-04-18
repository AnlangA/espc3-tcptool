//! UART module
//!
//! This module provides functionality for UART communication and forwarding data between
//! UART and TCP clients.

use esp_idf_hal::gpio;
use esp_idf_hal::uart::{UartDriver, config};
use esp_idf_hal::prelude::*;
use esp_idf_hal::delay::BLOCK;
use esp_idf_hal::peripheral::Peripheral;
use log::{info, error, trace, warn};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::config::UartConfig;
use crate::error::{Error, Result};
use crate::storage::StorageManager;
use crate::tcp_client_manager::TcpClientManager;

/// UART Manager
///
/// Manages UART communication and provides methods for sending and receiving data.
pub struct UartManager {
    /// UART driver
    uart: Mutex<UartDriver<'static>>,
    /// UART configuration
    config: UartConfig,
    /// Storage manager for persistent configuration
    storage: Option<Mutex<StorageManager>>,
}

impl UartManager {
    /// Create a new UART manager with the given configuration
    pub fn new(
        uart: impl Peripheral<P = esp_idf_hal::uart::UART1> + 'static,
        tx_pin: impl Peripheral<P = impl gpio::OutputPin> + 'static,
        rx_pin: impl Peripheral<P = impl gpio::InputPin> + 'static,
        mut config: UartConfig,
    ) -> Result<Self> {
        // Try to initialize storage manager
        let storage = match StorageManager::new() {
            Ok(storage) => {
                // Try to read baudrate from flash
                if let Some(baudrate) = storage.read_baudrate() {
                    // Check if the baudrate is valid
                    if Self::is_valid_baudrate(baudrate) {
                        // Update config with the baudrate from flash
                        info!("Using baudrate {} from flash", baudrate);
                        config.baudrate = baudrate;
                    } else {
                        warn!("Invalid baudrate {} read from flash, using default", baudrate);
                    }
                } else {
                    info!("No baudrate found in flash, using default: {}", config.baudrate);
                }
                Some(Mutex::new(storage))
            },
            Err(e) => {
                warn!("Failed to initialize storage manager: {}, baudrate will not be persisted", e);
                None
            }
        };

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
            storage,
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

    /// 修改UART波特率
    ///
    /// 这个方法允许动态修改UART的波特率
    pub fn set_baudrate(&self, baudrate: u32) -> Result<()> {
        // 验证波特率是否有效
        if !Self::is_valid_baudrate(baudrate) {
            return Err(Error::UartError(format!("Invalid baudrate: {}", baudrate)));
        }

        // 锁定UART进行重新配置
        let uart_guard = self.uart.lock().map_err(|_| Error::UartError("Failed to lock UART".to_string()))?;

        // 创建新的UART配置
        // 注意：当前不使用这个配置，但保留代码以便将来实现
        let _uart_config = config::Config::new().baudrate(Hertz(baudrate));

        // 应用新的波特率设置
        // 尝试直接重新配置UART
        // 在ESP32上，我们可以尝试使用低级API来设置波特率
        // 这是不安全的操作，需要使用unsafe块
        // 使用UART1而不是UART0，因为我们在初始化时使用的是UART1
        let result = unsafe {
            esp_idf_sys::uart_set_baudrate(1, baudrate)
        };

        match result {
            0 => {
                info!("Successfully changed UART baudrate to {} at runtime", baudrate);
            },
            err => {
                // 如果失败，我们仍然更新内部配置
                warn!("Failed to change UART baudrate at runtime (error code: {}). \
                      Baudrate change will take full effect after device restart", err);
            }
        }

        // 更新内部配置
        let mut config = self.config.clone();
        config.baudrate = baudrate;
        // 修改结构体内部字段
        unsafe {
            let config_ptr = &self.config as *const UartConfig as *mut UartConfig;
            (*config_ptr).baudrate = baudrate;
        }

        // 保存波特率到flash
        if let Some(storage_mutex) = &self.storage {
            match storage_mutex.lock() {
                Ok(mut storage) => {
                    if let Err(e) = storage.save_baudrate(baudrate) {
                        warn!("Failed to save baudrate to flash: {}", e);
                    } else {
                        info!("Baudrate {} saved to flash", baudrate);
                    }
                },
                Err(e) => {
                    warn!("Failed to lock storage manager: {}, baudrate will not be persisted", e);
                }
            }
        }

        // 释放锁
        drop(uart_guard);

        info!("UART baudrate changed to: {}", baudrate);
        Ok(())
    }

    /// 检查波特率是否有效
    fn is_valid_baudrate(baudrate: u32) -> bool {
        // 支持的波特率列表
        const VALID_BAUDRATES: [u32; 9] = [
            9600, 19200, 38400, 57600, 115200, 230400, 460800, 921600, 1500000
        ];

        VALID_BAUDRATES.contains(&baudrate)
    }

    /// 获取当前波特率
    pub fn get_baudrate(&self) -> u32 {
        self.config.baudrate
    }

    /// Start UART forwarding service
    ///
    /// This method starts a thread that reads data from UART and forwards it to TCP clients.
    /// Highly optimized for low latency.
    pub fn start_forwarding(self_arc: Arc<Self>, client_manager: Arc<TcpClientManager>) -> Result<()> {
        let uart_manager = Arc::clone(&self_arc);
        let config = uart_manager.config.clone();

        // 使用高优先级线程处理UART数据
        let builder = thread::Builder::new()
            .name("uart_forwarding".into())
            .stack_size(4096); // 指定足够的栈大小

        builder.spawn(move || {
            // 预分配缓冲区以避免运行时分配
            let mut buffer = vec![0u8; config.buffer_size];
            let poll_interval = Duration::from_millis(config.poll_interval_ms);

            // 记录上次有数据的时间，用于自适应轮询
            let mut last_data_time = std::time::Instant::now();
            let mut adaptive_interval = poll_interval;

            // 检查是否有客户端的频率较低，减少不必要的检查
            let mut check_counter = 0;
            let check_interval = 10; // 每10次读取才检查一次客户端数量

            loop {
                // 定期检查是否有客户端连接
                check_counter += 1;
                if check_counter >= check_interval {
                    check_counter = 0;
                    // 如果没有客户端，可以使用更长的轮询间隔
                    let client_count = match client_manager.client_count() {
                        Ok(count) => count,
                        Err(_) => 0, // 如果出错，假设没有客户端
                    };
                    if client_count == 0 {
                        thread::sleep(Duration::from_millis(50)); // 更长的睡眠时间
                        continue;
                    }
                }

                // 使用非阻塞模式读取数据
                match uart_manager.receive_data(&mut buffer) {
                    Ok(len) => {
                        if len > 0 {
                            // 有数据时立即广播到所有TCP客户端，不做中间处理
                            let _ = client_manager.broadcast(&buffer[0..len]); // 忽略错误，减少延迟

                            // 更新最后收到数据的时间
                            last_data_time = std::time::Instant::now();

                            // 当有数据时使用最短轮询间隔，减少延迟
                            adaptive_interval = poll_interval;

                            // 只在trace级别记录详细数据
                            if log::log_enabled!(log::Level::Trace) {
                                trace!("UART -> TCP: {} bytes", len);
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
                    Err(_) => {
                        // 完全忽略错误，减少延迟
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

// 旧的兼容性函数已删除

use esp_idf_hal::gpio;
use esp_idf_hal::uart::{UartDriver, config};
use esp_idf_hal::prelude::*;
use esp_idf_hal::delay::BLOCK;
use esp_idf_hal::peripheral::Peripheral;
use log::{info, error};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use anyhow;

use crate::tcp_client_manager::TcpClientManager;

pub struct UartManager {
    uart: UartDriver<'static>,
}

impl UartManager {
    pub fn new(
        uart: impl Peripheral<P = esp_idf_hal::uart::UART1> + 'static,
        tx_pin: impl Peripheral<P = impl gpio::OutputPin> + 'static,
        rx_pin: impl Peripheral<P = impl gpio::InputPin> + 'static,
        baudrate: u32,
    ) -> anyhow::Result<Self> {
        // 配置UART
        let config = config::Config::new().baudrate(Hertz(baudrate));

        // 创建UART驱动
        let uart = UartDriver::new(
            uart,
            tx_pin,
            rx_pin,
            Option::<gpio::Gpio0>::None, // RTS pin (不使用)
            Option::<gpio::Gpio1>::None, // CTS pin (不使用)
            &config,
        )?;

        info!("UART1 initialized with TX:GPIO21, RX:GPIO20, baudrate: {}", baudrate);

        Ok(Self { uart })
    }

    // 发送数据
    pub fn send_data(&mut self, data: &[u8]) -> anyhow::Result<()> {
        self.uart.write(data)?;
        info!("UART sent {} bytes", data.len());
        Ok(())
    }

    // 接收数据 (非阻塞)
    pub fn receive_data(&mut self, buffer: &mut [u8]) -> anyhow::Result<usize> {
        // 使用非阻塞模式读取数据
        match self.uart.read(buffer, 0) {
            Ok(len) => Ok(len),
            Err(e) => {
                // 检查错误类型，如果是超时错误，则不记录日志
                // 超时错误通常意味着没有数据可读
                if format!("{:?}", e).contains("TIMEOUT") {
                    // 返回0表示没有数据
                    Ok(0)
                } else {
                    // 其他错误仍然记录
                    error!("UART receive error: {:?}", e);
                    Err(e.into())
                }
            }
        }
    }

    // 接收数据 (阻塞)
    pub fn receive_data_blocking(&mut self, buffer: &mut [u8]) -> anyhow::Result<usize> {
        match self.uart.read(buffer, BLOCK) {
            Ok(len) => Ok(len),
            Err(e) => {
                // 即使在阻塞模式下，也可能出现超时
                if format!("{:?}", e).contains("TIMEOUT") {
                    Ok(0)
                } else {
                    error!("UART receive error: {:?}", e);
                    Err(e.into())
                }
            }
        }
    }

    // 启动UART转发服务，将数据发送到TCP客户端
    pub fn start_uart_forwarding(mut self, client_manager: Arc<TcpClientManager>) -> anyhow::Result<()> {
        thread::spawn(move || {
            let mut buffer = [0u8; 256];

            loop {
                // 使用非阻塞模式读取数据
                if let Ok(len) = self.receive_data(&mut buffer) {
                    if len > 0 {
                        // 打印收到的数据
                        let data_str = match std::str::from_utf8(&buffer[0..len]) {
                            Ok(s) => s.to_string(),
                            Err(_) => format!("(binary data: {:?})", &buffer[0..len])
                        };

                        info!("UART received: {}", data_str);

                        // 尝试将数据解析为十六进制并打印
                        let hex_str: String = buffer[0..len].iter()
                            .map(|b| format!("{:02X} ", b))
                            .collect();
                        info!("UART received (hex): {}", hex_str);

                        // 将数据发送到所有TCP客户端
                        client_manager.broadcast(&buffer[0..len]);
                    }
                    // 如果len为0，表示没有数据，不需要做任何处理
                } else {
                    // 其他非超时错误仍然记录
                    error!("UART receive error in forwarding service");
                }

                // 短暂休眠，避免CPU占用过高
                thread::sleep(Duration::from_millis(10));
            }
        });

        info!("UART to TCP forwarding service started");
        Ok(())
    }
}

// 初始化UART并启动转发服务
pub fn initialize_uart_forwarding(
    uart: impl Peripheral<P = esp_idf_hal::uart::UART1> + 'static,
    tx_pin: impl Peripheral<P = impl gpio::OutputPin> + 'static,
    rx_pin: impl Peripheral<P = impl gpio::InputPin> + 'static,
    baudrate: u32,
    client_manager: Arc<TcpClientManager>,
) -> anyhow::Result<()> {
    // 使用从主函数传递的共享客户端管理器

    let uart_manager = UartManager::new(uart, tx_pin, rx_pin, baudrate)?;
    uart_manager.start_uart_forwarding(client_manager)?;
    Ok(())
}

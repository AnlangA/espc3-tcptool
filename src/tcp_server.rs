use log::{info, error};
use std::io::Read;
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use anyhow;

use crate::tcp_client_manager::TcpClientManager;
use crate::uart::UartManager;

pub fn run_tcp_server(client_manager: Arc<TcpClientManager>, uart_manager: Arc<UartManager>) -> anyhow::Result<()> {
    // Use the shared client manager and UART manager passed from main

    // Create a TCP listener bound to all interfaces (0.0.0.0)
    let listener = TcpListener::bind("0.0.0.0:8080")?;
    info!("TCP server listening on port 8080");

    // Set socket options for better reliability
    if let Ok(_) = listener.set_nonblocking(false) {
        info!("TCP server set to blocking mode");
    }

    // Accept connections and process them
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                // Clone the client manager for this thread
                let client_manager = Arc::clone(&client_manager);

                // Clone the UART manager for this thread
                let client_uart_manager = Arc::clone(&uart_manager);

                // Handle each client in a new thread
                thread::spawn(move || {
                    if let Err(e) = handle_client(stream, client_manager, client_uart_manager) {
                        error!("Error handling client: {:?}", e);
                    }
                });
            }
            Err(e) => {
                error!("Connection failed: {:?}", e);
            }
        }
    }

    Ok(())
}

fn handle_client(stream: TcpStream, client_manager: Arc<TcpClientManager>, uart_manager: Arc<UartManager>) -> anyhow::Result<()> {
    let peer_addr = stream.peer_addr()?;
    info!("New client connected: {}", peer_addr);

    // 先注册客户端地址
    client_manager.register_client(peer_addr);
    info!("Registered client {} with manager", peer_addr);

    // 使用Arc包装流，而不是克隆它
    // 这样可以在多个线程之间安全地共享流
    let stream_arc = Arc::new(Mutex::new(stream));

    // 将客户端添加到管理器
    client_manager.add_client(peer_addr, Arc::clone(&stream_arc));
    info!("Added client stream to manager for {}", peer_addr);

    // 使用原始流进行读取操作
    // 我们需要从 Arc<Mutex<TcpStream>> 中获取原始流
    let stream_guard = stream_arc.lock().unwrap();

    // 设置非阻塞模式，这样即使没有数据也不会阻塞
    if let Err(e) = stream_guard.set_nonblocking(true) {
        error!("Failed to set non-blocking mode for client {}: {:?}", peer_addr, e);
        // 继续处理，即使设置失败
    }
    info!("Client {} ready for reading", peer_addr);

    // 释放锁，这样其他线程可以使用流
    drop(stream_guard);

    // Buffer for reading data
    let mut buffer = [0; 1024];
    info!("Starting to read from client {}", peer_addr);
    loop {
        // 获取流的锁进行读取
        let mut stream = match stream_arc.lock() {
            Ok(guard) => guard,
            Err(e) => {
                error!("Failed to lock stream for client {}: {:?}", peer_addr, e);
                break;
            }
        };

        // Read data from the client
        match stream.read(&mut buffer) {
            Ok(0) => {
                // Connection closed by client
                info!("Client {} disconnected", peer_addr);
                // 从管理器中移除客户端
                client_manager.remove_client(&peer_addr);
                info!("Removed client {} from manager", peer_addr);
                break;
            }
            Ok(n) => {
                // Log received data from client
                info!("Received {} bytes from {}", n, peer_addr);

                // 将收到的数据发送到UART
                if n > 0 {
                    if let Err(e) = uart_manager.send_data(&buffer[0..n]) {
                        error!("Error sending data to UART: {:?}", e);
                    } else {
                        info!("Sent {} bytes from TCP client to UART", n);
                    }
                }
            }
            Err(e) => {
                // 检查是否是“暂时没有数据可读”的错误
                let error_string = format!("{:?}", e);
                if error_string.contains("WouldBlock") || error_string.contains("TimedOut") {
                    // 这只是没有数据可读，不是错误，不应该断开连接
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    continue;
                } else {
                    // 真正的错误，断开连接
                    error!("Error reading from client {}: {:?}", peer_addr, e);
                    // 从管理器中移除客户端
                    client_manager.remove_client(&peer_addr);
                    info!("Removed client {} from manager due to error", peer_addr);
                    break;
                }
            }
        }
    }

    Ok(())
}

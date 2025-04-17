use log::{info, error};
use std::collections::HashMap;
use std::io::Write;
use std::net::{TcpStream, SocketAddr};
use std::sync::{Arc, Mutex};

// 客户端连接管理器
pub struct TcpClientManager {
    clients: Mutex<HashMap<SocketAddr, Arc<Mutex<TcpStream>>>>,
}

impl TcpClientManager {
    pub fn new() -> Self {
        Self {
            clients: Mutex::new(HashMap::new()),
        }
    }

    // 注册客户端地址（不需要流）
    pub fn register_client(&self, addr: SocketAddr) {
        // 我们只需要记录客户端地址，实际的流将在add_client中添加
        info!("Client {} registered for future connection", addr);
    }

    // 添加新客户端（使用Arc<Mutex<TcpStream>>）
    pub fn add_client(&self, addr: SocketAddr, stream_arc: Arc<Mutex<TcpStream>>) {
        // 尝试获取流的锁并设置为阻塞模式
        if let Ok(stream) = stream_arc.lock() {
            if let Err(e) = stream.set_nonblocking(false) {
                error!("Failed to set blocking mode for client {}: {:?}", addr, e);
                // 继续添加客户端，即使设置失败
            }
        } else {
            error!("Failed to lock stream for client {}", addr);
            // 继续添加客户端，即使锁定失败
        }

        let mut clients = self.clients.lock().unwrap();
        info!("Adding client {} to manager", addr);
        clients.insert(addr, stream_arc);
        info!("Total clients: {}", clients.len());
    }

    // 移除客户端
    pub fn remove_client(&self, addr: &SocketAddr) {
        let mut clients = self.clients.lock().unwrap();
        if clients.remove(addr).is_some() {
            info!("Removed client {}", addr);
            info!("Total clients: {}", clients.len());
        }
    }

    // 向所有客户端广播数据
    pub fn broadcast(&self, data: &[u8]) {
        // Skip if no data to send
        if data.is_empty() {
            info!("空数据");
            return;
        }

        let mut clients = self.clients.lock().unwrap();

        // Skip if no clients registered
        if clients.is_empty() {
            info!("没有client");
            return;
        }

        info!("Broadcasting {} bytes to {} clients", data.len(), clients.len());
        let mut disconnected_clients = Vec::new();
        let client_addresses: Vec<SocketAddr> = clients.keys().cloned().collect();

        // 处理所有已注册的客户端
        for addr in client_addresses {
            // 检查客户端是否有流
            if let Some(stream_arc) = clients.get_mut(&addr) {
                info!("Sending data to client {}", addr);

                // 尝试获取流的锁
                if let Ok(mut stream) = stream_arc.lock() {
                    // 尝试写入数据
                    match stream.write_all(data) {
                        Ok(_) => {
                            // 尝试立即刷新以提高响应速度
                            if let Err(e) = stream.flush() {
                                // 检查是否是临时错误
                                let error_string = format!("{:?}", e);
                                if error_string.contains("WouldBlock") || error_string.contains("TimedOut") {
                                    // 这只是临时错误，不断开连接
                                    info!("Temporary flush error for client {}, will retry later", addr);
                                } else {
                                    // 真正的错误，断开连接
                                    error!("Error flushing after write to client {}: {:?}", addr, e);
                                    disconnected_clients.push(addr);
                                }
                            }
                        }
                        Err(e) => {
                            // 检查是否是临时错误
                            let error_string = format!("{:?}", e);
                            if error_string.contains("WouldBlock") || error_string.contains("TimedOut") {
                                // 这只是临时错误，不断开连接
                                info!("Temporary write error for client {}, will retry later", addr);
                            } else {
                                // 真正的错误，断开连接
                                error!("Error sending to client {}: {:?}", addr, e);
                                disconnected_clients.push(addr);
                            }
                        }
                    }
                } else {
                    // 无法获取流的锁
                    error!("Failed to lock stream for client {}", addr);
                    disconnected_clients.push(addr);
                }
            }
        }

        // 记录断开连接的客户端数量
        if !disconnected_clients.is_empty() {
            info!("{} clients disconnected during broadcast", disconnected_clients.len());
        }

        // 移除断开连接的客户端
        for addr in disconnected_clients {
            clients.remove(&addr);
            info!("Removed disconnected client {}", addr);
        }

        let success_count = clients.len();
        if success_count > 0 {
            info!("Successfully sent {} bytes to {} clients", data.len(), success_count);
        }
    }

    // 获取客户端数量
    pub fn client_count(&self) -> usize {
        let clients = self.clients.lock().unwrap();
        clients.len()
    }
}

// 创建一个全局的TCP客户端管理器
pub fn create_tcp_client_manager() -> Arc<TcpClientManager> {
    Arc::new(TcpClientManager::new())
}

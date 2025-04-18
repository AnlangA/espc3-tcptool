//! TCP Client Manager module
//!
//! This module provides functionality for managing TCP client connections.

use log::{info, error, debug, trace};
use std::collections::HashMap;
use std::io::Write;
use std::net::{TcpStream, SocketAddr};
use std::sync::{Arc, Mutex};

use crate::error::{Error, Result};

/// TCP Client Manager
///
/// Manages TCP client connections and provides methods for broadcasting data to all clients.
pub struct TcpClientManager {
    /// Map of client socket addresses to TCP streams
    clients: Mutex<HashMap<SocketAddr, Arc<Mutex<TcpStream>>>>,
    /// Number of active clients (cached to avoid locking for count)
    client_count: std::sync::atomic::AtomicUsize,
}

impl TcpClientManager {
    /// Create a new TCP client manager
    pub fn new() -> Self {
        Self {
            clients: Mutex::new(HashMap::new()),
            client_count: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// Register a client address (without a stream)
    ///
    /// This is useful for tracking clients before their streams are available.
    pub fn register_client(&self, addr: SocketAddr) {
        debug!("Client {} registered for future connection", addr);
    }

    /// Check if a client is connected
    pub fn is_client_connected(&self, addr: &SocketAddr) -> bool {
        let clients = match self.clients.lock() {
            Ok(clients) => clients,
            Err(_) => return false,
        };
        clients.contains_key(addr)
    }

    /// Add a new client with its stream
    ///
    /// The stream is wrapped in an Arc<Mutex<>> for thread-safe sharing.
    pub fn add_client(&self, addr: SocketAddr, stream_arc: Arc<Mutex<TcpStream>>) -> Result<()> {
        // Try to get the stream lock and set it to blocking mode
        if let Ok(stream) = stream_arc.lock() {
            if let Err(e) = stream.set_nonblocking(false) {
                error!("Failed to set blocking mode for client {}: {}", addr, e);
                // Continue adding the client even if setting the mode fails
            }
        } else {
            error!("Failed to lock stream for client {}", addr);
            // Continue adding the client even if locking fails
        }

        // 尽量减少锁的持有时间
        let is_new_client = {
            let mut clients = self.clients.lock().map_err(|_| Error::ClientError("Failed to lock clients map".to_string()))?;
            info!("Adding client {} to manager", addr);
            let is_new = !clients.contains_key(&addr);
            clients.insert(addr, stream_arc);
            is_new
        };

        // 如果是新客户端，增加计数器
        if is_new_client {
            let count = self.client_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
            debug!("Total clients: {}", count);
        }
        Ok(())
    }

    /// Remove a client
    pub fn remove_client(&self, addr: &SocketAddr) -> Result<()> {
        // 尽量减少锁的持有时间
        let removed = {
            let mut clients = self.clients.lock().map_err(|_| Error::ClientError("Failed to lock clients map".to_string()))?;
            clients.remove(addr).is_some()
        };

        // 只在实际移除客户端时更新计数
        if removed {
            info!("Removed client {}", addr);
            let count = self.client_count.fetch_sub(1, std::sync::atomic::Ordering::SeqCst) - 1;
            debug!("Total clients: {}", count);
        }

        Ok(())
    }

    /// Broadcast data to all connected clients
    /// Optimized for low latency
    pub fn broadcast(&self, data: &[u8]) -> Result<usize> {
        // Skip if no data to send
        if data.is_empty() {
            return Ok(0);
        }

        // 尽量减少锁的持有时间，先复制客户端列表
        let client_streams: Vec<(SocketAddr, Arc<Mutex<TcpStream>>)>;
        {
            let clients = self.clients.lock().map_err(|_| Error::ClientError("Failed to lock clients map".to_string()))?;

            // Skip if no clients registered
            if clients.is_empty() {
                return Ok(0);
            }

            // 复制客户端列表，这样可以快速释放锁
            client_streams = clients.iter().map(|(addr, stream)| (*addr, Arc::clone(stream))).collect();
        }

        // 记录断开连接的客户端
        let mut disconnected_clients = Vec::new();
        let mut success_count = 0;

        // 使用trace级别记录详细日志，减少日志开销
        if log::log_enabled!(log::Level::Trace) {
            trace!("Broadcasting {} bytes to {} clients", data.len(), client_streams.len());
        }

        // 处理所有客户端
        for (addr, stream_arc) in client_streams {
            // 尝试获取流的锁
            if let Ok(mut stream) = stream_arc.lock() {
                // 尝试写入数据
                match stream.write_all(data) {
                    Ok(_) => {
                        // 立即刷新以提高响应速度
                        if let Err(e) = stream.flush() {
                            // 检查是否是临时错误
                            let error_string = format!("{:?}", e);
                            if !error_string.contains("WouldBlock") && !error_string.contains("TimedOut") {
                                // 真正的错误，断开连接
                                disconnected_clients.push(addr);
                                continue;
                            }
                        }
                        success_count += 1;
                    }
                    Err(e) => {
                        // 检查是否是临时错误
                        let error_string = format!("{:?}", e);
                        if !error_string.contains("WouldBlock") && !error_string.contains("TimedOut") {
                            // 真正的错误，断开连接
                            disconnected_clients.push(addr);
                        }
                    }
                }
            } else {
                // 无法获取流的锁
                disconnected_clients.push(addr);
            }
        }

        // 如果有断开连接的客户端，则移除它们
        if !disconnected_clients.is_empty() {
            let mut clients = self.clients.lock().map_err(|_| Error::ClientError("Failed to lock clients map".to_string()))?;
            for addr in disconnected_clients {
                clients.remove(&addr);
                debug!("Removed disconnected client {}", addr);
            }
        }

        Ok(success_count)
    }

    /// Get the number of connected clients
    /// Uses atomic counter for better performance
    pub fn client_count(&self) -> Result<usize> {
        // 尝试使用原子计数器
        let count = self.client_count.load(std::sync::atomic::Ordering::Relaxed);

        // 如果需要精确值，可以锁定并计数
        if count == 0 {
            // 可能是计数器不准确，直接计算实际客户端数量
            let clients = self.clients.lock().map_err(|_| Error::ClientError("Failed to lock clients map".to_string()))?;
            let actual_count = clients.len();

            // 更新计数器
            self.client_count.store(actual_count, std::sync::atomic::Ordering::Relaxed);

            Ok(actual_count)
        } else {
            Ok(count)
        }
    }
}

/// Create a new TCP client manager wrapped in an Arc for thread-safe sharing
pub fn create_tcp_client_manager() -> Arc<TcpClientManager> {
    Arc::new(TcpClientManager::new())
}

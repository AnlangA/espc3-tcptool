use log::{info, error};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use anyhow;

pub fn run_tcp_server() -> anyhow::Result<()> {
    // Create a TCP listener bound to the AP IP address
    let listener = TcpListener::bind("0.0.0.0:8080")?;
    info!("TCP server listening on port 8080");

    // Accept connections and process them
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                // Handle each client in a new thread
                thread::spawn(move || {
                    if let Err(e) = handle_client(stream) {
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

fn handle_client(mut stream: TcpStream) -> anyhow::Result<()> {
    let peer_addr = stream.peer_addr()?;
    info!("New client connected: {}", peer_addr);

    // Buffer for reading data
    let mut buffer = [0; 1024];

    loop {
        // Read data from the client
        match stream.read(&mut buffer) {
            Ok(0) => {
                // Connection closed by client
                info!("Client {} disconnected", peer_addr);
                break;
            }
            Ok(n) => {
                // Echo the data back to the client
                info!("Received {} bytes from {}", n, peer_addr);
                stream.write_all(&buffer[0..n])?;
                info!("Echoed data back to {}", peer_addr);
            }
            Err(e) => {
                error!("Error reading from client {}: {:?}", peer_addr, e);
                break;
            }
        }
    }

    Ok(())
}

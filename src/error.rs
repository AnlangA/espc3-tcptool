use std::fmt;
use std::io;
use std::error::Error as StdError;

/// Custom error type for the application
#[derive(Debug)]
pub enum Error {
    /// I/O errors
    Io(io::Error),
    /// ESP-IDF specific errors
    EspError(String),
    /// WiFi configuration errors
    WiFiError(String),
    /// TCP server errors
    TcpError(String),
    /// UART errors
    UartError(String),
    /// Client manager errors
    ClientError(String),
    /// General errors
    General(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(err) => write!(f, "I/O error: {}", err),
            Error::EspError(msg) => write!(f, "ESP-IDF error: {}", msg),
            Error::WiFiError(msg) => write!(f, "WiFi error: {}", msg),
            Error::TcpError(msg) => write!(f, "TCP error: {}", msg),
            Error::UartError(msg) => write!(f, "UART error: {}", msg),
            Error::ClientError(msg) => write!(f, "Client error: {}", msg),
            Error::General(msg) => write!(f, "Error: {}", msg),
        }
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Error::Io(err) => Some(err),
            _ => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Self {
        Error::Io(err)
    }
}

impl From<anyhow::Error> for Error {
    fn from(err: anyhow::Error) -> Self {
        Error::General(err.to_string())
    }
}

/// Result type for the application
pub type Result<T> = std::result::Result<T, Error>;

//! Storage module
//!
//! This module provides functionality for storing and retrieving configuration
//! values in non-volatile storage (NVS).

use esp_idf_svc::nvs::{EspNvs, NvsCustom, EspCustomNvsPartition};
use log::{info, error, warn};

use crate::error::{Error, Result};

/// Key for storing the UART baudrate in NVS
const BAUDRATE_KEY: &str = "uart_baud";

/// Storage manager for persistent configuration
pub struct StorageManager {
    /// NVS handle
    nvs: EspNvs<NvsCustom>,
}

impl StorageManager {
    /// Create a new storage manager
    pub fn new() -> Result<Self> {
        // Use a custom NVS partition instead of the default one
        let nvs_partition = EspCustomNvsPartition::take("nvs")
            .map_err(|e| Error::StorageError(format!("Failed to take custom NVS partition: {}", e)))?;

        // Open the NVS namespace for our application
        let nvs = EspNvs::new(nvs_partition, "uart_cfg", true)
            .map_err(|e| Error::StorageError(format!("Failed to open NVS namespace: {}", e)))?;

        Ok(Self { nvs })
    }

    /// Save the UART baudrate to NVS
    pub fn save_baudrate(&mut self, baudrate: u32) -> Result<()> {
        match self.nvs.set_u32(BAUDRATE_KEY, baudrate) {
            Ok(_) => {
                info!("Baudrate {} saved to flash", baudrate);
                Ok(())
            },
            Err(e) => {
                error!("Failed to save baudrate to NVS: {}", e);
                Err(Error::StorageError(format!("Failed to save baudrate to NVS: {}", e)))
            }
        }
    }

    /// Read the UART baudrate from NVS
    /// Returns None if the baudrate is not found or invalid
    pub fn read_baudrate(&self) -> Option<u32> {
        match self.nvs.get_u32(BAUDRATE_KEY) {
            Ok(Some(baudrate)) => {
                info!("Read baudrate {} from flash", baudrate);
                Some(baudrate)
            },
            Ok(None) => {
                // Key doesn't exist yet
                warn!("No baudrate found in NVS");
                None
            },
            Err(e) => {
                // This is an actual error
                warn!("Error reading baudrate from NVS: {}", e);
                None
            }
        }
    }
}

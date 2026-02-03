//! Network setup and WiFi connection management

// This module just re-exports config constants for use in main
// Network setup is done directly in main.rs for simplicity

// Include generated config
include!(concat!(env!("OUT_DIR"), "/config.rs"));

# MetroTimes3

A real-time Stockholm metro departure display for ESP32-S3 with HUB75 LED matrix.

## Overview

MetroTimes3 is an embedded Rust application that fetches real-time departure information from Stockholm's public transport API (SL) and displays it on a HUB75 LED matrix. The application runs on an ESP32-S3 microcontroller and updates departure times every 30 seconds.

## Features

- Real-time metro departure information from SL (Storstockholms Lokaltrafik)
- HUB75 LED matrix display (64x32 pixels)
- WiFi connectivity with automatic reconnection
- Dual-core architecture: dedicated display refresh on Core 1, data fetching on Core 0
- Configurable via TOML configuration file
- Low-latency display updates with DMA transfers
- Support for filtering by line, direction, and transport type

## Hardware Requirements

- ESP32-S3 development board
- HUB75 LED matrix panel (64x32 recommended)
- Appropriate power supply for LED matrix

### Pin Configuration

The following GPIO pins are used for the HUB75 display:
- GPIO42, GPIO41, GPIO40, GPIO38, GPIO39: Data pins
- GPIO37, GPIO45, GPIO36, GPIO48, GPIO35: Control pins
- GPIO21, GPIO14, GPIO2, GPIO47: Additional control pins

## Software Requirements

- Rust 1.88 or later (specified in `rust-toolchain.toml`)
- ESP32 toolchain and dependencies
- `espflash` for flashing the firmware

## Configuration

1. Copy the sample configuration file:
   ```bash
   cp config.sample.toml config.toml
   ```

2. Edit `config.toml` with your settings:

### WiFi Configuration
```toml
[wifi]
ssid = "YOUR_WIFI_SSID"
password = "YOUR_WIFI_PASSWORD"
```

### API Configuration
```toml
[api]
site_id = 9144  # Your metro station ID (e.g., 9144 for Hammarbyhöjden)
mode = "departures"  # or "arrivals"
transport = "METRO"
direction = 1  # 1 for south, 2 for north
forecast = 60  # Minutes ahead to fetch
request_interval_secs = 30
```

### Finding Your Station ID
Visit https://transport.integration.sl.se/v1/sites to find your station's site ID. You can search for your station name and get the corresponding site_id from the API response.

## Building

Build the project using cargo:

```bash
cargo build --release
```

## Flashing

Flash the firmware to your ESP32-S3:

```bash
cargo run --release
```

Or use `espflash` directly:

```bash
espflash flash target/xtensa-esp32s3-none-elf/release/metrotimes3
```

## Project Structure

```
metrotimes3/
├── src/
│   ├── bin/
│   │   └── main.rs       # Main application entry point
│   ├── api.rs            # SL API client
│   ├── display.rs        # HUB75 display driver and rendering
│   ├── network.rs        # Network configuration
│   ├── types.rs          # Shared type definitions
│   └── lib.rs            # Library root
├── build.rs              # Build script (config generation)
├── config.toml           # Runtime configuration (not in git)
├── config.sample.toml    # Sample configuration
├── Cargo.toml            # Dependencies and build config
└── .clippy.toml          # Clippy lints configuration
```

## Architecture

The application uses a dual-core architecture:

- **Core 0**: Network stack, WiFi management, and API data fetching
- **Core 1**: High-priority display refresh task using DMA

This separation ensures smooth, flicker-free display updates even during network operations.

## Dependencies

Key dependencies include:
- `esp-hal`: ESP32 hardware abstraction layer
- `esp-rtos`: RTOS support with Embassy async runtime
- `embassy-net`: Async networking stack
- `esp-hub75`: HUB75 LED matrix driver
- `reqwless`: Embedded HTTP client with TLS support
- `embedded-graphics`: Graphics primitives

## Development

### Logging

The application uses `esp-println` for logging. Set the log level via environment variables:

```bash
export RUST_LOG=info
cargo run --release
```

### Memory Configuration

The application allocates 73,744 bytes of heap memory from the ESP32-S3's reclaimed RAM region.

## Troubleshooting

### WiFi Connection Issues
- Verify SSID and password in `config.toml`
- Check that your WiFi network is 2.4GHz (ESP32-S3 doesn't support 5GHz)

### Display Issues
- Verify all pin connections to the HUB75 panel
- Ensure adequate power supply for the LED matrix

### API Errors
- Verify your station `site_id` is correct
- Check that the SL API is accessible from your network
- Increase `buffer_size` if responses are larger than expected

## License

[Specify your license here]

## Credits

Built with Rust and the amazing ESP-RS ecosystem.

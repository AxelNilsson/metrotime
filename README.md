# MetroTimes3

A real-time Stockholm metro departure display for ESP32-S3 with HUB75 LED matrix.

## Overview

MetroTimes3 is an embedded Rust application that fetches real-time departure information from Stockholm's public transport API (SL) and displays it on a HUB75 LED matrix. The application runs on an ESP32-S3 microcontroller and updates departure times every 30 seconds.

## Features

- **Real-time metro departure information** from SL (Storstockholms Lokaltrafik)
- **Multi-screen support** - 1 to 3 HUB75 panels (64×32 each)
- **Butter-smooth rendering** with zero frame drops
  - Lock-free channel architecture (no cross-core mutex contention)
  - Double-buffered framebuffers (no render/display blocking)
  - 200Hz display refresh, 100Hz render updates
- **PSRAM enabled** - 2MB external RAM for large API responses and framebuffers
- **Dual-core isolation**
  - Core 0: Network/API/JSON parsing (can stutter without affecting display)
  - Core 1: Display refresh + rendering (consistently smooth)
- **WiFi connectivity** with automatic reconnection
- **Configurable** via TOML configuration file
- **Low-latency DMA** transfers to LED matrix
- Support for filtering by line, direction, and transport type
- **100% safe Rust** - zero `unsafe` blocks!

## Hardware Requirements

- **ESP32-S3 development board** (Matrix Portal S3 recommended)
  - 8MB Flash
  - 2MB PSRAM (required for multi-screen support)
  - 512KB internal SRAM
- **HUB75 LED matrix panels** (64×32 pixels each)
  - Supports 1-3 panels side-by-side
  - Current config: 2 panels (128×32 total display)
- **Appropriate 5V power supply** for LED matrix
  - ~2A per panel recommended

### Pin Configuration (Matrix Portal S3)

The following GPIO pins are used for the HUB75 display:

**RGB Data:**
- RGB1: GPIO42 (R1), GPIO41 (G1), GPIO40 (B1)
- RGB2: GPIO38 (R2), GPIO39 (G2), GPIO37 (B2)

**Address Lines:**
- GPIO45 (A), GPIO36 (B), GPIO48 (C), GPIO35 (D), GPIO21 (E)

**Control:**
- GPIO2 (CLK), GPIO47 (LAT), GPIO14 (OE/BLANK)

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

### Display Configuration
```toml
[display]
num_screens = 2  # Number of 64x32 panels (1, 2, or 3)
rows = 32
cols = 64
scroll_speed_ms = 50  # Scroll speed (lower = faster)
```

### API Configuration
```toml
[api]
site_id = 9144  # Your metro station ID (e.g., 9144 for Hammarbyhöjden)
mode = "departures"  # or "arrivals"
transport = "METRO"
direction = 1  # 1 for south, 2 for north
forecast = 60  # Minutes ahead to fetch
request_interval_secs = 20
buffer_size = 262144  # 256KB buffer for API responses (uses PSRAM)
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

MetroTimes3 uses a sophisticated **three-layer stutter-elimination architecture**:

### Layer 1: Core Isolation
- **Core 0**: Network/WiFi/HTTP/JSON parsing (variable latency, can stutter)
- **Core 1**: Display refresh + rendering (consistent, butter-smooth)

### Layer 2: Lock-Free Channel
- Core 0 → Core 1 data passing via `Channel` (no mutex contention!)
- Core 0: `.send()` when data ready (non-blocking)
- Core 1: `.try_receive()` to check for updates (non-blocking)

### Layer 3: Double Buffering
- **Two framebuffers** with separate mutexes (~200KB each in PSRAM)
- Display refresh reads from **active** buffer (fb0 or fb1)
- Render task writes to **inactive** buffer (fb1 or fb0)
- Atomic swap when render complete (~1 instruction)

**Result:** Zero frame drops, butter-smooth scrolling at all times!

📖 **See [ARCHITECTURE.md](./ARCHITECTURE.md) for detailed diagrams and explanation**

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

The application uses `esp-println` for logging. Set the log level:

```bash
# Info level (default)
ESP_LOG=info cargo run --release

# Debug level (shows frame counts, scroll updates, channel events)
ESP_LOG=debug cargo run --release
```

### Memory Configuration

**PSRAM Enabled (2MB external RAM):**
- Dual framebuffers: ~400KB (200KB × 2)
- API response buffer: 256KB
- Free PSRAM: ~1.35MB (ready for 3rd screen!)

**Internal SRAM (512KB):**
- Heap: 120KB (for WiFi/system)
- Stack/System: ~392KB

**Flash (8MB):**
- Program size: ~633KB
- Free: 7.4MB

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
- **Response too large errors**: Increase `buffer_size` in config.toml (current: 256KB)
- **Out of memory**: Enable PSRAM feature (should be enabled by default)

## License

[Specify your license here]

## Credits

Built with Rust and the amazing ESP-RS ecosystem.

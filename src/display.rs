//! LED Matrix display module using HUB75 interface

use embedded_graphics::prelude::*;
use esp_hal::time::Rate;
use esp_hub75::framebuffer::compute_frame_count;
use esp_hub75::framebuffer::compute_rows;
use esp_hub75::framebuffer::plain::DmaFrameBuffer;
use esp_hub75::{Color, Hub75, Hub75Pins16};
use log::info;

// Include generated config
include!(concat!(env!("OUT_DIR"), "/config.rs"));

// Display configuration
const ROWS: usize = display::ROWS;
const COLS: usize = display::COLS;
const BITS: u8 = 2; // 2-bit color depth (reduced for larger displays to save memory)
const NROWS: usize = compute_rows(ROWS);
const FRAME_COUNT: usize = compute_frame_count(BITS);

// Color constants
const COLOR_RED: Color = Color::new(255, 0, 0);
const COLOR_GREEN: Color = Color::new(0, 255, 0);

pub type DisplayFrameBuffer = DmaFrameBuffer<ROWS, COLS, NROWS, BITS, FRAME_COUNT>;
pub type Hub75Type = Hub75<'static, esp_hal::Blocking>;

// Export display dimensions for use in main
pub const DISPLAY_ROWS: usize = ROWS;
pub const DISPLAY_COLS: usize = COLS;

// Simple 5x7 bitmap font (each character is 5 pixels wide, 7 pixels tall)
// Bit pattern: top to bottom, left to right
const FONT_5X7: &[(&str, [u8; 5])] = &[
    // Digits
    ("0", [0x7E, 0x81, 0x81, 0x81, 0x7E]),
    ("1", [0x00, 0x82, 0xFF, 0x80, 0x00]),
    ("2", [0xC2, 0xA1, 0x91, 0x89, 0x86]),
    ("3", [0x42, 0x81, 0x89, 0x89, 0x76]),
    ("4", [0x18, 0x14, 0x12, 0xFF, 0x10]),
    ("5", [0x4F, 0x89, 0x89, 0x89, 0x71]),
    ("6", [0x7E, 0x89, 0x89, 0x89, 0x72]),
    ("7", [0x01, 0xE1, 0x11, 0x09, 0x07]),
    ("8", [0x76, 0x89, 0x89, 0x89, 0x76]),
    ("9", [0x4E, 0x91, 0x91, 0x91, 0x7E]),
    (" ", [0x00, 0x00, 0x00, 0x00, 0x00]),
    // Uppercase A-Z
    ("A", [0xFC, 0x22, 0x21, 0x22, 0xFC]),
    ("B", [0xFF, 0x89, 0x89, 0x89, 0x76]),
    ("C", [0x7E, 0x81, 0x81, 0x81, 0x42]),
    ("D", [0xFF, 0x81, 0x81, 0x42, 0x3C]),
    ("E", [0xFF, 0x89, 0x89, 0x89, 0x81]),
    ("F", [0xFF, 0x09, 0x09, 0x09, 0x01]),
    ("G", [0x7E, 0x81, 0x89, 0x89, 0x7A]),
    ("H", [0xFF, 0x08, 0x08, 0x08, 0xFF]),
    ("I", [0x00, 0x81, 0xFF, 0x81, 0x00]),
    ("J", [0x40, 0x80, 0x81, 0x7F, 0x01]),
    ("K", [0xFF, 0x08, 0x14, 0x22, 0xC1]),
    ("L", [0xFF, 0x80, 0x80, 0x80, 0x80]),
    ("M", [0xFF, 0x02, 0x0C, 0x02, 0xFF]),
    ("N", [0xFF, 0x04, 0x08, 0x10, 0xFF]),
    ("O", [0x7E, 0x81, 0x81, 0x81, 0x7E]),
    ("P", [0xFF, 0x09, 0x09, 0x09, 0x06]),
    ("Q", [0x7E, 0x81, 0xA1, 0x41, 0xBE]),
    ("R", [0xFF, 0x09, 0x19, 0x29, 0xC6]),
    ("S", [0x46, 0x89, 0x89, 0x89, 0x72]),
    ("T", [0x01, 0x01, 0xFF, 0x01, 0x01]),
    ("U", [0x7F, 0x80, 0x80, 0x80, 0x7F]),
    ("V", [0x1F, 0x60, 0x80, 0x60, 0x1F]),
    ("W", [0xFF, 0x40, 0x30, 0x40, 0xFF]),
    ("X", [0xC3, 0x24, 0x18, 0x24, 0xC3]),
    ("Y", [0x07, 0x08, 0xF0, 0x08, 0x07]),
    ("Z", [0xC1, 0xA1, 0x91, 0x89, 0x87]),
    // Lowercase a-z
    ("a", [0x70, 0x88, 0x88, 0x88, 0xF0]),
    ("b", [0xFF, 0x88, 0x88, 0x88, 0x70]),
    ("c", [0x70, 0x88, 0x88, 0x88, 0x40]),
    ("d", [0x70, 0x88, 0x88, 0x88, 0xFF]),
    ("e", [0x78, 0x94, 0x94, 0x94, 0x98]),
    ("f", [0x08, 0xFE, 0x09, 0x09, 0x02]),
    ("g", [0x4E, 0x91, 0x91, 0x91, 0x7F]),
    ("h", [0xFF, 0x08, 0x08, 0x08, 0xF0]),
    ("i", [0x00, 0x00, 0xFA, 0x00, 0x00]),
    ("j", [0x40, 0x80, 0x80, 0x7A, 0x00]),
    ("k", [0xFF, 0x10, 0x28, 0x44, 0x80]),
    ("l", [0x00, 0x00, 0xFF, 0x00, 0x00]),
    ("m", [0xF8, 0x04, 0xF8, 0x04, 0xF8]),
    ("n", [0xF8, 0x08, 0x08, 0x08, 0xF0]),
    ("o", [0x70, 0x88, 0x88, 0x88, 0x70]),
    ("p", [0xF8, 0x14, 0x14, 0x14, 0x08]),
    ("q", [0x08, 0x14, 0x14, 0x14, 0xF8]),
    ("r", [0xF8, 0x08, 0x04, 0x04, 0x08]),
    ("s", [0x88, 0x94, 0x94, 0x94, 0x64]),
    ("t", [0x04, 0x04, 0x7F, 0x84, 0x84]),
    ("u", [0x78, 0x80, 0x80, 0x80, 0xF8]),
    ("v", [0x38, 0x40, 0x80, 0x40, 0x38]),
    ("w", [0x78, 0x80, 0x60, 0x80, 0x78]),
    ("x", [0xC8, 0x30, 0x30, 0x48, 0x88]),
    ("y", [0x18, 0xA0, 0xA0, 0xA0, 0x78]),
    ("z", [0x88, 0xC8, 0xA8, 0x98, 0x88]),
    // Swedish characters
    ("å", [0x74, 0x8A, 0x8A, 0x8A, 0xF4]),
    ("ä", [0x70, 0x8A, 0x8A, 0x8A, 0xF0]),
    ("ö", [0x70, 0x8A, 0x8A, 0x8A, 0x70]),
    ("Å", [0xFC, 0x22, 0x21, 0x22, 0xFC]),
    ("Ä", [0xFC, 0x26, 0x25, 0x26, 0xFC]),
    ("Ö", [0x7E, 0x85, 0x81, 0x85, 0x7E]),
];

/// Draw a single character at position (x, y)
fn draw_char(fb: &mut DisplayFrameBuffer, c: char, x: i32, y: i32, color: Color) {
    // Find character in font
    let char_data = FONT_5X7.iter().find(|(ch, _)| ch.chars().next() == Some(c));

    if let Some((_, columns)) = char_data {
        for (col_idx, &column) in columns.iter().enumerate() {
            for row_idx in 0..8 {
                if (column & (1 << row_idx)) != 0 {
                    fb.set_pixel(Point::new(x + col_idx as i32, y + row_idx), color);
                }
            }
        }
    }
}

/// Draw text at position (x, y)
fn draw_text(fb: &mut DisplayFrameBuffer, text: &str, x: i32, y: i32, color: Color) {
    let mut cursor_x = x;
    for c in text.chars() {
        draw_char(fb, c, cursor_x, y, color);
        cursor_x += 6; // 5 pixel width + 1 pixel spacing
    }
}

/// Determine color based on departure time (red if 5 min or less, otherwise green)
fn get_time_color(time: &str) -> Color {
    // Check if time is "Nu" (Now)
    if time.trim() == "Nu" {
        return COLOR_RED;
    }

    // Check if time is in "X min" format
    if let Some(min_str) = time.strip_suffix(" min") {
        if let Ok(minutes) = min_str.trim().parse::<u32>() {
            if minutes <= 5 {
                return COLOR_RED;
            }
        }
    }
    COLOR_GREEN
}

/// Initializes the HUB75 LED matrix display with Matrix Portal S3 pin configuration.
///
/// # Arguments
/// * `lcd_cam` - LCD_CAM peripheral for driving the display
/// * `dma_channel` - DMA channel for data transfer
/// * `gpio*` - GPIO pins for HUB75 interface (RGB1, RGB2, Address lines, Control signals)
///
/// # Returns
/// * `Ok(Hub75)` - Initialized display driver
/// * `Err(&str)` - Error message if initialization fails
pub fn init_display<'d>(
    lcd_cam: esp_hal::peripherals::LCD_CAM<'d>,
    dma_channel: esp_hal::peripherals::DMA_CH0<'d>,
    gpio42: impl esp_hal::gpio::OutputPin + 'd,
    gpio41: impl esp_hal::gpio::OutputPin + 'd,
    gpio40: impl esp_hal::gpio::OutputPin + 'd,
    gpio38: impl esp_hal::gpio::OutputPin + 'd,
    gpio39: impl esp_hal::gpio::OutputPin + 'd,
    gpio37: impl esp_hal::gpio::OutputPin + 'd,
    gpio45: impl esp_hal::gpio::OutputPin + 'd,
    gpio36: impl esp_hal::gpio::OutputPin + 'd,
    gpio48: impl esp_hal::gpio::OutputPin + 'd,
    gpio35: impl esp_hal::gpio::OutputPin + 'd,
    gpio21: impl esp_hal::gpio::OutputPin + 'd,
    gpio14: impl esp_hal::gpio::OutputPin + 'd,
    gpio2: impl esp_hal::gpio::OutputPin + 'd,
    gpio47: impl esp_hal::gpio::OutputPin + 'd,
) -> Result<Hub75<'d, esp_hal::Blocking>, &'static str> {
    info!("Initializing HUB75 LED matrix display...");

    // Matrix Portal S3 pin configuration
    // RGB1: 42, 41, 40 (R1, G1, B1)
    // RGB2: 38, 39, 37 (R2, G2, B2)
    // Address: 45, 36, 48, 35, 21 (A, B, C, D, E)
    // Control: 2 (CLK), 47 (LAT), 14 (OE/BLANK)

    let pins = Hub75Pins16 {
        red1: gpio42.degrade(),
        grn1: gpio41.degrade(),
        blu1: gpio40.degrade(),
        red2: gpio38.degrade(),
        grn2: gpio39.degrade(),
        blu2: gpio37.degrade(),
        addr0: gpio45.degrade(),
        addr1: gpio36.degrade(),
        addr2: gpio48.degrade(),
        addr3: gpio35.degrade(),
        addr4: gpio21.degrade(),
        blank: gpio14.degrade(),
        clock: gpio2.degrade(),
        latch: gpio47.degrade(),
    };

    // Allocate DMA descriptors
    let (_, tx_descriptors) =
        esp_hal::dma_descriptors!(0, DisplayFrameBuffer::dma_buffer_size_bytes());

    // Create Hub75 driver using LCD_CAM peripheral (blocking mode)
    let hub75 = Hub75::new(
        lcd_cam,
        pins,
        dma_channel,
        tx_descriptors,
        Rate::from_mhz(20),
    )
    .map_err(|_| "Failed to initialize Hub75")?;

    info!("HUB75 display initialized successfully");
    Ok(hub75)
}

/// Draws a loading message on the display.
///
/// # Arguments
/// * `fb` - The framebuffer to draw to
pub fn draw_loading(fb: &mut DisplayFrameBuffer) {
    draw_text(fb, "LOADING", 2, 12, COLOR_GREEN);
}

/// Draws a "no departures" message on the display.
///
/// # Arguments
/// * `fb` - The framebuffer to draw to
pub fn draw_no_departures(fb: &mut DisplayFrameBuffer) {
    draw_text(fb, "NO DEPARTURES", 2, 12, COLOR_GREEN);
}

/// Draws metro departure information on the LED display.
///
/// # Arguments
/// * `fb` - The framebuffer to draw to
/// * `departures` - Array of (line, destination, time) tuples
/// * `scroll_offset` - Horizontal scroll offset in pixels for the bottom row
///
/// # Layout
/// - Top row: First upcoming departure (line, destination on left; time on right)
/// - Bottom row: Remaining departures scrolling continuously
/// - Color: Red if departure is ≤5 minutes or "Nu" (now), green otherwise
pub fn draw_departures(
    fb: &mut DisplayFrameBuffer,
    departures: &[(
        heapless::String<8>,
        heapless::String<32>,
        heapless::String<16>,
    )], // (line, destination, time)
    scroll_offset: i32, // Horizontal scroll offset for bottom row
) {
    // DON'T erase - background is already black from caller
    // Draw text in GREEN using custom bitmap font

    if departures.is_empty() {
        // This should not be called with empty departures
        // Use draw_loading() or draw_no_departures() instead
        return;
    } else {
        // Show first departure
        let (line, dest, time) = &departures[0];

        // Check if we have enough space for single-line layout (dual screen or more)
        // Each char is 6 pixels wide (5 + 1 spacing), need ~100+ pixels for single line
        let single_line_layout = COLS >= 100;

        if single_line_layout {
            // Dual/triple screen: show first departure on row 1, rest scrolling on row 2
            let mut line1 = heapless::String::<32>::new();
            line1.push_str(line.as_str()).ok();
            line1.push(' ').ok();

            // Destination can be longer on dual screen (up to ~10 chars)
            let max_dest_len = 10;
            if dest.len() > max_dest_len {
                line1.push_str(&dest.as_str()[..max_dest_len]).ok();
            } else {
                line1.push_str(dest.as_str()).ok();
            }

            // Draw first departure: line and destination at the left, time at the right
            // Use red for both station and time if departure is 5 min or less
            let departure_color = get_time_color(time.as_str());
            draw_text(fb, line1.as_str(), 2, 5, departure_color);

            let time_width = time.len() as i32 * 6;
            let time_x = (COLS as i32) - time_width - 2;
            draw_text(fb, time.as_str(), time_x, 5, departure_color);

            // Draw bottom row: draw each departure with appropriate time color
            if departures.len() > 1 {
                // Calculate total width for the loop
                let mut total_text_width = 0i32;
                for i in 1..departures.len() {
                    let (line_n, dest_n, time_n) = &departures[i];
                    let entry_len = line_n.len() + 1 + dest_n.len() + 1 + time_n.len();
                    total_text_width += entry_len as i32 * 6;
                    if i > 1 {
                        total_text_width += 12; // Separator spacing
                    }
                }

                // Draw departures twice for seamless loop
                for loop_iteration in 0..2 {
                    let base_x = if loop_iteration == 0 {
                        (COLS as i32) - scroll_offset
                    } else {
                        (COLS as i32) - scroll_offset + total_text_width + COLS as i32
                    };

                    let mut cursor_x = base_x;

                    for i in 1..departures.len() {
                        let (line_n, dest_n, time_n) = &departures[i];

                        // Add separator between departures
                        if i > 1 {
                            cursor_x += 12; // "  " spacing
                        }

                        // Get color for this departure (red if 5 min or less, green otherwise)
                        let departure_color = get_time_color(time_n.as_str());

                        // Draw line and destination
                        let mut line_dest = heapless::String::<64>::new();
                        line_dest.push_str(line_n.as_str()).ok();
                        line_dest.push(' ').ok();
                        line_dest.push_str(dest_n.as_str()).ok();
                        line_dest.push(' ').ok();
                        draw_text(fb, line_dest.as_str(), cursor_x, 20, departure_color);
                        cursor_x += line_dest.len() as i32 * 6;

                        // Draw time in same color
                        draw_text(fb, time_n.as_str(), cursor_x, 20, departure_color);
                        cursor_x += time_n.len() as i32 * 6;
                    }
                }
            }
        } else {
            // Single screen: show "17 Dest" on line 1, "2 min" on line 2
            let mut line1 = heapless::String::<16>::new();
            line1.push_str(line.as_str()).ok();
            line1.push(' ').ok();

            // Truncate destination to fit (about 7-8 chars max)
            let max_dest_len = 8;
            if dest.len() > max_dest_len {
                line1.push_str(&dest.as_str()[..max_dest_len]).ok();
            } else {
                line1.push_str(dest.as_str()).ok();
            }

            draw_text(fb, line1.as_str(), 2, 8, COLOR_GREEN);
            draw_text(fb, time.as_str(), 2, 20, COLOR_GREEN);
        }
    }
}

/// Draws a simple test pattern to verify the display is working correctly.
///
/// Fills the entire display with dim green, useful for hardware testing.
///
/// # Arguments
/// * `fb` - The framebuffer to draw the test pattern to
pub fn draw_test_pattern(fb: &mut DisplayFrameBuffer) {
    // Start with all green - easiest to see
    for y in 0..ROWS as i32 {
        for x in 0..COLS as i32 {
            fb.set_pixel(Point::new(x, y), Color::new(0, 64, 0)); // Dim green
        }
    }
}

# Character Bitmap Font Documentation

This document describes how we created the 5x7 bitmap font used in the LED matrix display.

## Overview

The font is a custom 5x7 pixel bitmap font designed for the 64x32 LED matrix display. Each character consists of 5 columns (width) and 7 rows (height), stored as an array of 5 bytes.

## Data Structure

Located in `src/display.rs:23-97`, the font is defined as:

```rust
const FONT_5X7: &[(&str, [u8; 5])] = &[
    ("0", [0x7E, 0x81, 0x81, 0x81, 0x7E]),
    ("1", [0x00, 0x82, 0xFF, 0x80, 0x00]),
    // ... more characters
];
```

Each entry is a tuple containing:
- Character string (supports multi-byte UTF-8 characters like "å")
- Array of 5 bytes representing the character bitmap

## Bitmap Encoding

### Column-Major Format
Each character is stored in **column-major** order:
- Each byte represents one **vertical column** (left to right)
- Each bit in the byte represents one **pixel** in that column (bottom to top)

### Bit Mapping
Within each byte (column):
- **Bit 0** (LSB) = Top pixel (row 0)
- **Bit 1** = Row 1
- **Bit 2** = Row 2
- **Bit 3** = Row 3
- **Bit 4** = Row 4
- **Bit 5** = Row 5
- **Bit 6** = Row 6
- **Bit 7** (MSB) = Bottom pixel (row 7, usually unused in 5x7 font)

### Example: Character "0"

```
Hex values: [0x7E, 0x81, 0x81, 0x81, 0x7E]
Binary:     [01111110, 10000001, 10000001, 10000001, 01111110]
```

Visual representation (1 = lit pixel, 0 = off):
```
     Col: 0  1  2  3  4
Row 0:    0  1  1  1  0
Row 1:    1  0  0  0  1
Row 2:    1  0  0  0  1
Row 3:    1  0  0  0  1
Row 4:    1  0  0  0  1
Row 5:    1  0  0  0  1
Row 6:    1  0  0  0  1
Row 7:    0  0  0  0  0
```

This creates the outline of the number "0".

## Character Set

The font includes:

### Digits (0-9)
```rust
("0", [0x7E, 0x81, 0x81, 0x81, 0x7E]),
("1", [0x00, 0x82, 0xFF, 0x80, 0x00]),
("2", [0xC2, 0xA1, 0x91, 0x89, 0x86]),
// ... etc
```

### Uppercase Letters (A-Z)
```rust
("A", [0xFC, 0x22, 0x21, 0x22, 0xFC]),
("B", [0xFF, 0x89, 0x89, 0x89, 0x76]),
// ... etc
```

### Lowercase Letters (a-z)
```rust
("a", [0x70, 0x88, 0x88, 0x88, 0xF0]),
("b", [0xFF, 0x88, 0x88, 0x88, 0x70]),
// ... etc
```

### Swedish Characters (åäöÅÄÖ)
```rust
("å", [0x74, 0x8A, 0x8A, 0x8A, 0xF4]),
("ä", [0x70, 0x8A, 0x8A, 0x8A, 0xF0]),
("ö", [0x70, 0x8A, 0x8A, 0x8A, 0x70]),
("Å", [0xFC, 0x22, 0x21, 0x22, 0xFC]),
("Ä", [0xFC, 0x26, 0x25, 0x26, 0xFC]),
("Ö", [0x7E, 0x85, 0x81, 0x85, 0x7E]),
```

### Special Characters
```rust
(" ", [0x00, 0x00, 0x00, 0x00, 0x00]),  // Space
```

## Rendering Implementation

The font is rendered by the `draw_char()` function in `src/display.rs:100-113`:

```rust
fn draw_char(fb: &mut DisplayFrameBuffer, c: char, x: i32, y: i32, color: Color) {
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
```

The algorithm:
1. Find the character in the font table
2. Iterate through each of the 5 columns
3. For each column, test bits 0-7 (rows)
4. If a bit is set, draw a pixel at position (x + column, y + row)

## Text Spacing

Characters are spaced 6 pixels apart (5 pixels for character width + 1 pixel spacing) as implemented in `draw_text()` at `src/display.rs:116-122`.

## How the Bitmaps Were Created

The bitmap values were created manually by:

1. **Designing the character** on a 5x7 grid (on paper or using a pixel editor)
2. **Converting columns to binary**:
   - For each of the 5 columns (left to right)
   - Read the pixels from top to bottom
   - Create an 8-bit binary number where 1 = lit pixel, 0 = off
3. **Converting to hexadecimal**:
   - Convert each 8-bit binary number to hex (0x00 to 0xFF)
4. **Testing on the display** to verify legibility

### Example Process for "A":

1. Design on grid:
```
  X X X X
  X     X
  X     X
  X X X X
  X     X
  X     X
  X     X
```

2. Extract columns (5 wide):
```
Col 0: 11111100 = 0xFC
Col 1: 00100010 = 0x22
Col 2: 00100001 = 0x21
Col 3: 00100010 = 0x22
Col 4: 11111100 = 0xFC
```

3. Result: `[0xFC, 0x22, 0x21, 0x22, 0xFC]`

## Display Hardware

The font is rendered on:
- **64x32 LED matrix** (HUB75 interface)
- **Adafruit Matrix Portal S3** controller
- **4-bit color depth** (16 brightness levels per RGB channel)
- **Green color** for text (RGB: 0, 255, 0)

## Usage in Application

The font is used to display:
- Metro line numbers (e.g., "17")
- Destination names (e.g., "Alvik")
- Departure times (e.g., "2 min")
- Status messages (e.g., "no departures")

Text is drawn at specific positions:
- Line 1 (y=8): Line number and destination
- Line 2 (y=20): Departure time

See `draw_departures()` in `src/display.rs:187-221` for the complete implementation.

use {Location, UIButton};
use byteorder::{NativeEndian, WriteBytesExt};
use image::{ImageBuffer, Rgba};
use std::io;

const DECORATION_SIZE: i32 = 8;
const DECORATION_TOP_SIZE: i32 = 32;

/// Compute on which part of the window given point falls
pub(crate) fn compute_location((x, y): (f64, f64), (w, h): (i32, i32)) -> Location {
    if y <= DECORATION_TOP_SIZE as f64 {
        // we are in the top part
        if x <= DECORATION_SIZE as f64 {
            Location::TopLeft
        } else if x <= (w + DECORATION_SIZE) as f64 {
            if y <= DECORATION_SIZE as f64 {
                Location::Top
            } else {
                // check for buttons
                if (w >= 24) && (x > (w + DECORATION_SIZE - 24) as f64)
                    && (x <= (w + DECORATION_SIZE) as f64) && (y > DECORATION_SIZE as f64)
                    && (y <= (DECORATION_SIZE + 16) as f64)
                {
                    Location::Button(UIButton::Close)
                } else if (w >= 56) && (x > (w + DECORATION_SIZE - 56) as f64)
                    && (x <= (w + DECORATION_SIZE - 32) as f64)
                    && (y > DECORATION_SIZE as f64)
                    && (y <= (DECORATION_SIZE + 16) as f64)
                {
                    Location::Button(UIButton::Maximize)
                } else if (w >= 88) && (x > (w + DECORATION_SIZE - 88) as f64)
                    && (x <= (w + DECORATION_SIZE - 64) as f64)
                    && (y > DECORATION_SIZE as f64)
                    && (y <= (DECORATION_SIZE + 16) as f64)
                {
                    Location::Button(UIButton::Minimize)
                } else {
                    Location::TopBar
                }
            }
        } else {
            Location::TopRight
        }
    } else if y <= (DECORATION_TOP_SIZE + h) as f64 {
        if x <= DECORATION_SIZE as f64 {
            Location::Left
        } else if x <= (w + DECORATION_SIZE) as f64 {
            Location::Inside
        } else {
            Location::Right
        }
    } else {
        if x <= DECORATION_SIZE as f64 {
            Location::BottomLeft
        } else if x <= (w + DECORATION_SIZE) as f64 {
            Location::Bottom
        } else {
            Location::BottomRight
        }
    }
}

/// Offset at which the contents should be drawn relative to the top-left
/// corner of the decorations
pub(crate) fn subsurface_offset() -> (i32, i32) {
    (DECORATION_SIZE, DECORATION_TOP_SIZE)
}

/// Subtracts the border dimensions from the given dimensions.
pub fn subtract_borders(width: i32, height: i32) -> (i32, i32) {
    (
        width - 2 * (DECORATION_SIZE as i32),
        height - DECORATION_SIZE as i32 - DECORATION_TOP_SIZE as i32,
    )
}

/// Adds the border dimensions to the given dimensions.
pub fn add_borders(width: i32, height: i32) -> (i32, i32) {
    (
        width + 2 * (DECORATION_SIZE as i32),
        height + DECORATION_SIZE as i32 + DECORATION_TOP_SIZE as i32,
    )
}

/// Total number of pixels of the rectangle containing the whole
/// decorated window
pub(crate) fn pxcount(w: i32, h: i32) -> i32 {
    (w + 2 * DECORATION_SIZE) * (h + DECORATION_SIZE + DECORATION_TOP_SIZE)
}

/// Draw the decorations on the rectangle
///
/// Actual contents of the window will be drawn on top
pub(crate) fn draw_contents<W: io::Write>(mut to: W, w: u32, h: u32, activated: bool, maximized: bool, maximizable: bool,
                                          ptr_location: Location)
                                          -> io::Result<()> {
    let drawn = draw_image(w, h, activated, maximized, maximizable, ptr_location);
    for p in drawn.pixels() {
        let val = ((p.data[3] as u32) << 24) // A
                + ((p.data[0] as u32) << 16) // R
                + ((p.data[1] as u32) << 8 ) // G
                + (p.data[2] as u32); // B
        to.write_u32::<NativeEndian>(val)?;
    }
    Ok(())
}

fn draw_image(w: u32, h: u32, activated: bool, maximized: bool, maximizable: bool, ptr_location: Location)
              -> ImageBuffer<Rgba<u8>, Vec<u8>> {
    let ds = DECORATION_SIZE as u32;
    let dts = DECORATION_TOP_SIZE as u32;
    let mut canvas = ImageBuffer::<Rgba<u8>, Vec<u8>>::new(w + 2 * ds, h + ds + dts);
    // draw the borders
    let border_rectangles = [
        (0, 0, w + 2 * ds, dts),      // top rectangle
        (0, dts, ds, h),              // left rectangle
        (w + ds, dts, ds, h),         // right rectangle
        (0, h + dts, w + 2 * ds, ds), // bottom rectangle
    ];

    // fill these rectangles with grey
    let border_color = if activated {
        [0x80, 0x80, 0x80, 0xFF]
    } else {
        [0x60, 0x60, 0x60, 0xFF]
    };
    for &(x, y, w, h) in &border_rectangles {
        for xx in x..(x + w) {
            for yy in y..(y + h) {
                canvas.put_pixel(xx, yy, Rgba { data: border_color });
            }
        }
    }

    // draw the red close button
    if w >= 24 {
        let button_color = if let Location::Button(UIButton::Close) = ptr_location {
            [0xFF, 0x40, 0x40, 0xFF]
        } else {
            [0xB0, 0x40, 0x40, 0xFF]
        };
        for xx in (w + ds - 24)..(w + ds) {
            for yy in ds..(ds + 16) {
                canvas.put_pixel(xx, yy, Rgba { data: button_color });
            }
        }
    }

    // draw the yellow maximize button
    if w >= 56 {
        let button_color = if maximizable {
            if let Location::Button(UIButton::Maximize) = ptr_location {
                [0xFF, 0xFF, 0x40, 0xFF]
            } else {
                [0xB0, 0xB0, 0x40, 0xFF]
            }
        } else {
            [0x80, 0x80, 0x20, 0xFF]
        };
        for xx in (w + ds - 56)..(w + ds - 32) {
            for yy in ds..(ds + 16) {
                canvas.put_pixel(xx, yy, Rgba { data: button_color });
            }
        }
    }

    // draw the green minimize button
    if w >= 88 {
        let button_color = if let Location::Button(UIButton::Minimize) = ptr_location {
            [0x40, 0xFF, 0x40, 0xFF]
        } else {
            [0x40, 0xB0, 0x40, 0xFF]
        };
        for xx in (w + ds - 88)..(w + ds - 64) {
            for yy in ds..(ds + 16) {
                canvas.put_pixel(xx, yy, Rgba { data: button_color });
            }
        }
    }
    // end
    canvas
}

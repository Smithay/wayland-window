use Location;
use byteorder::{NativeEndian, WriteBytesExt};
use std::io;

const DECORATION_SIZE: i32 = 8;
const DECORATION_TOP_SIZE: i32 = 24;

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
                Location::TopBar
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
pub(crate) fn draw_contents<W: io::Write>(mut to: W, w: i32, h: i32, _maximized: bool,
                                          _ptr_location: Location)
                                          -> io::Result<()> {
    // TODO: draw something better than just gray borders
    let pxcount = pxcount(w, h);
    for _ in 0..pxcount {
        to.write_u32::<NativeEndian>(0xFF444444)?;
    }
    Ok(())
}

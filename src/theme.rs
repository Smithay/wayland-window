use {Location, UIButton};

const DECORATION_SIZE: i32 = 8;
const DECORATION_TOP_SIZE: i32 = 32;

#[cfg(target_endian = "little")]
macro_rules! auto_endian(
    ($a: expr, $r: expr, $g: expr, $b: expr) => {
        [$b, $g, $r, $a]
    }
);

#[cfg(target_endian = "big")]
macro_rules! auto_endian(
    ($a: expr, $r: expr, $g: expr, $b: expr) => {
        [$a, $r, $g, $b]
    }
);

// defining the color scheme
const INACTIVE_BORDER: [u8; 4] = auto_endian!(0xFF, 0x60, 0x60, 0x60);
const ACTIVE_BORDER: [u8; 4] = auto_endian!(0xFF, 0x80, 0x80, 0x80);
const RED_BUTTON_REGULAR: [u8; 4] = auto_endian!(0xFF, 0xB0, 0x40, 0x40);
const RED_BUTTON_HOVER: [u8; 4] = auto_endian!(0xFF, 0xFF, 0x40, 0x40);
const GREEN_BUTTON_REGULAR: [u8; 4] = auto_endian!(0xFF, 0x40, 0xB0, 0x40);
const GREEN_BUTTON_HOVER: [u8; 4] = auto_endian!(0xFF, 0x40, 0xFF, 0x40);
const YELLOW_BUTTON_REGULAR: [u8; 4] = auto_endian!(0xFF, 0xB0, 0xB0, 0x40);
const YELLOW_BUTTON_HOVER: [u8; 4] = auto_endian!(0xFF, 0xFF, 0xFF, 0x40);
const YELLOW_BUTTON_DISABLED: [u8; 4] = auto_endian!(0xFF, 0x80, 0x80, 0x20);

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
                if (w >= 24) && (x > (w + DECORATION_SIZE - 24) as f64) && (x <= (w + DECORATION_SIZE) as f64)
                    && (y > DECORATION_SIZE as f64) && (y <= (DECORATION_SIZE + 16) as f64)
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
pub(crate) fn draw_contents(canvas: &mut [u8], w: u32, h: u32, activated: bool, _maximized: bool,
                            maximizable: bool, ptr_location: Location) {
    let ds = DECORATION_SIZE as u32;
    let dts = DECORATION_TOP_SIZE as u32;
    let mut canvas = Canvas::new(w + 2 * ds, h + ds + dts, canvas);
    // draw the borders
    let border_rectangles = [
        (0, 0, w + 2 * ds, dts+1),      // top rectangle
        (0, dts, ds+1, h),              // left rectangle
        (w + ds-1, dts, ds+1, h),         // right rectangle
        (0, h + dts-1, w + 2 * ds, ds+1), // bottom rectangle
    ];

    // We've built an ImageBuffer from a raw &[u8] buffer, and the wayland spec
    // explicitly says we should use native endiannes
    // also we're doing ARGB (while image expects RGBA), though as long as we
    // only blit pixels it's not very important

    // fill these rectangles with grey
    let border_color = if activated {
        ACTIVE_BORDER
    } else {
        INACTIVE_BORDER
    };
    for &(x, y, w, h) in &border_rectangles {
        for xx in x..(x + w) {
            for yy in y..(y + h) {
                canvas.put_pixel(xx, yy, border_color);
            }
        }
    }

    // draw the red close button
    if w >= 24 {
        let button_color = if let Location::Button(UIButton::Close) = ptr_location {
            RED_BUTTON_HOVER
        } else {
            RED_BUTTON_REGULAR
        };
        for xx in (w + ds - 24)..(w + ds) {
            for yy in ds..(ds + 16) {
                canvas.put_pixel(xx, yy, button_color);
            }
        }
    }

    // draw the yellow maximize button
    if w >= 56 {
        let button_color = if maximizable {
            if let Location::Button(UIButton::Maximize) = ptr_location {
                YELLOW_BUTTON_HOVER
            } else {
                YELLOW_BUTTON_REGULAR
            }
        } else {
            YELLOW_BUTTON_DISABLED
        };
        for xx in (w + ds - 56)..(w + ds - 32) {
            for yy in ds..(ds + 16) {
                canvas.put_pixel(xx, yy, button_color);
            }
        }
    }

    // draw the green minimize button
    if w >= 88 {
        let button_color = if let Location::Button(UIButton::Minimize) = ptr_location {
            GREEN_BUTTON_HOVER
        } else {
            GREEN_BUTTON_REGULAR
        };
        for xx in (w + ds - 88)..(w + ds - 64) {
            for yy in ds..(ds + 16) {
                canvas.put_pixel(xx, yy, button_color);
            }
        }
    }
}

struct Canvas<'a> {
    width: u32,
    contents: &'a mut [u8]
}

impl<'a> Canvas<'a> {
    fn new(width: u32, height: u32, contents: &mut[u8]) -> Canvas {
        debug_assert!(contents.len() == (width*height*4) as usize);
        Canvas { width, contents }
    }

    #[inline]
    fn put_pixel(&mut self, x: u32, y: u32, val: [u8; 4]) {
        let idx = ((y*self.width + x)*4) as usize;
        self.contents[idx + 0] = val[0];
        self.contents[idx + 1] = val[1];
        self.contents[idx + 2] = val[2];
        self.contents[idx + 3] = val[3];
    }
}
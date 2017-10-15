use super::themed_pointer::ThemedPointer;
use byteorder::{NativeEndian, WriteBytesExt};
use shell::{self, Shell};
use std::cell::RefCell;
use std::cmp::max;
use std::fs::File;
use std::io::{Seek, SeekFrom, Write};
use std::os::unix::io::AsRawFd;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use tempfile::tempfile;
use wayland_client::{EventQueueHandle, Proxy};
use wayland_client::protocol::{wl_buffer, wl_compositor, wl_output, wl_pointer, wl_seat, wl_shell_surface,
                               wl_shm, wl_shm_pool, wl_subcompositor, wl_subsurface, wl_surface};

// The surfaces handling the borders, 8 total, are organised this way:
//
//        0
// ---|-------|---
//    |       |
//  3 | user  | 1
//    |       |
// ---|-------|---
//        2
//
pub const BORDER_TOP: usize = 0;
pub const BORDER_RIGHT: usize = 1;
pub const BORDER_BOTTOM: usize = 2;
pub const BORDER_LEFT: usize = 3;

const DECORATION_SIZE: i32 = 8;
const DECORATION_TOP_SIZE: i32 = 24;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum PtrLocation {
    None,
    Top,
    Right,
    Bottom,
    Left,
}

enum Pointer {
    Plain(wl_pointer::WlPointer),
    Themed(ThemedPointer),
    None,
}

#[derive(Copy, Clone)]
pub(crate) struct SurfaceMetadata {
    dimensions: (i32, i32),
    decorate: bool,
    min_size: Option<(i32, i32)>,
    max_size: Option<(i32, i32)>,
}

pub(crate) struct PointerState {
    surfaces: Vec<wl_surface::WlSurface>,
    location: PtrLocation,
    coordinates: (f64, f64),
    cornered: bool,
    topped: bool,
    pointer: Pointer,
    shell_surface: shell::Surface,
    seat: Option<wl_seat::WlSeat>,
    meta: Arc<Mutex<SurfaceMetadata>>,
}

impl PointerState {
    fn pointer_entered(&mut self, surface: &wl_surface::WlSurface, serial: u32) {
        if self.surfaces[BORDER_TOP].equals(surface) {
            self.location = PtrLocation::Top;
        } else if self.surfaces[BORDER_RIGHT].equals(surface) {
            self.location = PtrLocation::Right
        } else if self.surfaces[BORDER_BOTTOM].equals(surface) {
            self.location = PtrLocation::Bottom;
        } else if self.surfaces[BORDER_LEFT].equals(surface) {
            self.location = PtrLocation::Left
        } else {
            // A surface that we don't manage
            self.location = PtrLocation::None;
            return;
        }
        self.update(Some(serial), true);
    }

    fn pointer_left(&mut self, serial: u32) {
        self.location = PtrLocation::None;
        self.change_pointer("left_ptr", Some(serial))
    }

    fn update(&mut self, serial: Option<u32>, force: bool) {
        let old_cornered = self.cornered;
        let surface_width = self.meta.lock().unwrap().dimensions.0;
        self.cornered = (self.location == PtrLocation::Top || self.location == PtrLocation::Bottom)
            && (self.coordinates.0 <= DECORATION_SIZE as f64
                || self.coordinates.0 >= (surface_width + DECORATION_SIZE) as f64);
        let old_topped = self.topped;
        self.topped = self.location == PtrLocation::Top && self.coordinates.1 <= DECORATION_SIZE as f64;
        if force || (self.cornered ^ old_cornered) || (old_topped ^ self.topped) {
            let name = if self.cornered {
                match self.location {
                    PtrLocation::Top => if self.coordinates.0 <= DECORATION_SIZE as f64 {
                        "top_left_corner"
                    } else {
                        "top_right_corner"
                    },
                    PtrLocation::Bottom => if self.coordinates.0 <= DECORATION_SIZE as f64 {
                        "bottom_left_corner"
                    } else {
                        "bottom_right_corner"
                    },
                    _ => unreachable!(),
                }
            } else {
                match self.location {
                    PtrLocation::Top => if self.topped {
                        "top_side"
                    } else {
                        "left_ptr"
                    },
                    PtrLocation::Bottom => "bottom_side",
                    PtrLocation::Right => "right_side",
                    PtrLocation::Left => "left_side",
                    _ => "left_ptr",
                }
            };
            self.change_pointer(name, serial)
        }
    }

    fn change_pointer(&self, name: &str, serial: Option<u32>) {
        if let Pointer::Themed(ref themed) = self.pointer {
            themed.set_cursor(name, serial);
        }
    }

    pub(crate) fn clamp_to_limits(&self, size: (i32, i32)) -> (i32, i32) {
        use std::cmp::{max, min};
        let (mut w, mut h) = size;
        let meta = self.meta.lock().unwrap().clone();
        if meta.decorate {
            let (ww, hh) = subtract_borders(w, h);
            w = ww;
            h = hh;
        }
        if let Some((minw, minh)) = meta.min_size {
            w = max(minw, w);
            h = max(minh, h);
        }
        if let Some((maxw, maxh)) = meta.max_size {
            w = min(maxw, w);
            h = min(maxh, h);
        }
        (w, h)
    }
}


/// A wrapper for a decorated surface.
///
/// This is the main object of this crate. It handles the drawing of
/// minimalistic borders allowing the resizing and moving of the window.
/// See the root documentation of this crate for explanations about how to use it.
pub struct DecoratedSurface {
    shell_surface: shell::Surface,
    border_subsurfaces: Vec<wl_subsurface::WlSubsurface>,
    border_surfaces: Vec<wl_surface::WlSurface>,
    buffers: Vec<wl_buffer::WlBuffer>,
    tempfile: File,
    pool: wl_shm_pool::WlShmPool,
    meta: Arc<Mutex<SurfaceMetadata>>,
    buffer_capacity: usize,
}

impl DecoratedSurface {
    /// Resizes the borders to given width and height.
    ///
    /// These values should be the dimentions of the internal surface of the
    /// window (the decorated window will thus be a little larger).
    pub fn resize(&mut self, width: i32, height: i32) {
        // flush buffers
        for b in self.buffers.drain(..) {
            b.destroy();
        }

        self.meta.lock().unwrap().dimensions = (width, height);

        // skip if not decorating
        if !self.meta.lock().unwrap().decorate {
            for s in &self.border_surfaces {
                s.attach(None, 0, 0);
                s.commit();
            }
            return;
        }

        // actually update the decorations
        let new_pxcount = max(
            DECORATION_TOP_SIZE * (DECORATION_SIZE * 2 + width),
            max(DECORATION_TOP_SIZE * width, DECORATION_SIZE * height),
        ) as usize;
        if new_pxcount * 4 > self.buffer_capacity {
            // reallocation needed !
            self.tempfile.set_len((new_pxcount * 4) as u64).unwrap();
            self.pool.resize((new_pxcount * 4) as i32);
            self.buffer_capacity = new_pxcount * 4;
        }
        // rewrite the data
        self.tempfile.seek(SeekFrom::Start(0)).unwrap();
        for _ in 0..(new_pxcount * 4) {
            // write a dark gray
            let _ = self.tempfile.write_u32::<NativeEndian>(0xFF444444);
        }
        self.tempfile.flush().unwrap();
        // resize the borders
        // top
        {
            let buffer = self.pool
                .create_buffer(
                    0,
                    width as i32 + (DECORATION_SIZE as i32) * 2,
                    DECORATION_TOP_SIZE as i32,
                    (width as i32 + (DECORATION_SIZE as i32) * 2) * 4,
                    wl_shm::Format::Argb8888,
                )
                .expect("Pool was destroyed!");
            self.border_surfaces[BORDER_TOP].attach(Some(&buffer), 0, 0);
            self.border_subsurfaces[BORDER_TOP]
                .set_position(-(DECORATION_SIZE as i32), -(DECORATION_TOP_SIZE as i32));
            self.buffers.push(buffer);
        }
        // right
        {
            let buffer = self.pool
                .create_buffer(
                    0,
                    DECORATION_SIZE as i32,
                    height as i32,
                    (DECORATION_SIZE * 4) as i32,
                    wl_shm::Format::Argb8888,
                )
                .expect("Pool was destroyed!");
            self.border_surfaces[BORDER_RIGHT].attach(Some(&buffer), 0, 0);
            self.border_subsurfaces[BORDER_RIGHT].set_position(width as i32, 0);
            self.buffers.push(buffer);
        }
        // bottom
        {
            let buffer = self.pool
                .create_buffer(
                    0,
                    width as i32 + (DECORATION_SIZE as i32) * 2,
                    DECORATION_SIZE as i32,
                    (width as i32 + (DECORATION_SIZE as i32) * 2) * 4,
                    wl_shm::Format::Argb8888,
                )
                .expect("Pool was destroyed!");
            self.border_surfaces[BORDER_BOTTOM].attach(Some(&buffer), 0, 0);
            self.border_subsurfaces[BORDER_BOTTOM]
                .set_position(-(DECORATION_SIZE as i32), height as i32);
            self.buffers.push(buffer);
        }
        // left
        {
            let buffer = self.pool
                .create_buffer(
                    0,
                    DECORATION_SIZE as i32,
                    height as i32,
                    (DECORATION_SIZE * 4) as i32,
                    wl_shm::Format::Argb8888,
                )
                .expect("Pool was destroyed!");
            self.border_surfaces[BORDER_LEFT].attach(Some(&buffer), 0, 0);
            self.border_subsurfaces[BORDER_LEFT].set_position(-(DECORATION_SIZE as i32), 0);
            self.buffers.push(buffer);
        }

        for s in &self.border_surfaces {
            s.commit();
        }
    }

    /// Set a short title for the surface.
    ///
    /// This string may be used to identify the surface in a task bar, window list, or other user
    /// interface elements provided by the compositor.
    pub fn set_title(&self, title: String) {
        match self.shell_surface {
            shell::Surface::Xdg(ref xdg) => {
                xdg.toplevel.set_title(title);
            }
            shell::Surface::Wl(ref wl) => {
                wl.set_title(title);
            }
        }
    }

    /// Set a class for the surface.
    ///
    /// The surface class identifies the general class of applications to which the surface
    /// belongs. A common convention is to use the file name (or the full path if it is a
    /// non-standard location) of the application's .desktop file as the class.
    ///
    /// When using xdg-shell, this calls `ZxdgTopLevelV6::set_app_id`.
    /// When using wl-shell, this calls `WlShellSurface::set_class`.
    pub fn set_class(&self, class: String) {
        match self.shell_surface {
            shell::Surface::Xdg(ref xdg) => {
                xdg.toplevel.set_app_id(class);
            }
            shell::Surface::Wl(ref wl) => {
                wl.set_class(class);
            }
        }
    }

    /// Turn on or off decoration of this surface
    ///
    /// Automatically disables fullscreen mode if it was set.
    pub fn set_decorate(&mut self, decorate: bool) {
        match self.shell_surface {
            shell::Surface::Wl(ref surface) => {
                surface.set_toplevel();
            }
            shell::Surface::Xdg(ref surface) => {
                surface.toplevel.unset_fullscreen();
            }
        }
        self.meta.lock().unwrap().decorate = decorate;
        // trigger redraw
        let (w, h) = self.meta.lock().unwrap().dimensions;
        self.resize(w, h);
    }

    /// Sets this surface as fullscreen (see `wl_shell_surface` for details)
    ///
    /// Automatically disables decorations.
    ///
    /// When using wl-shell, this uses the default fullscreen method and framerate.
    pub fn set_fullscreen(&mut self, output: Option<&wl_output::WlOutput>) {
        match self.shell_surface {
            shell::Surface::Xdg(ref mut xdg) => {
                xdg.toplevel.set_fullscreen(output);
            }
            shell::Surface::Wl(ref mut shell_surface) => {
                let method = wl_shell_surface::FullscreenMethod::Default;
                let framerate = 0; // Let the server decide the framerate.
                shell_surface.set_fullscreen(method, framerate, output);
            }
        }
        self.meta.lock().unwrap().decorate = false;
        // trigger redraw
        let (w, h) = self.meta.lock().unwrap().dimensions;
        self.resize(w, h);
    }

    /// Sets the minimum possible size for this window
    ///
    /// Provide either a tuple `Some((width, height))` or `None` to unset the
    /// minimum size.
    ///
    /// The provided size is the interior size, not counting decorations
    pub fn set_min_size(&mut self, size: Option<(i32, i32)>) {
        self.meta.lock().unwrap().min_size = size;
        if let shell::Surface::Xdg(ref mut xdg) = self.shell_surface {
            let (w, h) = match (size, self.meta.lock().unwrap().decorate) {
                (Some((w, h)), true) => add_borders(w, h),
                (Some((w, h)), false) => (w, h),
                (None, _) => (0, 0),
            };
            xdg.toplevel.set_min_size(w, h);
        }
    }

    /// Sets the maximum possible size for this window
    ///
    /// Provide either a tuple `Some((width, height))` or `None` to unset the
    /// maximum size.
    ///
    /// The provided size is the interior size, not counting decorations
    pub fn set_max_size(&mut self, size: Option<(i32, i32)>) {
        self.meta.lock().unwrap().max_size = size;
        if let shell::Surface::Xdg(ref mut xdg) = self.shell_surface {
            let (w, h) = match (size, self.meta.lock().unwrap().decorate) {
                (Some((w, h)), true) => add_borders(w, h),
                (Some((w, h)), false) => (w, h),
                (None, _) => (0, 0),
            };
            xdg.toplevel.set_max_size(w, h);
        }
    }
}

pub(crate) struct DecoratedSurfaceIData<ID> {
    pub(crate) pointer_state: Rc<RefCell<PointerState>>,
    pub(crate) implementation: DecoratedSurfaceImplementation<ID>,
    pub(crate) idata: Rc<RefCell<ID>>,
}

impl<ID> Clone for DecoratedSurfaceIData<ID> {
    fn clone(&self) -> DecoratedSurfaceIData<ID> {
        DecoratedSurfaceIData {
            pointer_state: self.pointer_state.clone(),
            implementation: self.implementation.clone(),
            idata: self.idata.clone(),
        }
    }
}

/// Creates a new `DecoratedSurface` and inserts it in the
/// event queue
/// 
/// This requires you to provide:
/// 
/// - a mutable reference to the event_queue
/// - an implementation (and implementation data) for the decorated surface
/// - a reference to the surface to decorate
/// - the wanted dimensions for this decoration
/// - a few wayland globals: compositor, subcompositor, shm, shell, and an optional seat
/// - a bool specifying if decorations should be displayed or not
/// 
/// If you provide a seat, it will be used to process user interaction with the decorations.
pub fn init_decorated_surface<ID: 'static>(evqh: &mut EventQueueHandle,
                                           implementation: DecoratedSurfaceImplementation<ID>, idata: ID,
                                           surface: &wl_surface::WlSurface, width: i32, height: i32,
            compositor: &wl_compositor::WlCompositor,
            subcompositor: &wl_subcompositor::WlSubcompositor, shm: &wl_shm::WlShm, shell: &Shell,
            seat: Option<wl_seat::WlSeat>, decorate: bool) -> Result<DecoratedSurface, ()> {
    let (decorated_surface, pointer_state) = create_states(surface, width, height, compositor, subcompositor, shm, shell, seat, decorate)?;
    let shell_surface = decorated_surface.shell_surface.clone().unwrap();
    let pointer = match pointer_state.pointer {
        Pointer::Plain(ref pointer) => pointer.clone(),
        Pointer::Themed(ref pointer) => (*pointer).clone(),
        Pointer::None => None,
    };

    let idata = DecoratedSurfaceIData {
        pointer_state: Rc::new(RefCell::new(pointer_state)),
        implementation: implementation,
        idata: Rc::new(RefCell::new(idata)),
    };

    // init implementations
    if let Some(pointer) = pointer {
        evqh.register(&pointer, pointer_implementation(), idata.clone());
    }
    shell_surface.register_to(evqh, idata);

    Ok(decorated_surface)
}

/// Create a new DecoratedSurface
fn create_states(surface: &wl_surface::WlSurface, width: i32, height: i32,
            compositor: &wl_compositor::WlCompositor,
            subcompositor: &wl_subcompositor::WlSubcompositor, shm: &wl_shm::WlShm, shell: &Shell,
            seat: Option<wl_seat::WlSeat>, decorate: bool)
            -> Result<(DecoratedSurface, PointerState), ()> {
    // handle Shm
    let pxcount = max(
        DECORATION_TOP_SIZE * DECORATION_SIZE,
        max(DECORATION_TOP_SIZE * width, DECORATION_SIZE * height),
    ) as usize;

    let tempfile = match tempfile() {
        Ok(t) => t,
        Err(_) => return Err(()),
    };

    match tempfile.set_len((pxcount * 4) as u64) {
        Ok(()) => {}
        Err(_) => return Err(()),
    };

    let pool = shm.create_pool(tempfile.as_raw_fd(), (pxcount * 4) as i32);

    // create surfaces
    let border_surfaces: Vec<_> = (0..4).map(|_| compositor.create_surface()).collect();
    let border_subsurfaces: Vec<_> = border_surfaces
        .iter()
        .map(|s| {
            subcompositor
                .get_subsurface(&s, surface)
                .expect("Subcompositor cannot be destroyed")
        })
        .collect();
    for s in &border_subsurfaces {
        s.set_desync();
    }

    let shell_surface = shell::Surface::from_shell(surface, shell);

    let meta = Arc::new(Mutex::new(SurfaceMetadata {
        dimensions: (width, height),
        decorate: decorate,
        min_size: None,
        max_size: None
    }));

    // Pointer
    let pointer_state = {
        let surfaces = border_surfaces.iter().map(|s| s.clone().unwrap()).collect();
        let pointer = seat.as_ref()
            .map(|seat| seat.get_pointer().expect("Seat cannot be dead!"));

        let pointer = match pointer.map(|pointer| {
            ThemedPointer::load(pointer, None, &compositor, &shm)
        }) {
            Some(Ok(themed)) => Pointer::Themed(themed),
            Some(Err(plain)) => Pointer::Plain(plain),
            None => Pointer::None,
        };
        PointerState {
            surfaces: surfaces,
            location: PtrLocation::None,
            coordinates: (0., 0.),
            meta: meta.clone(),
            cornered: false,
            topped: false,
            pointer: pointer,
            shell_surface: shell_surface.clone().unwrap(),
            seat: seat
        }
    };

    let mut me = DecoratedSurface {
        shell_surface: shell_surface,
        border_subsurfaces: border_subsurfaces,
        border_surfaces: border_surfaces,
        buffers: Vec::new(),
        tempfile: tempfile,
        pool: pool,
        meta: meta,
        buffer_capacity: pxcount * 4,
    };

    me.resize(width, height);

    Ok((me, pointer_state))
}

fn pointer_implementation<ID>() -> wl_pointer::Implementation<DecoratedSurfaceIData<ID>> {
    wl_pointer::Implementation {
        enter: |_, idata, _, serial, surface, x, y| {
            let mut pointer_state = idata.pointer_state.borrow_mut();
            pointer_state.coordinates = (x, y);
            pointer_state.pointer_entered(surface, serial);
        },
        leave: |_, idata, _, serial, _| {
            let mut pointer_state = idata.pointer_state.borrow_mut();
            pointer_state.pointer_left(serial);
        },
        motion: |_, idata, _, _, x, y| {
            let mut pointer_state = idata.pointer_state.borrow_mut();
            pointer_state.coordinates = (x, y);
            pointer_state.update(None, false);
        },
        button: |_, idata, _, serial, _, button, state| {
            if button != 0x110 {
                return;
            }
            if let wl_pointer::ButtonState::Released = state {
                return;
            }
            let pointer_state = idata.pointer_state.borrow_mut();
            let (x, y) = pointer_state.coordinates;
            let w = pointer_state.meta.lock().unwrap().dimensions.0;
            if let Some((direction, resize)) =
                compute_pointer_action(pointer_state.location, x, y, w as f64)
            {
                if let Some(ref seat) = pointer_state.seat {
                    match pointer_state.shell_surface {
                        shell::Surface::Xdg(ref xdg) if resize => {
                            xdg.toplevel.resize(&seat, serial, direction.to_raw());
                        }
                        shell::Surface::Xdg(ref xdg) => {
                            xdg.toplevel._move(&seat, serial);
                        }
                        shell::Surface::Wl(ref wl) if resize => {
                            wl.resize(&seat, serial, direction);
                        }
                        shell::Surface::Wl(ref wl) => {
                            wl._move(&seat, serial);
                        }
                    }
                }
            }
        },
        axis: |_, _, _, _, _, _| {},
        axis_discrete: |_, _, _, _, _| {},
        axis_source: |_, _, _, _| {},
        axis_stop: |_, _, _, _, _| {},
        frame: |_, _, _| {},
    }
}

fn compute_pointer_action(location: PtrLocation, x: f64, y: f64, w: f64)
                          -> Option<(wl_shell_surface::Resize, bool)> {
    use self::wl_shell_surface::Resize;
    match location {
        PtrLocation::Top => if y < DECORATION_SIZE as f64 {
            if x < DECORATION_SIZE as f64 {
                Some((Resize::TopLeft, true))
            } else if x > w as f64 + DECORATION_SIZE as f64 {
                Some((Resize::TopRight, true))
            } else {
                Some((Resize::Top, true))
            }
        } else {
            if x < DECORATION_SIZE as f64 {
                Some((Resize::Left, true))
            } else if x > w as f64 + DECORATION_SIZE as f64 {
                Some((Resize::Right, true))
            } else {
                Some((Resize::None, false))
            }
        },
        PtrLocation::Bottom => if x < DECORATION_SIZE as f64 {
            Some((Resize::BottomLeft, true))
        } else if x > w as f64 + DECORATION_SIZE as f64 {
            Some((Resize::BottomRight, true))
        } else {
            Some((Resize::Bottom, true))
        },
        PtrLocation::Left => Some((Resize::Left, true)),
        PtrLocation::Right => Some((Resize::Right, true)),
        PtrLocation::None => None,
    }
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

/// For handling events that occur to a DecoratedSurface.
pub struct DecoratedSurfaceImplementation<ID> {
    /// Called whenever the DecoratedSurface has been resized.
    ///
    /// **Note:** if you've not set a minimum size, `width` and `height` will not always be
    /// positive values. Values can be negative if a user attempts to resize the window past
    /// the left or top borders.
    pub configure:
        fn(evqh: &mut EventQueueHandle, idata: &mut ID, cfg: shell::Configure, newsize: Option<(i32, i32)>),
    /// Called when the DecoratedSurface is closed.
    pub close: fn(evqh: &mut EventQueueHandle, idata: &mut ID),
}

impl<ID> Copy for DecoratedSurfaceImplementation<ID> {}
impl<ID> Clone for DecoratedSurfaceImplementation<ID> {
    fn clone(&self) -> DecoratedSurfaceImplementation<ID> {
        *self
    }
}

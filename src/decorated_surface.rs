use std::cmp::max;
use std::io::{Seek, SeekFrom, Write};
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Mutex, MutexGuard};

use byteorder::{WriteBytesExt, NativeEndian};

use tempfile::TempFile;

use wayland::core::{Surface, Registry};
use wayland::core::compositor::{WSurface, SurfaceId};
use wayland::core::seat::{Seat, Pointer, ButtonState};
use wayland::core::shell::{ShellSurface, ShellSurfaceResize};
use wayland::core::shm::{Buffer, ShmPool, ShmFormat};
use wayland::core::subcompositor::SubSurface;

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
pub const BORDER_TOP         : usize = 0;
pub const BORDER_RIGHT       : usize = 1;
pub const BORDER_BOTTOM      : usize = 2;
pub const BORDER_LEFT        : usize = 3;

const DECORATION_SIZE     : i32 = 8;
const DECORATION_TOP_SIZE : i32 = 24;

#[derive(Debug)]
enum PtrLocation {
    None,
    Top,
    Right,
    Bottom,
    Left
}

struct PointerState {
    surfaces: Vec<SurfaceId>,
    location: PtrLocation,
    coordinates: (f64, f64),
    surface_width: i32,
}

impl PointerState {
    fn pointer_entered(&mut self, sid: SurfaceId) {
        if self.surfaces[BORDER_TOP] == sid {
            self.location = PtrLocation::Top;
        } else if self.surfaces[BORDER_RIGHT] == sid {
            self.location = PtrLocation::Right
        } else if self.surfaces[BORDER_BOTTOM] == sid {
            self.location = PtrLocation::Bottom;
        } else if self.surfaces[BORDER_LEFT] == sid {
            self.location = PtrLocation::Left
        } else {
            // should probably never happen ?
            self.location = PtrLocation::None;
        }
    }

    fn pointer_left(&mut self) {
        self.location = PtrLocation::None;
    }
}

/// A wrapper for a decorated surface.
///
/// This is the main object of this crate. It wraps a user provided
/// wayland surface into a `ShellSurface` and gives you acces to it
/// via the `.get_shell()` method.
///
/// It also handles the drawing of minimalistic borders allowing the
/// resizing and moving of the window. See the root documentation of
/// this crate for explanations about how to use it.
pub struct DecoratedSurface<S: Surface> {
    shell_surface: Arc<Mutex<Option<ShellSurface<S>>>>,
    border_surfaces: Vec<SubSurface<WSurface>>,
    buffers: Vec<Buffer>,
    tempfile: TempFile,
    pool: ShmPool,
    height: i32,
    width: i32,
    buffer_capacity: usize,
    _pointer: Option<Pointer<WSurface>>,
    pointer_state: Arc<Mutex<PointerState>>
}

/// A wrapper around a reference to the surface you
/// stored in a `DecoratedSurface`.
///
/// It allows you to access the `ShellSurface` object via deref
/// (and thus the surface itself as well).
pub struct SurfaceGuard<'a, S: Surface + 'a> {
    guard: MutexGuard<'a, Option<ShellSurface<S>>>
}

impl<'a, S: Surface + 'a> Deref for SurfaceGuard<'a, S> {
    type Target = ShellSurface<S>;
    fn deref(&self) -> &ShellSurface<S> {
        self.guard.as_ref().unwrap()
    }
}

impl<'a, S: Surface + 'a> DerefMut for SurfaceGuard<'a, S> {
    fn deref_mut(&mut self) -> &mut ShellSurface<S> {
        self.guard.as_mut().unwrap()
    }
}

impl<S: Surface + Send + 'static> DecoratedSurface<S> {
    /// Resizes the borders to given width and height.
    ///
    /// These values should be the dimentions of the internal surface of the
    /// window (the decorated window will thus be a little larger).
    pub fn resize(&mut self, width: i32, height: i32) {
        let new_pxcount = max(DECORATION_TOP_SIZE * (DECORATION_SIZE * 2 + width),
            max(DECORATION_TOP_SIZE * width, DECORATION_SIZE * height)
        ) as usize;
        if new_pxcount * 4 > self.buffer_capacity {
            // reallocation needed !
            self.tempfile.set_len((new_pxcount * 4) as u64).unwrap();
            self.pool.resize((new_pxcount * 4) as i32);
            self.buffer_capacity = new_pxcount * 4;
        }
        self.width = width;
        self.height = height;
        self.pointer_state.lock().unwrap().surface_width = width;
        // rewrite the data
        self.tempfile.seek(SeekFrom::Start(0)).unwrap();
        for _ in 0..(new_pxcount*4) {
            // write a dark gray
            let _ = self.tempfile.write_u32::<NativeEndian>(0xFF444444);
        }
        self.tempfile.flush().unwrap();
        // resize the borders
        self.buffers.clear();
        // top
        {
            let buffer = self.pool.create_buffer(
                0,
                self.width as i32 + (DECORATION_SIZE as i32) * 2,
                DECORATION_TOP_SIZE as i32,
                (self.width as i32 + (DECORATION_SIZE as i32) * 2) * 4,
                ShmFormat::ARGB8888
            ).unwrap();
            self.border_surfaces[BORDER_TOP].attach(&buffer, 0, 0);
            self.border_surfaces[BORDER_TOP].set_position(
                -(DECORATION_SIZE as i32),
                -(DECORATION_TOP_SIZE as i32)
            );
            self.buffers.push(buffer);
        }
        // right
        {
            let buffer = self.pool.create_buffer(
                0, DECORATION_SIZE as i32,
                self.height as i32, (DECORATION_SIZE*4) as i32,
                ShmFormat::ARGB8888
            ).unwrap();
            self.border_surfaces[BORDER_RIGHT].attach(&buffer, 0, 0);
            self.border_surfaces[BORDER_RIGHT].set_position(self.width as i32, 0);
            self.buffers.push(buffer);
        }
        // bottom
        {
            let buffer = self.pool.create_buffer(
                0,
                self.width as i32 + (DECORATION_SIZE as i32) * 2,
                DECORATION_SIZE as i32,
                (self.width as i32 + (DECORATION_SIZE as i32) * 2) * 4,
                ShmFormat::ARGB8888
            ).unwrap();
            self.border_surfaces[BORDER_BOTTOM].attach(&buffer, 0, 0);
            self.border_surfaces[BORDER_BOTTOM].set_position(-(DECORATION_SIZE as i32), self.height as i32);
            self.buffers.push(buffer);
        }
        // left
        {
            let buffer = self.pool.create_buffer(
                0, DECORATION_SIZE as i32,
                self.height as i32, (DECORATION_SIZE*4) as i32,
                ShmFormat::ARGB8888
            ).unwrap();
            self.border_surfaces[BORDER_LEFT].attach(&buffer, 0, 0);
            self.border_surfaces[BORDER_LEFT].set_position(-(DECORATION_SIZE as i32), 0);
            self.buffers.push(buffer);
        }

        for s in &self.border_surfaces { s.commit(); }
    }

    /// Creates a new decorated window around given surface.
    ///
    /// If the creation failed (likely if the registry was not ready), hands back the surface.
    pub fn new(user_surface: S, width: i32, height: i32, registry: &Registry, seat: Option<&Seat>)
        -> Result<DecoratedSurface<S>,S>
    {
        // fetch the global 
        let comp = match registry.get_compositor() {
            Some(c) => c,
            None => return Err(user_surface)
        };
        let subcomp = match registry.get_subcompositor() {
            Some(c) => c,
            None => return Err(user_surface)
        };
        let shm = match registry.get_shm() {
            Some(s) => s,
            None => return Err(user_surface)
        };
        let shell = match registry.get_shell() {
            Some(s) => s,
            None => return Err(user_surface)
        };

        // handle Shm
        let pxcount = max(DECORATION_TOP_SIZE * DECORATION_SIZE,
            max(DECORATION_TOP_SIZE * width, DECORATION_SIZE * height)
        ) as usize;

        let tempfile = match TempFile::new() {
            Ok(t) => t,
            Err(_) => return Err(user_surface)
        };

        match tempfile.set_len((pxcount *4) as u64) {
            Ok(()) => {},
            Err(_) => return Err(user_surface)
        };

        let pool = shm.pool_from_fd(&tempfile, (pxcount * 4) as i32);

        // create surfaces
        let border_surfaces: Vec<_> = (0..4).map(|_|
            subcomp.get_subsurface(comp.create_surface(), user_surface.get_wsurface())
        ).collect();
        for s in &border_surfaces { s.set_sync(false) }

        let shell_surface = shell.get_shell_surface(user_surface);
        shell_surface.set_toplevel();

        // Pointer
        let mut pointer_state = PointerState {
            surfaces: Vec::with_capacity(4),
            location: PtrLocation::None,
            coordinates: (0., 0.),
            surface_width: width
        };
        let mut pointer = seat.and_then(|seat| seat.get_pointer())
                          .map(|mut pointer| {
            // let (mut pointer, _) = pointer.set_cursor(Some(comp.create_surface()), (0,0));
            for s in &border_surfaces {
                pointer.add_handled_surface(s.get_id());
                pointer_state.surfaces.push(s.get_id());
            }
            pointer
        });
        let pointer_state = Arc::new(Mutex::new(pointer_state));

        let shell_surface = Arc::new(Mutex::new(Some(shell_surface)));

        if let Some(ref mut pointer) = pointer {
            let my_pointer = pointer_state.clone();
            pointer.set_enter_action(move |_pid, _serial, sid, x, y| {
                let mut guard = my_pointer.lock().unwrap();
                guard.pointer_entered(sid);
                guard.coordinates = (x, y);
            });

            let my_pointer = pointer_state.clone();
            pointer.set_leave_action(move |_pid, _serial, _sid| {
                let mut guard = my_pointer.lock().unwrap();
                guard.pointer_left();
                guard.coordinates = (0., 0.);
            });

            let my_pointer = pointer_state.clone();
            pointer.set_motion_action(move |_pid, _t, x, y| {
                let mut guard = my_pointer.lock().unwrap();
                guard.coordinates = (x, y);
            });

            let my_pointer = pointer_state.clone();
            let my_seat = pointer.get_seat().clone();
            let my_shell = shell_surface.clone();
            pointer.set_button_action(move |_pid, serial, _t, button, state| {
                if button != 0x110 { return; }
                if state != ButtonState::Pressed { return; }
                let pguard = my_pointer.lock().unwrap();
                let sguard = my_shell.lock().unwrap();
                let shell = match sguard.as_ref() {
                    Some(s) => s,
                    None => return
                };
                let (x, y) = pguard.coordinates;
                let w = pguard.surface_width;
                let (direction, resize) = match pguard.location {
                    PtrLocation::Top => {
                        if y < DECORATION_SIZE as f64 {
                            if x < DECORATION_SIZE as f64 {
                                (ShellSurfaceResize::TopLeft, true)
                            } else if x > w as f64 + DECORATION_SIZE as f64 {
                                (ShellSurfaceResize::TopRight, true)
                            } else {
                                (ShellSurfaceResize::Top, true)
                            }
                        } else {
                            if x < DECORATION_SIZE as f64 {
                                (ShellSurfaceResize::Left, true)
                            } else if x > w as f64 + DECORATION_SIZE as f64 {
                                (ShellSurfaceResize::Right, true)
                            } else {
                                (ShellSurfaceResize::None, false)
                            }
                        }
                    },
                    PtrLocation::Bottom => {
                        if x < DECORATION_SIZE as f64 {
                            (ShellSurfaceResize::BottomLeft, true)
                        } else if x > w as f64 + DECORATION_SIZE as f64 {
                            (ShellSurfaceResize::BottomRight, true)
                        } else {
                            (ShellSurfaceResize::Bottom, true)
                        }
                    },
                    PtrLocation::Left => (ShellSurfaceResize::Left, true),
                    PtrLocation::Right => (ShellSurfaceResize::Right, true),
                    PtrLocation::None => (ShellSurfaceResize::None, true)
                };
                if resize {
                    shell.start_resize(&my_seat, serial, direction);
                } else {
                    shell.start_move(&my_seat, serial);
                }
            });
        }

        let mut me = DecoratedSurface {
            shell_surface: shell_surface,
            border_surfaces: border_surfaces,
            buffers: Vec::new(),
            tempfile: tempfile,
            pool: pool,
            height: height,
            width: width,
            buffer_capacity: pxcount * 4,
            _pointer: pointer,
            pointer_state: pointer_state
        };

        me.resize(width, height);

        Ok(me)
    }

    /// Creates a guard giving you access to the shell surface wrapped
    /// in this object.
    ///
    /// Calling for a processing of the events from the wayland server
    /// (via `Display::dispatch()` for example) while this guard is in
    /// scope may result in a deadlock.
    pub fn get_shell(&self) -> SurfaceGuard<S> {
        SurfaceGuard {
            guard: self.shell_surface.lock().unwrap()
        }
    }

    /// Destroys the DecoratedSurface and returns the wrapped surface
    pub fn destroy(self) -> S {
        self.shell_surface.lock().unwrap().take().unwrap().destroy()
    }
}

/// Substracts the border dimensions from the given dimensions.
pub fn substract_borders(width: i32, height: i32) -> (i32, i32) {
    (
        width - 2*(DECORATION_SIZE as i32),
        height - DECORATION_SIZE as i32 - DECORATION_TOP_SIZE as i32
    )
}
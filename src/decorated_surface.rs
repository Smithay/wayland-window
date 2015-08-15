use std::cmp::max;
use std::io::{Seek, SeekFrom, Write};
use std::ops::Deref;
use std::sync::{Arc, Mutex};

use byteorder::{WriteBytesExt, NativeEndian};

use tempfile::TempFile;

use wayland::core::{Surface, Registry};
use wayland::core::compositor::{WSurface, SurfaceId};
use wayland::core::seat::{Seat, Pointer};
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

const DECORATION_SIZE     : usize = 8;
const DECORATION_TOP_SIZE : usize = 24;

#[derive(Debug)]
enum PtrLocation {
    None,
    Top,
    Right,
    Bottom,
    Left
}

struct DecoratedInternals {
    shell_surface: ShellSurface<WSurface>,
    border_surfaces: Vec<SubSurface<WSurface>>,
    buffers: Vec<Buffer>,
    tempfile: TempFile,
    pool: ShmPool,
    height: u32,
    width: u32,
    buffer_capacity: usize,
    pointer: Option<Pointer<WSurface>>,
    configure_user_callback: Box<Fn(ShellSurfaceResize, i32, i32) + 'static + Send + Sync>,
    current_pointer_location: PtrLocation,
    current_pointer_coordinates: (f64, f64)
}

impl DecoratedInternals {
    fn resize(&mut self, width: u32, height: u32) {
        let new_pxcount = max(DECORATION_TOP_SIZE * (DECORATION_SIZE * 2 + (width as usize)),
            max(DECORATION_TOP_SIZE * (width as usize), DECORATION_SIZE * (height as usize))
        );
        if new_pxcount * 4 > self.buffer_capacity {
            // reallocation needed !
            self.tempfile.set_len((new_pxcount * 4) as u64).unwrap();
            self.pool.resize((new_pxcount * 4) as i32);
            self.buffer_capacity = new_pxcount * 4;
        }
        self.width = width;
        self.height = height;
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
                DECORATION_TOP_SIZE as i32, (self.width*4) as i32,
                ShmFormat::ARGB8888
            ).unwrap();
            self.border_surfaces[BORDER_TOP].attach(&buffer, 0, 0);
            self.border_surfaces[BORDER_TOP].set_position(0, 0);
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
            self.border_surfaces[BORDER_RIGHT].set_position(
                DECORATION_SIZE as i32 + self.width as i32, DECORATION_TOP_SIZE as i32);
            self.buffers.push(buffer);
        }
        // bottom
        {
            let buffer = self.pool.create_buffer(
                0,
                self.width as i32 + (DECORATION_SIZE as i32) * 2,
                DECORATION_SIZE as i32, (self.width*4) as i32,
                ShmFormat::ARGB8888
            ).unwrap();
            self.border_surfaces[BORDER_BOTTOM].attach(&buffer, 0, 0);
            self.border_surfaces[BORDER_BOTTOM].set_position(
                0,
                DECORATION_TOP_SIZE as i32 + self.height as i32);
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
            self.border_surfaces[BORDER_LEFT].set_position(0,
                DECORATION_TOP_SIZE as i32);
            self.buffers.push(buffer);
        }

        for s in &self.border_surfaces { s.commit(); }

        {
            let buffer = self.pool.create_buffer(
                0, 1, 1, 4,
                ShmFormat::ARGB8888
            ).unwrap();
            self.shell_surface.attach(&buffer, 0, 0);
            self.buffers.push(buffer);
            self.shell_surface.commit();
        }
    }

    fn pointer_entered(&mut self, sid: SurfaceId) {
        if self.border_surfaces[BORDER_TOP].get_id() == sid {
            self.current_pointer_location = PtrLocation::Top;
        } else if self.border_surfaces[BORDER_RIGHT].get_id() == sid {
            self.current_pointer_location = PtrLocation::Right
        } else if self.border_surfaces[BORDER_BOTTOM].get_id() == sid {
            self.current_pointer_location = PtrLocation::Bottom;
        } else if self.border_surfaces[BORDER_LEFT].get_id() == sid {
            self.current_pointer_location = PtrLocation::Left
        } else {
            // should probably never happen ?
            self.current_pointer_location = PtrLocation::None;
        }
    }

    fn pointer_left(&mut self) {
        self.current_pointer_location = PtrLocation::None;
    }
}

/// A decorated surface, wrapping a wayalnd surface and handling its decorations.
pub struct DecoratedSurface<S: Surface> {
    internals: Arc<Mutex<DecoratedInternals>>,
    user_surface: SubSurface<S>,
}

impl<S: Surface> DecoratedSurface<S> {
    /// Creates a new decorated window around given surface.
    ///
    /// If the creation failed (likely if the registry was not ready), hands back the surface.
    pub fn new(user_surface: S, width: u32, height: u32, registry: &Registry, seat: Option<&Seat>)
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
            max(DECORATION_TOP_SIZE * (width as usize), DECORATION_SIZE * (height as usize))
        );

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
        let main_surface = comp.create_surface();
        let user_subsurface = subcomp.get_subsurface(user_surface, &main_surface);
        user_subsurface.set_sync(false);
        let border_surfaces: Vec<_> = (0..4).map(|_|
            subcomp.get_subsurface(comp.create_surface(), &main_surface)
        ).collect();

        let shell_surface = shell.get_shell_surface(main_surface);
        shell_surface.set_toplevel();

        // Pointer
        let pointer = seat.and_then(|seat| seat.get_pointer())
                          .map(|mut pointer| {
            // let (mut pointer, _) = pointer.set_cursor(Some(comp.create_surface()), (0,0));
            for s in &border_surfaces {
                pointer.add_handled_surface(s.get_id());
            }
            pointer
        });

        // place the user surface
        user_subsurface.set_position(DECORATION_SIZE as i32, DECORATION_TOP_SIZE as i32);

        let internals = Arc::new(Mutex::new(DecoratedInternals {
            shell_surface: shell_surface,
            border_surfaces: border_surfaces,
            buffers: Vec::new(),
            height: height,
            width: width,
            tempfile: tempfile,
            pool: pool,
            buffer_capacity: pxcount * 4,
            pointer: pointer,
            configure_user_callback: Box::new(move |_,_,_| {}),
            current_pointer_location: PtrLocation::None,
            current_pointer_coordinates: (0., 0.)
        }));

        {

            let mut internals_guard = internals.lock().unwrap();
            let my_internals = internals.clone();

            internals_guard.shell_surface.set_configure_callback(move |resizedir, width, height| {
                let mut guard = my_internals.lock().unwrap();
                guard.resize(width as u32, height as u32);
                (guard.configure_user_callback)(
                    resizedir,
                    width - (DECORATION_SIZE*2) as i32,
                    height - (DECORATION_SIZE + DECORATION_TOP_SIZE) as i32
                );
            });

            if let Some(ref mut pointer) = internals_guard.pointer.as_mut() {
                let my_internals = internals.clone();
                pointer.set_enter_action(move |_pid, _serial, sid, x, y| {
                    let mut guard = my_internals.lock().unwrap();
                    guard.pointer_entered(sid);
                    guard.current_pointer_coordinates = (x, y);
                });

                let my_internals = internals.clone();
                pointer.set_leave_action(move |_pid, _serial, _sid| {
                    let mut guard = my_internals.lock().unwrap();
                    guard.pointer_left();
                    guard.current_pointer_coordinates = (0., 0.);
                });

                let my_internals = internals.clone();
                pointer.set_motion_action(move |_pid, _t, x, y| {
                    let mut guard = my_internals.lock().unwrap();
                    guard.current_pointer_coordinates = (x, y);
                })
            }

        }

        let mut me = DecoratedSurface {
            user_surface: user_subsurface,
            internals: internals
        };

        me.resize(width, height);

        Ok(me)
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        let mut internals = self.internals.lock().unwrap();
        internals.resize(width, height);
    }

    /// Destroys the decorated window and gives back the wrapped surface.
    pub fn unwrap(self) -> S {
        self.user_surface.destroy()
    }
}

impl<S: Surface> Deref for DecoratedSurface<S> {
    type Target = S;
    fn deref(&self) -> &S {
        &*self.user_surface
    }
}
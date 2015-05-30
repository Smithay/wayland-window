use std::cmp::max;
use std::ffi::CString;
use std::ops::Deref;
use std::sync::{Arc, Mutex};

use byteorder::{WriteBytesExt, NativeEndian};

use libc::{c_char, c_int, off_t, size_t, ftruncate, unlink, write, lseek, SEEK_SET};

use wayland::core::{Buffer, SubSurface, ShellSurface, Surface, WSurface,
                    Registry, ShmPool, ShmFormat, Pointer};

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

struct DecoratedInternals {
    shell_surface: ShellSurface<WSurface>,
    border_surfaces: Vec<SubSurface<WSurface>>,
    buffers: Vec<Buffer>,
    shm_fd: c_int,
    pool: ShmPool,
    height: u32,
    width: u32,
    buffer_capacity: usize,
    pointer: Option<Pointer<WSurface>>
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
    pub fn new(user_surface: S, width: u32, height: u32, registry: &Registry)
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
        let seats = registry.get_seats();

        // handle Shm
        let pxcount = max(DECORATION_TOP_SIZE * DECORATION_SIZE,
            max(DECORATION_TOP_SIZE * (width as usize), DECORATION_SIZE * (height as usize))
        );

        let pattern = CString::new("wayland-window-rs-XXXXXX").unwrap();
        let fd = unsafe { mkstemp(pattern.as_ptr() as *mut _) };
        if fd < 0 { return Err(user_surface) }
        unsafe {
            ftruncate(fd, (pxcount * 4) as off_t);
            unlink(pattern.as_ptr());
        }

        let pool = shm.pool_from_raw_fd(fd, (pxcount * 4) as i32);

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
        let pointer = seats.first().and_then(|seat| seat.get_pointer())
                                       .map(|pointer| {
            let (mut pointer, _) = pointer.set_cursor(Some(comp.create_surface()), (0,0));
            for s in &border_surfaces {
                pointer.add_handled_surface(s.get_id());
            }
            pointer
        });

        let internals = Arc::new(Mutex::new(DecoratedInternals {
            shell_surface: shell_surface,
            border_surfaces: border_surfaces,
            buffers: Vec::new(),
            height: height,
            width: width,
            shm_fd: fd,
            pool: pool,
            buffer_capacity: pxcount * 4,
            pointer: pointer
        }));

        let mut me = DecoratedSurface {
            user_surface: user_subsurface,
            internals: internals
        };

        me.resize(width, height);

        Ok(me)
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        let mut internals = self.internals.lock().unwrap();

        let new_pxcount = max(DECORATION_TOP_SIZE * (DECORATION_SIZE * 2 + (width as usize)),
            max(DECORATION_TOP_SIZE * (width as usize), DECORATION_SIZE * (height as usize))
        );
        if new_pxcount * 4 > internals.buffer_capacity {
            // reallocation needed !
            unsafe { ftruncate(internals.shm_fd, (new_pxcount * 4) as off_t) };
            internals.pool.resize((new_pxcount * 4) as i32);
            internals.buffer_capacity = new_pxcount * 4;
        }
        internals.width = width;
        internals.height = height;
        // rewrite the data
        {
            let mut new_data = Vec::<u8>::with_capacity(new_pxcount * 4);
            for _ in 0..(new_pxcount*4) {
                // write a dark gray
                let _ = new_data.write_u32::<NativeEndian>(0xFF444444);
            }
            unsafe {
                lseek(internals.shm_fd, 0, SEEK_SET);
                write(internals.shm_fd, new_data.as_ptr() as *const _, new_data.len() as size_t);
            }
        }

        //drop(mmap);
        
        // resize the borders
        internals.buffers.clear();
        // top
        {
            let buffer = internals.pool.create_buffer(
                0,
                internals.width as i32 + (DECORATION_SIZE as i32) * 2,
                DECORATION_TOP_SIZE as i32, (internals.width*4) as i32,
                ShmFormat::WL_SHM_FORMAT_ARGB8888
            ).unwrap();
            internals.border_surfaces[BORDER_TOP].attach(&buffer, 0, 0);
            internals.border_surfaces[BORDER_TOP].set_position(0, 0);
            internals.buffers.push(buffer);
        }
        // right
        {
            let buffer = internals.pool.create_buffer(
                0, DECORATION_SIZE as i32,
                internals.height as i32, (DECORATION_SIZE*4) as i32,
                ShmFormat::WL_SHM_FORMAT_ARGB8888
            ).unwrap();
            internals.border_surfaces[BORDER_RIGHT].attach(&buffer, 0, 0);
            internals.border_surfaces[BORDER_RIGHT].set_position(
                DECORATION_SIZE as i32 + internals.width as i32, DECORATION_TOP_SIZE as i32);
            internals.buffers.push(buffer);
        }
        // bottom
        {
            let buffer = internals.pool.create_buffer(
                0,
                internals.width as i32 + (DECORATION_SIZE as i32) * 2,
                DECORATION_SIZE as i32, (internals.width*4) as i32,
                ShmFormat::WL_SHM_FORMAT_ARGB8888
            ).unwrap();
            internals.border_surfaces[BORDER_BOTTOM].attach(&buffer, 0, 0);
            internals.border_surfaces[BORDER_BOTTOM].set_position(
                0,
                DECORATION_TOP_SIZE as i32 + internals.height as i32);
            internals.buffers.push(buffer);
        }
        // left
        {
            let buffer = internals.pool.create_buffer(
                0, DECORATION_SIZE as i32,
                internals.height as i32, (DECORATION_SIZE*4) as i32,
                ShmFormat::WL_SHM_FORMAT_ARGB8888
            ).unwrap();
            internals.border_surfaces[BORDER_LEFT].attach(&buffer, 0, 0);
            internals.border_surfaces[BORDER_LEFT].set_position(0,
                DECORATION_TOP_SIZE as i32);
            internals.buffers.push(buffer);
        }
        for s in &internals.border_surfaces { s.commit(); }

        self.user_surface.set_position(DECORATION_SIZE as i32, DECORATION_TOP_SIZE as i32);

        {
            let buffer = internals.pool.create_buffer(
                0, 1, 1, 4,
                ShmFormat::WL_SHM_FORMAT_ARGB8888
            ).unwrap();
            internals.shell_surface.attach(&buffer, 0, 0);
            internals.buffers.push(buffer);
            internals.shell_surface.commit();
        }

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

extern {
    fn mkstemp(template: *mut c_char) -> c_int;
}
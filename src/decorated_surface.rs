use std::cmp::max;
use std::ffi::CString;
use std::io::{Write, Cursor};
use std::ops::Deref;

use byteorder::{WriteBytesExt, NativeEndian};

use libc::{c_char, c_int, off_t, ftruncate};

use mmap::{MemoryMap, MapOption};

use wayland::core::{Buffer, SubSurface, ShellSurface, Surface, WSurface, Registry, ShmPool, ShmFormat, Display};

// The surfaces handling the borders, 8 total, are organised this way:
//
//  0 |   1   | 2
// ---|-------|---
//    |       |
//  7 | user  | 3
//    |       |
// ---|-------|---
//  6 |   5   | 4
//
pub const BORDER_TOP_LEFT    : usize = 0;
pub const BORDER_TOP         : usize = 1;
pub const BORDER_TOP_RIGHT   : usize = 2;
pub const BORDER_RIGHT       : usize = 3;
pub const BORDER_BOTTOM_RIGHT: usize = 4;
pub const BORDER_BOTTOM      : usize = 5;
pub const BORDER_BOTTOM_LEFT : usize = 6;
pub const BORDER_LEFT        : usize = 7;

const DECORATION_SIZE     : usize = 2;
const DECORATION_TOP_SIZE : usize = 8;

/// A decorated surface, wrapping a wayalnd surface and handling its decorations.
pub struct DecoratedSurface<S: Surface> {
    shell_surface: ShellSurface<WSurface>,
    user_surface: SubSurface<S>,
    border_surfaces: Vec<SubSurface<WSurface>>,
    buffers: Vec<Buffer>,
    shm_fd: c_int,
    map: MemoryMap,
    pool: ShmPool,
    height: u32,
    width: u32
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

        // handle Shm
        let pxcount = max(DECORATION_TOP_SIZE * DECORATION_SIZE,
            max(DECORATION_TOP_SIZE * (width as usize), DECORATION_SIZE * (height as usize))
        );

        let pattern = CString::new("wayland-window-rs-XXXXXX").unwrap();
        let fd = unsafe { mkstemp(pattern.as_ptr() as *mut _) };
        if fd < 0 { return Err(user_surface) }
        unsafe { ftruncate(fd, (pxcount * 4) as off_t); }

        let map = match MemoryMap::new(
            pxcount * 4,
            &[MapOption::MapWritable, MapOption::MapFd(fd)]
        ) {
            Ok(m) => m,
            Err(_) => return Err(user_surface)
        };

        let pool = shm.pool_from_raw_fd(fd, (pxcount * 4) as i32);

        // create surfaces
        let main_surface = comp.create_surface();
        let user_subsurface = subcomp.get_subsurface(user_surface, &main_surface);
        let border_surfaces = (0..8).map(|_|
            subcomp.get_subsurface(comp.create_surface(), &main_surface)
        ).collect();

        let shell_surface = shell.get_shell_surface(main_surface);
        shell_surface.set_toplevel();

        let mut me = DecoratedSurface {
            shell_surface: shell_surface,
            user_surface: user_subsurface,
            border_surfaces: border_surfaces,
            buffers: Vec::new(),
            height: height,
            width: width,
            shm_fd: fd,
            map: map,
            pool: pool
        };

        me.resize(width, height);

        Ok(me)
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        let new_pxcount = max(DECORATION_TOP_SIZE * DECORATION_SIZE,
            max(DECORATION_TOP_SIZE * (width as usize), DECORATION_SIZE * (height as usize))
        );
        if new_pxcount * 4 > self.map.len() {
            // reallocation needed !
            unsafe { ftruncate(self.shm_fd, (new_pxcount * 4) as off_t) };
            let new_map = MemoryMap::new(
                new_pxcount * 4,
                &[MapOption::MapWritable, MapOption::MapFd(self.shm_fd)]
            ).unwrap(); // possible to do better ?
            // drop the old mmap
            let _ = ::std::mem::replace(&mut self.map, new_map);
            self.pool.resize((new_pxcount * 4) as i32);
        }
        self.width = width;
        self.height = height;
        // rewrite the data
        {
            let mut pool_data: Cursor<&mut [u8]> = Cursor::new(unsafe {
                ::std::slice::from_raw_parts_mut(self.map.data(), self.map.len())
            });
            for _ in 0..new_pxcount {
                // write a dark gray
                let _ = pool_data.write_u32::<NativeEndian>(0xFFFFFFFF);
            }
            let _ = pool_data.flush();
        }
        
        // resize the borders
        self.buffers.clear();
        // top-left
        {
            let buffer = self.pool.create_buffer(
                0, DECORATION_SIZE as i32, DECORATION_TOP_SIZE as i32, (DECORATION_SIZE*4) as i32,
                ShmFormat::WL_SHM_FORMAT_ARGB8888
            ).unwrap();
            self.border_surfaces[BORDER_TOP_LEFT].attach(&buffer, 0, 0);
            self.buffers.push(buffer);
        }
        // top
        {
            let buffer = self.pool.create_buffer(
                0, self.width as i32, DECORATION_TOP_SIZE as i32, (self.width*4) as i32,
                ShmFormat::WL_SHM_FORMAT_ARGB8888
            ).unwrap();
            self.border_surfaces[BORDER_TOP].attach(&buffer, 0, 0);
            self.buffers.push(buffer);
        }
        // top-right
        {
            let buffer = self.pool.create_buffer(
                0, DECORATION_SIZE as i32, DECORATION_TOP_SIZE as i32, (DECORATION_SIZE*4) as i32,
                ShmFormat::WL_SHM_FORMAT_ARGB8888
            ).unwrap();
            self.border_surfaces[BORDER_TOP_RIGHT].attach(&buffer, 0, 0);
            self.buffers.push(buffer);
        }
        // right
        {
            let buffer = self.pool.create_buffer(
                0, DECORATION_SIZE as i32, self.height as i32, (DECORATION_SIZE*4) as i32,
                ShmFormat::WL_SHM_FORMAT_ARGB8888
            ).unwrap();
            self.border_surfaces[BORDER_RIGHT].attach(&buffer, 0, 0);
            self.buffers.push(buffer);
        }
        // bottom-right
        {
            let buffer = self.pool.create_buffer(
                0, DECORATION_SIZE as i32, DECORATION_SIZE as i32, (DECORATION_SIZE*4) as i32,
                ShmFormat::WL_SHM_FORMAT_ARGB8888
            ).unwrap();
            self.border_surfaces[BORDER_BOTTOM_RIGHT].attach(&buffer, 0, 0);
            self.buffers.push(buffer);
        }
        // bottom
        {
            let buffer = self.pool.create_buffer(
                0, self.width as i32, DECORATION_SIZE as i32, (self.width*4) as i32,
                ShmFormat::WL_SHM_FORMAT_ARGB8888
            ).unwrap();
            self.border_surfaces[BORDER_BOTTOM].attach(&buffer, 0, 0);
            self.buffers.push(buffer);
        }
        // bottom-left
        {
            let buffer = self.pool.create_buffer(
                0, DECORATION_SIZE as i32, DECORATION_SIZE as i32, (DECORATION_SIZE*4) as i32,
                ShmFormat::WL_SHM_FORMAT_ARGB8888
            ).unwrap();
            self.border_surfaces[BORDER_BOTTOM_LEFT].attach(&buffer, 0, 0);
            self.buffers.push(buffer);
        }
        // left
        {
            let buffer = self.pool.create_buffer(
                0, DECORATION_SIZE as i32, self.height as i32, (DECORATION_SIZE*4) as i32,
                ShmFormat::WL_SHM_FORMAT_ARGB8888
            ).unwrap();
            self.border_surfaces[BORDER_LEFT].attach(&buffer, 0, 0);
            self.buffers.push(buffer);
        }
        for s in &self.border_surfaces { s.commit(); }

        {
            let buffer = self.pool.create_buffer(
                0, 10, 10, 40,
                ShmFormat::WL_SHM_FORMAT_ARGB8888
            ).unwrap();
            self.shell_surface.attach(&buffer, 0, 0);
            self.buffers.push(buffer);
            self.shell_surface.commit();
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
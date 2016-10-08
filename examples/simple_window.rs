extern crate byteorder;
extern crate tempfile;
#[macro_use]
extern crate wayland_client;
extern crate wayland_window;

use byteorder::{WriteBytesExt, NativeEndian};

use std::fs::File;
use std::io::Write;
use std::os::unix::io::AsRawFd;

use tempfile::tempfile;

use wayland_client::{EventQueueHandle, EnvHandler};
use wayland_client::protocol::{wl_surface, wl_shm_pool, wl_buffer, wl_compositor, wl_shell,
                               wl_subcompositor, wl_shm, wl_shell_surface};

use wayland_window::DecoratedSurface;

wayland_env!(WaylandEnv,
    compositor: wl_compositor::WlCompositor,
    subcompositor: wl_subcompositor::WlSubcompositor,
    shm: wl_shm::WlShm,
    shell: wl_shell::WlShell
);

struct Window {
    s: wl_surface::WlSurface,
    tmp: File,
    pool: wl_shm_pool::WlShmPool,
    pool_size: usize,
    buf: wl_buffer::WlBuffer,
    newsize: Option<(i32, i32)>
}

impl wayland_window::Handler for Window {
    fn configure(&mut self, _: &mut EventQueueHandle, _: wl_shell_surface::Resize, width: i32, height: i32) {
        self.newsize = Some((width, height))
    }
}

impl Window {
    fn resize(&mut self, width: i32, height: i32) {
        if (width*height*4) as usize > self.pool_size {
            // need to reallocate a bigger buffer
            for _ in 0..((width*height) as usize - self.pool_size / 4) {
                self.tmp.write_u32::<NativeEndian>(0xFF880000).unwrap();
            }
            self.pool.resize(width*height*4);
            self.pool_size = (width*height*4) as usize;
        }
        self.buf.destroy();
        self.buf = self.pool.create_buffer(0, width, height, width*4, wl_shm::Format::Argb8888).expect("Pool should not be dead!");
        self.s.attach(Some(&self.buf), 0, 0);
        self.s.commit();
    }
}

fn main() {
    let (display, mut event_queue) = match wayland_client::default_connect() {
        Ok(ret) => ret,
        Err(e) => panic!("Cannot connect to wayland server: {:?}", e)
    };

    event_queue.add_handler(EnvHandler::<WaylandEnv>::new());
    let registry = display.get_registry().expect("Display cannot be already destroyed.");
    event_queue.register::<_, EnvHandler<WaylandEnv>>(&registry,0);
    event_queue.sync_roundtrip().unwrap();

     // create a tempfile to write the conents of the window on
    let mut tmp = tempfile().ok().expect("Unable to create a tempfile.");
    // write the contents to it, lets put everything in dark red
    for _ in 0..16 {
        let _ = tmp.write_u32::<NativeEndian>(0xFF880000);
    }
    let _ = tmp.flush();

    // prepare the decorated surface
    let decorated_surface = {
        // introduce a new scope because .state() borrows the event_queue
        let state = event_queue.state();
        // retrieve the EnvHandler
        let env = state.get_handler::<EnvHandler<WaylandEnv>>(0);
        let surface = env.compositor.create_surface().expect("Compositor cannot be destroyed");
        let pool = env.shm.create_pool(tmp.as_raw_fd(), 64).expect("Shm cannot be destroyed");
        let buffer = pool.create_buffer(0, 4, 4, 16, wl_shm::Format::Argb8888).expect("I didn't destroy the pool!");

        // find the seat if any
        let mut seat = None;
        for &(id, ref interface, _) in env.globals() {
            if interface == "wl_seat" {
                seat = Some(registry.bind(1, id).expect("Registry cannot die!"));
                break;
            }
        }

        surface.attach(Some(&buffer), 0, 0);
        surface.commit();

        let window = Window {
            s: surface,
            tmp: tmp,
            pool: pool,
            pool_size: 64,
            buf: buffer,
            newsize: Some((256, 128))
        };

        let mut decorated_surface = DecoratedSurface::new(&window.s, 16, 16,
            &env.compositor,
            &env.subcompositor,
            &env.shm,
            &env.shell,
            seat,
            true
        ).unwrap();

        *(decorated_surface.handler()) = Some(window);
        decorated_surface
    };

    event_queue.add_handler_with_init(decorated_surface);

    loop {
        display.flush().unwrap();
        event_queue.dispatch().unwrap();

        // resize if needed
        let mut state = event_queue.state();
        let mut decorated_surface = state.get_mut_handler::<DecoratedSurface<Window>>(1);
        if let Some((w, h)) = decorated_surface.handler().as_mut().unwrap().newsize.take() {
            decorated_surface.resize(w, h);
            decorated_surface.handler().as_mut().unwrap().resize(w, h);
        }
    }
}

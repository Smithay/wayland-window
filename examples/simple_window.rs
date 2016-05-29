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

use wayland_client::wayland::get_display;
use wayland_client::wayland::compositor::{WlCompositor, WlSurface};
use wayland_client::wayland::subcompositor::WlSubcompositor;
use wayland_client::wayland::shm::{WlShm, WlShmFormat, WlShmPool, WlBuffer};
use wayland_client::wayland::seat::WlSeat;
use wayland_client::wayland::shell::WlShell;

use wayland_window::{DecoratedSurface, substract_borders};

wayland_env!(WaylandEnv,
    compositor: WlCompositor,
    subcompositor: WlSubcompositor,
    shm: WlShm,
    shell: WlShell
);

struct Window {
    w: DecoratedSurface,
    s: WlSurface,
    tmp: File,
    pool: WlShmPool,
    pool_size: usize,
    buf: WlBuffer
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
        self.buf = self.pool.create_buffer(0, width, height, width*4, WlShmFormat::Argb8888);
        self.w.resize(width, height);
        self.s.attach(Some(&self.buf), 0, 0);
        self.s.commit();
    }
}

fn main() {

    let (display, event_iterator) = get_display().expect("Unable to connect to Wayland server.");
    let (env, mut event_iterator) = WaylandEnv::init(display, event_iterator);

    // quickly extract the global we need, and fail-fast if any is missing
    // should not happen, as these are supposed to always be implemented by
    // the compositor
    let compositor = env.compositor.as_ref().map(|o| &o.0).unwrap();
    let subcompositor = env.subcompositor.as_ref().map(|o| &o.0).unwrap();
    let shell = env.shell.as_ref().map(|o| &o.0).unwrap();
    let shm = env.shm.as_ref().map(|o| &o.0).unwrap();

    // Not a good way to create a shared buffer, but this will do for this example.
    let mut tmp = tempfile().ok().expect("Unable to create a tempfile.");
    // write the contents to it, lets put everything in dark red
    for _ in 0..16 {
        let _ = tmp.write_u32::<NativeEndian>(0xFF880000);
    }
    let _ = tmp.flush();
    // create a shm_pool from this tempfile
    let pool = shm.create_pool(tmp.as_raw_fd(), 64);
    // match a buffer on the part we wrote on
    let buffer = pool.create_buffer(0, 4, 4, 16, WlShmFormat::Argb8888);

    let surface = compositor.create_surface();

    let window = match DecoratedSurface::new(&surface, 16, 16,
        &env.display,
        compositor,
        subcompositor,
        shm,
        shell,
        env.rebind::<WlSeat>().map(|(s, _)| s)
    ) {
        Ok(w) => w,
        Err(_) => panic!("ERROR")
    };

    // store all this in a struct to make sharing and update easier
    let mut window = Window {
        w: window,
        s: surface,
        tmp: tmp,
        pool: pool,
        pool_size: 64,
        buf: buffer
    };

    window.resize(256, 128);

    event_iterator.sync_roundtrip().unwrap();

    loop {
        for e in &mut event_iterator {
        match e {
            _ => {}
        }}
        let mut newsize = None;
        for (_, x, y) in &mut window.w {
            newsize = Some((x, y))
        }
        if let Some((x, y)) = newsize {
            let (x, y) = substract_borders(x, y);
            window.resize(x, y);
        }
        event_iterator.dispatch().expect("Connection with the compositor was lost.");
    }
}

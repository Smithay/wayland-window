extern crate byteorder;
extern crate tempfile;
extern crate wayland_client as wayland;
extern crate wayland_window;

use byteorder::{WriteBytesExt, NativeEndian};

use std::cmp::max;
use std::io::Write;
use std::sync::{Arc, Mutex};

use tempfile::TempFile;

use wayland::core::compositor::WSurface;
use wayland::core::default_display;
use wayland::core::shm::{Buffer, ShmPool, ShmFormat};

use wayland_window::{DecoratedSurface, substract_borders};

struct Window {
    w: DecoratedSurface<WSurface>,
    tmp: TempFile,
    pool: ShmPool,
    pool_size: usize,
    buf: Buffer
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
        self.buf = self.pool.create_buffer(0, width, height, width*4, ShmFormat::ARGB8888).unwrap();
        self.w.resize(width as u32, height as u32);
        let surface = self.w.get_shell();
        surface.attach(&self.buf, 0, 0);
        surface.commit();
    }
}

fn main() {

    let display = default_display().expect("Unable to connect to Wayland server.");

    let registry = display.get_registry();
    display.sync_roundtrip();

    let compositor = registry.get_compositor().expect("Unable to get the compositor.");
    let surface = compositor.create_surface();
    let shm = registry.get_shm().expect("Unable to get the shm.");
    let seats = registry.get_seats();

    display.sync_roundtrip();

    // Not a good way to create a shared buffer, but this will do for this example.
    let mut tmp = TempFile::new().ok().expect("Unable to create a tempfile.");
    // write the contents to it, lets put everything in dark red
    for _ in 0..16 {
        let _ = tmp.write_u32::<NativeEndian>(0xFF880000);
    }
    let _ = tmp.flush();
    // create a shm_pool from this tempfile
    let pool = shm.pool_from_fd(&tmp, 64);
    // match a buffer on the part we wrote on
    let buffer = pool.create_buffer(0, 4, 4, 16, ShmFormat::ARGB8888)
                     .expect("Could not create buffer.");

    let window = match DecoratedSurface::new(surface, 16, 16, &registry, seats.first()) {
        Ok(w) => w,
        Err(_) => panic!("ERROR")
    };

    // store all this in a struct to make sharing and update easier
    let mut window = Window {
        w: window,
        tmp: tmp,
        pool: pool,
        pool_size: 64,
        buf: buffer
    };

    window.resize(256, 128);

    display.sync_roundtrip();

    let newsize = Arc::new(Mutex::new((0, 0, false)));
    let my_newsize = newsize.clone();
    window.w.get_shell().set_configure_callback(move |_edge, width, height| {
        let (w, h) = substract_borders(width, height);
        let mut guard = my_newsize.lock().unwrap();
        // If several events occur in the same dispatch(),
        // only keep the last one
        // also, refuse to get too small
        *guard = (max(w, 16), max(h, 16), true);
    });

    loop {
        display.dispatch();
        let mut guard = newsize.lock().unwrap();
        let (w, h, b) = *guard;
        if b {
            window.resize(w, h);
            *guard = (0, 0, false);
        }

    }
}

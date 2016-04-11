extern crate byteorder;
extern crate tempfile;
extern crate wayland_client;
extern crate wayland_window;

use byteorder::{WriteBytesExt, NativeEndian};

use std::fs::File;
use std::io::Write;
use std::os::unix::io::AsRawFd;

use tempfile::tempfile;

use wayland_client::{EventIterator, Proxy, Event};
use wayland_client::wayland::{WlDisplay, WlRegistry, get_display};
use wayland_client::wayland::{WaylandProtocolEvent, WlRegistryEvent};
use wayland_client::wayland::compositor::{WlCompositor, WlSurface};
use wayland_client::wayland::subcompositor::WlSubcompositor;
use wayland_client::wayland::shm::{WlShm, WlShmFormat, WlShmPool, WlBuffer};
use wayland_client::wayland::seat::WlSeat;
use wayland_client::wayland::shell::WlShell;

use wayland_window::{DecoratedSurface, substract_borders};

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
        self.buf = self.pool.create_buffer(0, width, height, width*4, WlShmFormat::Argb8888 as u32);
        self.w.resize(width, height);
        self.s.attach(Some(&self.buf), 0, 0);
        self.s.commit();
    }
}

struct WaylandEnv {
    display: WlDisplay,
    registry: WlRegistry,
    compositor: Option<WlCompositor>,
    subcompositor: Option<WlSubcompositor>,
    seat: Option<WlSeat>,
    shm: Option<WlShm>,
    shell: Option<WlShell>,
}

impl WaylandEnv {
    fn new(mut display: WlDisplay) -> WaylandEnv {
        let registry = display.get_registry();
        display.sync_roundtrip().unwrap();

        WaylandEnv {
            display: display,
            registry: registry,
            compositor: None,
            subcompositor: None,
            seat: None,
            shm: None,
            shell: None
        }
    }

    fn handle_global(&mut self, name: u32, interface: &str, _version: u32) {
        match interface {
            "wl_compositor" => self.compositor = Some(
                unsafe { self.registry.bind::<WlCompositor>(name, 1) }
            ),
            "wl_subcompositor" => self.subcompositor = Some(
                unsafe { self.registry.bind::<WlSubcompositor>(name, 1) }
            ),
            "wl_seat" => self.seat = Some(
                unsafe { self.registry.bind::<WlSeat>(name, 1) }
            ),
            "wl_shell" => self.shell = Some(
                unsafe { self.registry.bind::<WlShell>(name, 1) }
            ),
            "wl_shm" => self.shm = Some(
                unsafe { self.registry.bind::<WlShm>(name, 1) }
            ),
            _ => {}
        }
    }

    fn init(&mut self, iter: &mut EventIterator) {
        for evt in iter {
            match evt {
                Event::Wayland(WaylandProtocolEvent::WlRegistry(
                    _, WlRegistryEvent::Global(name, interface, version)
                )) => {
                    self.handle_global(name, &interface, version)
                }
                _ => {}
            }
        }
        if self.compositor.is_none() || self.seat.is_none() ||
            self.shell.is_none() || self.shm.is_none() {
            panic!("Missing some globals ???");
        }
    }
}


fn main() {

    let mut display = get_display().expect("Unable to connect to Wayland server.");
    let mut event_iterator = EventIterator::new();
    display.set_evt_iterator(&event_iterator);

    let mut env = WaylandEnv::new(display);
    // the only events to handle are the globals
    env.init(&mut event_iterator);

    // Not a good way to create a shared buffer, but this will do for this example.
    let mut tmp = tempfile().ok().expect("Unable to create a tempfile.");
    // write the contents to it, lets put everything in dark red
    for _ in 0..16 {
        let _ = tmp.write_u32::<NativeEndian>(0xFF880000);
    }
    let _ = tmp.flush();
    // create a shm_pool from this tempfile
    let pool = env.shm.as_ref().unwrap().create_pool(tmp.as_raw_fd(), 64);
    // match a buffer on the part we wrote on
    let buffer = pool.create_buffer(0, 4, 4, 16, WlShmFormat::Argb8888 as u32);

    let surface = env.compositor.as_ref().unwrap().create_surface();

    let window = match DecoratedSurface::new(&surface, 16, 16,
        env.compositor.as_ref().unwrap(),
        env.subcompositor.as_ref().unwrap(),
        env.shm.as_ref().unwrap(),
        env.shell.as_ref().unwrap(),
        env.seat.take()
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

    env.display.sync_roundtrip().unwrap();

    loop {
        env.display.flush().unwrap();
        env.display.dispatch().unwrap();
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
    }
}

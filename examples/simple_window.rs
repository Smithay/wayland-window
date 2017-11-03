extern crate byteorder;
extern crate tempfile;
#[macro_use]
extern crate wayland_client;
extern crate wayland_protocols;
extern crate wayland_window;

use byteorder::{NativeEndian, WriteBytesExt};
use std::cmp;
use std::fs::File;
use std::io::{Seek, SeekFrom, Write};
use std::os::unix::io::AsRawFd;
use tempfile::tempfile;
use wayland_client::{EnvHandler, Proxy, StateToken};
use wayland_client::protocol::{wl_buffer, wl_compositor, wl_shell, wl_shm, wl_shm_pool, wl_subcompositor,
                               wl_surface};
use wayland_protocols::unstable::xdg_shell::v6::client::zxdg_shell_v6::{self, ZxdgShellV6};
use wayland_window::create_frame;

wayland_env!(
    WaylandEnv,
    compositor: wl_compositor::WlCompositor,
    subcompositor: wl_subcompositor::WlSubcompositor,
    shm: wl_shm::WlShm
);

struct Window {
    s: wl_surface::WlSurface,
    tmp: File,
    pool: wl_shm_pool::WlShmPool,
    pool_size: usize,
    buf: wl_buffer::WlBuffer,
    newsize: Option<(i32, i32)>,
    closed: bool,
    refresh: bool,
}

fn window_implementation() -> wayland_window::FrameImplementation<StateToken<Window>> {
    wayland_window::FrameImplementation {
        configure: |evqh, token, config, newsize| {
            if let Some((w, h)) = newsize {
                println!("configure newsize: {:?}", (w, h));
                evqh.state().get_mut(token).newsize = Some((w, h))
            }
            println!("configure metadata: {:?}", config);
            evqh.state().get_mut(token).refresh = true;
        },
        close: |evqh, token| {
            println!("close window");
            evqh.state().get_mut(token).closed = true;
        },
        refresh: |evqh, token| {
            evqh.state().get_mut(token).refresh = true;
        },
    }
}

impl Window {
    fn new(surface: wl_surface::WlSurface, shm: &wl_shm::WlShm) -> Window {
        // create a tempfile to write the contents of the window on
        let mut tmp = tempfile().ok().expect("Unable to create a tempfile.");
        // write the contents to it, lets put everything in dark red
        for _ in 0..16 {
            let _ = tmp.write_u32::<NativeEndian>(0xFF880000);
        }
        let _ = tmp.flush();
        let pool = shm.create_pool(tmp.as_raw_fd(), 64);
        let buffer = pool.create_buffer(0, 4, 4, 16, wl_shm::Format::Argb8888)
            .expect("I didn't destroy the pool!");
        Window {
            s: surface,
            tmp: tmp,
            pool: pool,
            pool_size: 64,
            buf: buffer,
            newsize: Some((200, 150)),
            closed: false,
            refresh: false,
        }
    }
    fn resize(&mut self, width: i32, height: i32) {
        // write the contents to it, lets put a nice color gradient
        self.tmp.seek(SeekFrom::Start(0)).unwrap();
        for i in 0..(width * height) {
            let x = (i % width) as u32;
            let y = (i / width) as u32;
            let w = width as u32;
            let h = height as u32;
            let r: u32 = cmp::min(((w - x) * 0xFF) / w, ((h - y) * 0xFF) / h);
            let g: u32 = cmp::min((x * 0xFF) / w, ((h - y) * 0xFF) / h);
            let b: u32 = cmp::min(((w - x) * 0xFF) / w, (y * 0xFF) / h);
            self.tmp
                .write_u32::<NativeEndian>((0xFF << 24) + (r << 16) + (g << 8) + b)
                .unwrap();
        }
        self.tmp.flush().unwrap();
        if (width * height * 4) as usize > self.pool_size {
            // the buffer has grown, notify the compositor
            self.pool.resize(width * height * 4);
            self.pool_size = (width * height * 4) as usize;
        }
        self.buf.destroy();
        self.buf = self.pool
            .create_buffer(0, width, height, width * 4, wl_shm::Format::Argb8888)
            .expect("Pool should not be dead!");
        self.s.attach(Some(&self.buf), 0, 0);
        self.s.commit();
    }
}

fn main() {
    let (display, mut event_queue) = match wayland_client::default_connect() {
        Ok(ret) => ret,
        Err(e) => panic!("Cannot connect to wayland server: {:?}", e),
    };

    let registry = display.get_registry();
    let env_token = EnvHandler::<WaylandEnv>::init(&mut event_queue, &registry);
    event_queue.sync_roundtrip().unwrap();

    // Use `xdg-shell` if its available. Otherwise, fall back to `wl-shell`.
    let (mut xdg_shell, mut wl_shell) = (None, None);
    {
        let state = event_queue.state();
        let env = state.get(&env_token);
        for &(name, ref interface, version) in env.globals() {
            if interface == ZxdgShellV6::interface_name() {
                xdg_shell = Some(registry.bind::<ZxdgShellV6>(version, name));
                break;
            }
        }

        if xdg_shell.is_none() {
            for &(name, ref interface, version) in env.globals() {
                if interface == wl_shell::WlShell::interface_name() {
                    wl_shell = Some(registry.bind::<wl_shell::WlShell>(version, name));
                    break;
                }
            }
        }
    }

    let shell = match (xdg_shell, wl_shell) {
        (Some(shell), _) => {
            // If using xdg-shell, we'll need to answer the pings.
            let shell_implementation = zxdg_shell_v6::Implementation {
                ping: |_, _, shell, serial| {
                    shell.pong(serial);
                },
            };
            event_queue.register(&shell, shell_implementation, ());
            wayland_window::Shell::Xdg(shell)
        }
        (_, Some(shell)) => wayland_window::Shell::Wl(shell),
        _ => panic!("No available shell"),
    };

    // get the env
    let env = event_queue.state().get(&env_token).clone_inner().unwrap();

    // prepare the Window
    let wl_surface = env.compositor.create_surface();
    let window_token = event_queue
        .state()
        .insert(Window::new(wl_surface.clone().unwrap(), &env.shm));

    // find the seat if any
    let seat = event_queue.state().with_value(&env_token, |_, env| {
        for &(id, ref interface, _) in env.globals() {
            if interface == "wl_seat" {
                return Some(registry.bind(1, id));
            }
        }
        None
    });

    let mut frame = create_frame(
        &mut event_queue,
        window_implementation(),
        window_token.clone(),
        &wl_surface,
        16,
        16,
        &env.compositor,
        &env.subcompositor,
        &env.shm,
        &shell,
        seat,
    ).unwrap();

    frame.set_title("My example window".into());
    frame.set_decorate(true);
    frame.set_min_size(Some((10, 10)));
    frame.refresh();

    loop {
        display.flush().unwrap();
        event_queue.dispatch().unwrap();

        // resize if needed
        let keep_going = event_queue.state().with_value(&window_token, |_, window| {
            if let Some((w, h)) = window.newsize.take() {
                frame.resize(w, h);
                window.resize(w, h);
                frame.refresh();
            } else if window.refresh {
                frame.refresh();
            }
            window.refresh = false;
            !window.closed
        });

        if !keep_going {
            break;
        }
    }
}

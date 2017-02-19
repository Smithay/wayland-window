use std::cell::Cell;
use std::ops::Deref;

use wayland_client::Proxy;
use wayland_client::cursor::{is_available, CursorTheme, load_theme};
use wayland_client::protocol::{wl_compositor,wl_shm,wl_surface,wl_pointer};

pub struct ThemedPointer {
    pointer: wl_pointer::WlPointer,
    surface: wl_surface::WlSurface,
    theme: CursorTheme,
    last_serial: Cell<u32>,
}

impl ThemedPointer {
    pub fn load(pointer: wl_pointer::WlPointer, name: Option<&str>,
                compositor: &wl_compositor::WlCompositor, shm: &wl_shm::WlShm)
        -> Result<ThemedPointer, wl_pointer::WlPointer>
    {
        if !is_available() { return Err(pointer) }

        let theme = load_theme(name, 16, shm);
        let surface = compositor.create_surface();

        Ok(ThemedPointer {
            pointer: pointer,
            surface: surface,
            theme: theme,
            last_serial: Cell::new(0)
        })
    }

    pub fn set_cursor(&self, name: &str, serial: Option<u32>) {
        let cursor = if let Some(c) = self.theme.get_cursor(name) { c } else { return };
        let buffer = if let Some(b) = cursor.frame_buffer(0) { b } else { return };
        let (w, h, hx, hy) = cursor.frame_info(0)
                                   .map(|(w,h,hx,hy,_)| (w as i32, h as i32, hx as i32, hy as i32))
                                   .unwrap_or((0,0, 0, 0));

        if let Some(s) = serial { self.last_serial.set(s); }

        self.surface.attach(Some(&buffer), 0, 0);
        if self.surface.version() >= 4 {
            self.surface.damage_buffer(0,0,w,h);
        } else {
            // surface is old and does not support damage_buffer, so we damage
            // in surface coordinates and hope it is not rescaled
            self.surface.damage(0,0,w,h);
        }
        self.surface.commit();
        self.pointer.set_cursor(self.last_serial.get(), Some(&self.surface), hx, hy);
    }
}

impl Deref for ThemedPointer {
    type Target = wl_pointer::WlPointer;
    fn deref(&self) -> &wl_pointer::WlPointer {
        &self.pointer
    }
}

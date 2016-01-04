use std::cell::Cell;
use std::ops::Deref;

use wayland_client::cursor::{is_available, CursorTheme, load_theme};
use wayland_client::wayland::compositor::{WlCompositor, WlSurface};
use wayland_client::wayland::shm::WlShm;
use wayland_client::wayland::seat::WlPointer;

pub struct ThemedPointer {
    pointer: WlPointer,
    surface: WlSurface,
    theme: CursorTheme,
    last_serial: Cell<u32>,
}

impl ThemedPointer {
    pub fn load(pointer: WlPointer, name: Option<&str>, compositor: &WlCompositor, shm: &WlShm)
        -> Result<ThemedPointer, WlPointer>
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
        let (hx, hy) = cursor.frame_info(0).map(|(_,_,hx,hy,_)| (hx as i32, hy as i32)).unwrap_or((0,0));

        if let Some(s) = serial { self.last_serial.set(s); }

        self.pointer.set_cursor(self.last_serial.get(), Some(&self.surface), hx, hy);

        self.surface.attach(Some(&buffer), 0, 0);
        self.surface.commit();
    }
}

impl Deref for ThemedPointer {
    type Target = WlPointer;
    fn deref(&self) -> &WlPointer {
        &self.pointer
    }
}
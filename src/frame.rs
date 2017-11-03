use Location;
use shell;
use std::fs::File;
use std::io::{Seek, SeekFrom, Write};
use std::os::unix::io::AsRawFd;
use std::sync::{Arc, Mutex};
use tempfile::tempfile;
use wayland_client::Proxy;
use wayland_client::protocol::*;

#[derive(Copy, Clone)]
pub(crate) struct FrameMetadata {
    pub(crate) dimensions: (i32, i32),
    pub(crate) decorate: bool,
    pub(crate) fullscreen: bool,
    pub(crate) maximized: bool,
    pub(crate) min_size: Option<(i32, i32)>,
    pub(crate) max_size: Option<(i32, i32)>,
    pub(crate) old_size: Option<(i32, i32)>,
    pub(crate) activated: bool,
    pub(crate) ready: bool,
    pub(crate) need_redraw: bool,
    pub(crate) ptr_location: Location,
}

impl FrameMetadata {
    pub(crate) fn clamp_to_limits(&self, size: (i32, i32)) -> (i32, i32) {
        use std::cmp::{max, min};
        let (mut w, mut h) = size;
        if self.decorate {
            let (ww, hh) = ::subtract_borders(w, h);
            w = ww;
            h = hh;
        }
        if let Some((minw, minh)) = self.min_size {
            w = max(minw, w);
            h = max(minh, h);
        }
        if let Some((maxw, maxh)) = self.max_size {
            w = min(maxw, w);
            h = min(maxh, h);
        }
        (w, h)
    }
}

/// A decorated frame for a window
///
/// This object allows you to interact with the shell_surface
/// and frame.
///
/// You'll at least need to use it to resize the borders when you window is
/// resized.
///
/// Dropping it will remove your window and unmap your wl_surface.
pub struct Frame {
    pub(crate) surface: wl_surface::WlSurface,
    contents: wl_subsurface::WlSubsurface,
    pub(crate) shell_surface: shell::Surface,
    buffer: Option<wl_buffer::WlBuffer>,
    tempfile: File,
    pool: wl_shm_pool::WlShmPool,
    pub(crate) pointer: Option<wl_pointer::WlPointer>,
    pub(crate) meta: Arc<Mutex<FrameMetadata>>,
    buffer_capacity: i32,
}

/// Possible requested state for a window
pub enum State<'output> {
    /// Regular floating window
    Regular,
    /// Minimized window
    Minimized,
    /// Maximized window
    Maximized,
    /// Fullscreen, with optional specification of an output to maximize over
    Fullscreen(Option<&'output wl_output::WlOutput>),
}

impl Frame {
    pub(crate) fn new(user_surface: &wl_surface::WlSurface, width: i32, height: i32,
                      compositor: &wl_compositor::WlCompositor,
                      subcompositor: &wl_subcompositor::WlSubcompositor, shm: &wl_shm::WlShm,
                      shell: &shell::Shell)
                      -> Result<Frame, ()> {
        if width <= 0 || height <= 0 {
            return Err(());
        }

        let tempfile = match tempfile() {
            Ok(t) => t,
            Err(_) => return Err(()),
        };

        match tempfile.set_len(100) {
            Ok(()) => {}
            Err(_) => return Err(()),
        };

        let pool = shm.create_pool(tempfile.as_raw_fd(), 100);

        let meta = Arc::new(Mutex::new(FrameMetadata {
            dimensions: (width, height),
            decorate: false,
            fullscreen: false,
            maximized: false,
            min_size: None,
            max_size: None,
            old_size: None,
            activated: true,
            ready: !shell.needs_readiness(),
            need_redraw: shell.needs_readiness(),
            ptr_location: Location::None,
        }));

        let frame_surface = compositor.create_surface();
        let contents = subcompositor
            .get_subsurface(&user_surface, &frame_surface)
            .expect("Provided Subcompositor was defunct");
        contents.set_position(0, 0);
        contents.set_desync();

        let shell_surface = shell::Surface::from_shell(&frame_surface, shell);

        let mut frame = Frame {
            surface: frame_surface,
            contents: contents,
            shell_surface: shell_surface,
            buffer: None,
            tempfile: tempfile,
            pool: pool,
            pointer: None,
            meta: meta,
            buffer_capacity: 100,
        };

        frame.redraw();

        Ok(frame)
    }

    pub(crate) fn redraw(&mut self) {
        let mut meta = self.meta.lock().unwrap();
        if !meta.ready {
            return;
        }

        if !meta.decorate || meta.fullscreen {
            // setup a dummy surface so that the subsurface does all
            // write a transparent buffer
            self.tempfile.seek(SeekFrom::Start(0)).unwrap();
            let _ = self.tempfile.write_all(&[0, 0, 0, 0]).unwrap();
            self.tempfile.flush().unwrap();
            if let Some(buffer) = self.buffer.take() {
                // TODO: better handling of buffer release
                buffer.destroy();
            }
            let buffer = self.pool
                .create_buffer(0, 1, 1, 4, wl_shm::Format::Argb8888)
                .expect("The pool cannot be defunct!");
            self.surface.attach(Some(&buffer), 0, 0);
            self.surface.commit();
            return
        }

        let (w, h) = meta.dimensions;
        let pxcount = ::theme::pxcount(w, h);

        if pxcount * 4 > self.buffer_capacity {
            // realloc needed!
            self.tempfile.set_len((pxcount * 4) as u64).unwrap();
            self.pool.resize(pxcount * 4);
            self.buffer_capacity = pxcount * 4;
        }
        // rewrite the data
        let mut mmap = unsafe {
            ::memmap::MmapOptions::new()
                .len(pxcount as usize * 4)
                .map_mut(&self.tempfile)
                .unwrap()
        };
        let _ = ::theme::draw_contents(
            &mut *mmap,
            w as u32,
            h as u32,
            meta.activated,
            meta.maximized,
            meta.max_size.is_none(),
            meta.ptr_location,
        );
        mmap.flush().unwrap();
        drop(mmap);

        // commit a new buffer
        if let Some(buffer) = self.buffer.take() {
            // TODO: better handling of buffer release
            buffer.destroy();
        }
        let (full_w, full_h) = ::theme::add_borders(w, h);
        let buffer = self.pool
            .create_buffer(0, full_w, full_h, full_w * 4, wl_shm::Format::Argb8888)
            .expect("The pool cannot be defunct!");
        self.surface.attach(Some(&buffer), 0, 0);
        // damage the surface
        if self.surface.version() >= 4 {
            self.surface.damage_buffer(0, 0, full_w, full_h);
        } else {
            // surface is old and does not support damage_buffer, so we damage
            // in surface coordinates and hope it is not rescaled
            self.surface.damage(0, 0, full_w, full_h);
        }
        self.surface.commit();
        self.buffer = Some(buffer);
        meta.need_redraw = false;
    }

    /// Refreshes the frame
    ///
    /// Redraws the frame to match its requested state (dimensions, presence/
    /// absence of decorations, ...)
    ///
    /// If the frame does not need a redraw, this method will do nothing,
    /// so don't be afraid to call it frequently.
    ///
    /// You need to call this method after every change to the dimensions or state
    /// of the decorations of your window, otherwise the drawn decorations may go
    /// out of sync with the state of your content.
    pub fn refresh(&mut self) {
        let need_redraw = self.meta.lock().unwrap().need_redraw;
        if need_redraw {
            self.redraw();
        }
    }

    /// Set a short title for the window.
    ///
    /// This string may be used to identify the surface in a task bar, window list, or other user
    /// interface elements provided by the compositor.
    pub fn set_title(&self, title: String) {
        self.shell_surface.set_title(title)
    }

    /// Set an app id for the surface.
    ///
    /// The surface class identifies the general class of applications to which the surface
    /// belongs.
    ///
    /// Several wayland compositors will try to find a `.desktop` file matching this name
    /// to find metadata about your apps.
    pub fn set_app_id(&self, app_id: String) {
        self.shell_surface.set_app_id(app_id)
    }

    /// Set wether the window should be decorated or not
    ///
    /// You need to call `refresh()` afterwards for this to properly
    /// take effect.
    pub fn set_decorate(&mut self, decorate: bool) {
        let mut meta = self.meta.lock().unwrap();
        meta.decorate = decorate;
        meta.need_redraw = true;
        if decorate {
            let (dx, dy) = ::theme::subsurface_offset();
            self.contents.set_position(dx, dy);
        } else {
            self.contents.set_position(0, 0);
        }
    }

    /// Resize the decorations
    ///
    /// You should call this whenever you change the size of the contents
    /// of your window, with the new _inner size_ of your window.
    ///
    /// You need to call `refresh()` afterwards for this to properly
    /// take effect.
    pub fn resize(&mut self, w: i32, h: i32) {
        use std::cmp::max;
        let w = max(w, 1);
        let h = max(h, 1);
        let mut meta = self.meta.lock().unwrap();
        meta.dimensions = (w, h);
        meta.need_redraw = true;
    }

    /// Sets the requested state of this surface
    pub fn set_state(&mut self, state: State) {
        match state {
            State::Regular => {
                self.shell_surface.unset_fullscreen();
                self.shell_surface.unset_maximized();
            }
            State::Minimized => {
                self.shell_surface.unset_fullscreen();
                self.shell_surface.set_minimized();
            }
            State::Maximized => {
                self.shell_surface.unset_fullscreen();
                self.shell_surface.set_maximized();
            }
            State::Fullscreen(output) => {
                self.shell_surface.set_fullscreen(output);
            }
        }
    }

    /// Sets the minimum possible size for this window
    ///
    /// Provide either a tuple `Some((width, height))` or `None` to unset the
    /// minimum size.
    ///
    /// The provided size is the interior size, not counting decorations
    pub fn set_min_size(&mut self, size: Option<(i32, i32)>) {
        let decorate = {
            let mut meta = self.meta.lock().unwrap();
            meta.min_size = size;
            meta.decorate
        };
        self.shell_surface
            .set_min_size(size.map(|(w, h)| if decorate {
                ::theme::add_borders(w, h)
            } else {
                (w, h)
            }));
    }

    /// Sets the maximum possible size for this window
    ///
    /// Provide either a tuple `Some((width, height))` or `None` to unset the
    /// maximum size.
    ///
    /// The provided size is the interior size, not counting decorations
    pub fn set_max_size(&mut self, size: Option<(i32, i32)>) {
        let decorate = {
            let mut meta = self.meta.lock().unwrap();
            meta.max_size = size;
            meta.decorate
        };
        self.shell_surface
            .set_max_size(size.map(|(w, h)| if decorate {
                ::theme::add_borders(w, h)
            } else {
                (w, h)
            }));
    }
}

impl Drop for Frame {
    fn drop(&mut self) {
        self.shell_surface.destroy();
        self.surface.destroy();
        self.contents.destroy();
        if let Some(buffer) = self.buffer.take() {
            buffer.destroy();
        }
        self.pool.destroy();
        if let Some(ref pointer) = self.pointer {
            if pointer.version() >= 3 {
                pointer.release();
            }
        }
    }
}

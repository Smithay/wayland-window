use FrameIData;
use wayland_client::{EventQueueHandle, Proxy};
use wayland_client::protocol::*;
use wayland_protocols::unstable::xdg_shell::v6 as xdg_shell;

mod xdg;
mod wl;

/// Enum over the supported shells
pub enum Shell {
    /// A xdg_shell from unstable v6
    Xdg(xdg_shell::client::zxdg_shell_v6::ZxdgShellV6),
    /// A wl_shell
    Wl(wl_shell::WlShell),
}

impl Shell {
    pub(crate) fn needs_readiness(&self) -> bool {
        match *self {
            Shell::Xdg(_) => true,
            Shell::Wl(_) => false,
        }
    }
}

pub(crate) enum Surface {
    Xdg(self::xdg::Surface),
    Wl(wl_shell_surface::WlShellSurface),
}

/// Configure data for a decorated surface handler.
#[derive(Debug, Clone)]
pub enum Configure {
    /// Configure data from xdg_shell
    Xdg(Vec<xdg_shell::client::zxdg_toplevel_v6::State>),
    /// Configure data from wl_shell
    Wl(wl_shell_surface::Resize),
}

impl Surface {
    pub fn from_shell(surface: &wl_surface::WlSurface, shell: &Shell) -> Self {
        match *shell {
            // Create the `xdg_surface` and assign the `toplevel` role.
            Shell::Xdg(ref shell) => {
                let xdg_surface = shell
                    .get_xdg_surface(surface)
                    .expect("shell cannot be destroyed");
                let toplevel = xdg_surface
                    .get_toplevel()
                    .expect("xdg_surface cannot be destroyed");
                surface.commit();
                Surface::Xdg(self::xdg::Surface {
                    surface: xdg_surface,
                    toplevel: toplevel,
                })
            }

            // Create a `wl_shell_surface` and set it as the `toplevel`.
            Shell::Wl(ref shell) => {
                let shell_surface = shell.get_shell_surface(surface);
                shell_surface.set_toplevel();
                Surface::Wl(shell_surface)
            }
        }
    }

    pub(crate) fn clone(&self) -> Option<Surface> {
        match *self {
            Surface::Xdg(ref s) => s.clone().map(Surface::Xdg),
            Surface::Wl(ref s) => s.clone().map(Surface::Wl),
        }
    }

    pub(crate) fn register_to<ID: 'static>(&self, evqh: &mut EventQueueHandle, idata: FrameIData<ID>) {
        match *self {
            Surface::Xdg(ref xdg) => {
                evqh.register(
                    &xdg.toplevel,
                    self::xdg::xdg_toplevel_implementation(),
                    idata.clone(),
                );
                evqh.register(&xdg.surface, self::xdg::xdg_surface_implementation(), idata);
            }
            Surface::Wl(ref shell_surface) => {
                evqh.register(
                    shell_surface,
                    self::wl::wl_shell_surface_implementation(),
                    idata,
                );
            }
        }
    }

    pub(crate) fn destroy(&self) {
        match *self {
            Surface::Xdg(ref xdg) => xdg.destroy(),
            Surface::Wl(ref _shell_surface) => { /* we can't destroy it :'( */ }
        }
    }

    pub(crate) fn resize(&self, seat: &wl_seat::WlSeat, serial: u32, direction: wl_shell_surface::Resize) {
        match *self {
            Surface::Xdg(ref xdg) => {
                xdg.toplevel.resize(seat, serial, direction.to_raw());
            }
            Surface::Wl(ref shell_surface) => shell_surface.resize(seat, serial, direction),
        }
    }

    pub(crate) fn _move(&self, seat: &wl_seat::WlSeat, serial: u32) {
        match *self {
            Surface::Xdg(ref xdg) => {
                xdg.toplevel._move(seat, serial);
            }
            Surface::Wl(ref shell_surface) => shell_surface._move(seat, serial),
        }
    }

    pub(crate) fn set_title(&self, title: String) {
        match *self {
            Surface::Xdg(ref xdg) => {
                xdg.toplevel.set_title(title);
            }
            Surface::Wl(ref wl) => {
                wl.set_title(title);
            }
        }
    }

    pub(crate) fn set_app_id(&self, title: String) {
        match *self {
            Surface::Xdg(ref xdg) => {
                xdg.toplevel.set_app_id(title);
            }
            Surface::Wl(ref wl) => {
                wl.set_class(title);
            }
        }
    }

    pub(crate) fn set_fullscreen(&self, output: Option<&wl_output::WlOutput>) {
        match *self {
            Surface::Xdg(ref xdg) => {
                xdg.toplevel.set_fullscreen(output);
            }
            Surface::Wl(ref wl) => {
                let method = wl_shell_surface::FullscreenMethod::Default;
                let framerate = 0; // Let the server decide the framerate.
                wl.set_fullscreen(method, framerate, output);
            }
        }
    }

    pub(crate) fn unset_fullscreen(&self) {
        match *self {
            Surface::Xdg(ref xdg) => {
                xdg.toplevel.unset_fullscreen();
            }
            Surface::Wl(ref wl) => {
                wl.set_toplevel();
            }
        }
    }

    pub(crate) fn set_maximized(&self) {
        match *self {
            Surface::Xdg(ref xdg) => {
                xdg.toplevel.set_maximized();
            }
            Surface::Wl(ref wl) => {
                wl.set_maximized(None);
            }
        }
    }

    pub(crate) fn unset_maximized(&self) {
        match *self {
            Surface::Xdg(ref xdg) => {
                xdg.toplevel.unset_maximized();
            }
            Surface::Wl(ref wl) => {
                wl.set_toplevel();
            }
        }
    }

    pub(crate) fn set_minimized(&self) {
        match *self {
            Surface::Xdg(ref xdg) => {
                xdg.toplevel.set_minimized();
            }
            Surface::Wl(_) => { /* not available */ }
        }
    }

    pub(crate) fn set_min_size(&self, size: Option<(i32, i32)>) {
        if let Surface::Xdg(ref xdg) = *self {
            let (w, h) = size.unwrap_or((0, 0));
            xdg.toplevel.set_min_size(w, h);
        }
    }

    pub(crate) fn set_max_size(&self, size: Option<(i32, i32)>) {
        if let Surface::Xdg(ref xdg) = *self {
            let (w, h) = size.unwrap_or((0, 0));
            xdg.toplevel.set_max_size(w, h);
        }
    }
}

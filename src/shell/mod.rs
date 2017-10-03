use decorated_surface::DecoratedSurfaceIData;
use wayland_client::{EventQueueHandle, Proxy};
use wayland_client::protocol::{wl_shell, wl_shell_surface, wl_surface};
use wayland_protocols::unstable::xdg_shell;

mod xdg;
mod wl;

pub enum Shell {
    Xdg(xdg_shell::client::zxdg_shell_v6::ZxdgShellV6),
    Wl(wl_shell::WlShell),
}

pub(crate) enum Surface {
    Xdg(self::xdg::Surface),
    Wl(wl_shell_surface::WlShellSurface),
}

/// Configure data for a decorated surface handler.
#[derive(Debug, Clone)]
pub enum Configure {
    Xdg(Vec<xdg_shell::client::zxdg_toplevel_v6::State>),
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

    pub(crate) fn register_to<ID: 'static>(&self, evqh: &mut EventQueueHandle,
                                           idata: DecoratedSurfaceIData<ID>) {
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
}

use wayland_client::protocol::{wl_shell, wl_shell_surface, wl_surface};
use wayland_protocols::unstable::xdg_shell;

mod xdg;
mod wl;

pub enum Shell {
    Xdg(xdg_shell::client::zxdg_shell_v6::ZxdgShellV6),
    Wl(wl_shell::WlShell),
}

pub enum Surface {
    Xdg(self::xdg::Surface),
    Wl(wl_shell_surface::WlShellSurface),
}

/// Configure data for a decorated surface handler.
pub enum Configure {
    Xdg,
    Wl(wl_shell_surface::Resize),
}

impl Surface {

    pub fn from_shell(surface: &wl_surface::WlSurface, shell: &Shell) -> Self {
        match *shell {

            // Create the `xdg_surface` and assign the `toplevel` role.
            Shell::Xdg(ref shell) => {
                let xdg_surface = shell.get_xdg_surface(surface).expect("shell cannot be destroyed");
                let toplevel = xdg_surface.get_toplevel().expect("xdg_surface cannot be destroyed");
                surface.commit();
                Surface::Xdg(self::xdg::Surface {
                    surface: xdg_surface,
                    toplevel: toplevel,
                })
            },

            // Create a `wl_shell_surface` and set it as the `toplevel`.
            Shell::Wl(ref shell) => {
                let shell_surface = shell.get_shell_surface(surface);
                shell_surface.set_toplevel();
                Surface::Wl(shell_surface)
            },

        }
    }

}

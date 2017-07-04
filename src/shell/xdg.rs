use decorated_surface::{self, DecoratedSurface, Handler as UserHandler};
use wayland_client::EventQueueHandle;
use wayland_protocols::unstable::xdg_shell;
use wayland_protocols::unstable::xdg_shell::client::zxdg_toplevel_v6::{self, ZxdgToplevelV6};
use wayland_protocols::unstable::xdg_shell::client::zxdg_surface_v6::{self, ZxdgSurfaceV6};

pub struct Surface {
    pub toplevel: ZxdgToplevelV6,
    pub surface: ZxdgSurfaceV6,
}


/////////////////////////////////////////
// xdg_shell `Handler` implementations //
/////////////////////////////////////////

declare_handler!(DecoratedSurface<H: [UserHandler]>, zxdg_toplevel_v6::Handler, ZxdgToplevelV6);

declare_handler!(DecoratedSurface<H: [UserHandler]>, zxdg_surface_v6::Handler, ZxdgSurfaceV6);

impl<H> xdg_shell::client::zxdg_toplevel_v6::Handler for DecoratedSurface<H>
    where H: decorated_surface::Handler,
{

    fn configure(
        &mut self,
        evqh: &mut EventQueueHandle,
        _proxy: &ZxdgToplevelV6,
        width: i32, height: i32,
        _states: Vec<u8>,
    ) {
        // NOTE: Not sure if/how `_states` should be handled here.
        if let Some(handler) = decorated_surface::handler_mut(self) {
            let (w, h) = decorated_surface::subtract_borders(width, height);
            let configure = super::Configure::Xdg;
            handler.configure(evqh, configure, w, h);
        }
    }

    fn close(&mut self, evqh: &mut EventQueueHandle, _proxy: &ZxdgToplevelV6) {
        if let Some(handler) = decorated_surface::handler_mut(self) {
            handler.close(evqh);
        }
    }
}

impl<H> xdg_shell::client::zxdg_surface_v6::Handler for DecoratedSurface<H>
    where H: decorated_surface::Handler,
{

    fn configure(
        &mut self,
        _evqh: &mut EventQueueHandle,
        _proxy: &ZxdgSurfaceV6,
        serial: u32,
    ) {
        if let super::Surface::Xdg(ref xdg) = *decorated_surface::shell_surface(self) {
            xdg.surface.ack_configure(serial).expect("surface cannot be destroyed");
        }
    }

}

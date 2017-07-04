use decorated_surface::{self, DecoratedSurface, Handler as UserHandler};
use wayland_client::EventQueueHandle;
use wayland_client::protocol::wl_shell_surface;

////////////////////////////////////////
// wl_shell `Handler` implementations //
////////////////////////////////////////


impl<H> wl_shell_surface::Handler for DecoratedSurface<H>
    where H: UserHandler,
{
    fn ping(
        &mut self,
        _: &mut EventQueueHandle,
        me: &wl_shell_surface::WlShellSurface,
        serial: u32,
    ) {
        me.pong(serial);
    }

    fn configure(
        &mut self,
        evqh: &mut EventQueueHandle,
        _: &wl_shell_surface::WlShellSurface,
        edges: wl_shell_surface::Resize,
        width: i32,
        height: i32,
    ) {
        if let Some(handler) = decorated_surface::handler_mut(self) {
            let (w, h) = decorated_surface::subtract_borders(width, height);
            let configure = super::Configure::Wl(edges);
            handler.configure(evqh, configure, w, h)
        }
    }
}

declare_handler!(DecoratedSurface<H: [UserHandler]>, wl_shell_surface::Handler, wl_shell_surface::WlShellSurface);

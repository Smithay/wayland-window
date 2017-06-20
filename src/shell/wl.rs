use decorated_surface::{self, DecoratedSurface};
use wayland_client::{self, EventQueueHandle};
use wayland_client::protocol::wl_shell_surface;

////////////////////////////////////////
// wl_shell `Handler` implementations //
////////////////////////////////////////


impl<H> wl_shell_surface::Handler for DecoratedSurface<H>
    where H: decorated_surface::Handler,
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

unsafe impl<H> wayland_client::Handler<wl_shell_surface::WlShellSurface> for DecoratedSurface<H>
    where H: decorated_surface::Handler
{
    unsafe fn message(
        &mut self,
        evq: &mut EventQueueHandle,
        proxy: &wl_shell_surface::WlShellSurface,
        opcode: u32,
        args: *const wayland_client::sys::wl_argument,
    ) -> Result<(),()>
    {
        <DecoratedSurface<H> as wayland_client::protocol::wl_shell_surface::Handler>::__message(
            self, evq, proxy, opcode, args
        )
    }
}

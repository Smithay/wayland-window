use decorated_surface::DecoratedSurfaceIData;
use wayland_client::protocol::wl_shell_surface;

pub(crate) fn wl_shell_surface_implementation<ID>(
    )
    -> wl_shell_surface::Implementation<DecoratedSurfaceIData<ID>>
{
    wl_shell_surface::Implementation {
        ping: |_, _, shell_surface, serial| {
            shell_surface.pong(serial);
        },
        configure: |evqh, idata, _, edges, width, height| {
            let newsize = idata.pointer_state.borrow()
                .clamp_to_limits((width, height));
            let configure = super::Configure::Wl(edges);
            let mut user_idata = idata.idata.borrow_mut();
            (idata.implementation.configure)(evqh, &mut *user_idata, configure, Some(newsize))
        },
        popup_done: |_, _, _| {
            // We are not doing popups
        },
    }
}

use FrameIData;
use wayland_client::Proxy;
use wayland_protocols::unstable::xdg_shell::v6::client::zxdg_surface_v6::{self, ZxdgSurfaceV6};
use wayland_protocols::unstable::xdg_shell::v6::client::zxdg_toplevel_v6::{self, ZxdgToplevelV6};

pub(crate) struct Surface {
    pub toplevel: ZxdgToplevelV6,
    pub surface: ZxdgSurfaceV6,
}

impl Surface {
    pub(crate) fn clone(&self) -> Option<Surface> {
        match (self.toplevel.clone(), self.surface.clone()) {
            (Some(t), Some(s)) => Some(Surface {
                toplevel: t,
                surface: s,
            }),
            _ => None,
        }
    }

    pub(crate) fn destroy(&self) {
        // destroy surfaces in the right order
        self.toplevel.destroy();
        self.surface.destroy();
    }
}

pub(crate) fn xdg_toplevel_implementation<ID>() -> zxdg_toplevel_v6::Implementation<FrameIData<ID>> {
    zxdg_toplevel_v6::Implementation {
        configure: |evqh, idata, _, width, height, states| {
            idata.meta.lock().unwrap().need_redraw = true;
            let newsize = if width == 0 || height == 0 {
                // if either w or h is zero, then we get to choose our size
                None
            } else {
                Some(idata.meta.lock().unwrap().clamp_to_limits((width, height)))
            };
            let view: &[u32] =
                unsafe { ::std::slice::from_raw_parts(states.as_ptr() as *const _, states.len() / 4) };
            let configure = super::Configure::Xdg(
                // we ignore unknown values
                view.iter()
                    .cloned()
                    .flat_map(zxdg_toplevel_v6::State::from_raw)
                    .collect(),
            );
            let mut user_idata = idata.idata.borrow_mut();
            (idata.implementation.configure)(evqh, &mut *user_idata, configure, newsize);
        },
        close: |evqh, idata, _| {
            let mut user_idata = idata.idata.borrow_mut();
            (idata.implementation.close)(evqh, &mut *user_idata);
        },
    }
}

pub(crate) fn xdg_surface_implementation<ID>() -> zxdg_surface_v6::Implementation<FrameIData<ID>> {
    zxdg_surface_v6::Implementation {
        configure: |_, idata, xdg_surface, serial| {
            idata.meta.lock().unwrap().ready = true;
            xdg_surface.ack_configure(serial);
        },
    }
}

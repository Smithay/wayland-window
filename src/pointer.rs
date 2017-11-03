use {shell, Location, PointerIData, UIButton};
use frame::FrameMetadata;
use std::sync::{Arc, Mutex};
use theme::compute_location;
use themed_pointer::ThemedPointer;
use wayland_client::Proxy;
use wayland_client::protocol::{wl_pointer, wl_seat, wl_shell_surface, wl_surface};

pub(crate) enum Pointer {
    Plain(wl_pointer::WlPointer),
    Themed(ThemedPointer),
}

impl Drop for Pointer {
    fn drop(&mut self) {
        if let Pointer::Plain(ref pointer) = *self {
            pointer.release();
        }
    }
}

pub(crate) struct PointerState {
    location: Location,
    coordinates: (f64, f64),
    pointer: Pointer,
    shell_surface: shell::Surface,
    frame_surface: wl_surface::WlSurface,
    seat: wl_seat::WlSeat,
    meta: Arc<Mutex<FrameMetadata>>,
}

impl PointerState {
    pub(crate) fn new(meta: Arc<Mutex<FrameMetadata>>, pointer: Pointer,
                      frame_surface: wl_surface::WlSurface, shell_surface: shell::Surface,
                      seat: wl_seat::WlSeat)
                      -> PointerState {
        PointerState {
            location: Location::None,
            coordinates: (0., 0.),
            meta: meta,
            pointer: pointer,
            frame_surface: frame_surface,
            shell_surface: shell_surface,
            seat: seat,
        }
    }

    fn pointer_entered(&mut self, surface: &wl_surface::WlSurface, serial: u32) {
        if self.frame_surface.equals(surface) {
            self.update(Some(serial), true);
        } else {
            // A surface that we don't manage
            self.meta.lock().unwrap().ptr_location = Location::None;
            self.location = Location::None;
        }
    }

    fn pointer_left(&mut self, serial: u32) {
        self.meta.lock().unwrap().ptr_location = Location::None;
        self.location = Location::None;
        self.change_pointer(Location::None, Some(serial))
    }

    fn update(&mut self, serial: Option<u32>, force: bool) -> bool {
        let mut meta = self.meta.lock().unwrap();
        let new_location = if meta.decorate && !meta.fullscreen {
            compute_location(self.coordinates, meta.dimensions)
        } else {
            Location::Inside
        };

        if new_location != self.location || force {
            // a button is hovered, we need a redraw
            if let Location::Button(_) = self.location {
                meta.need_redraw = true;
            }
            if let Location::Button(_) = new_location {
                meta.need_redraw = true;
            }
            self.location = new_location;
            self.change_pointer(new_location, serial);
            meta.ptr_location = new_location;
        }
        return meta.need_redraw;
    }

    fn change_pointer(&self, location: Location, serial: Option<u32>) {
        let name = match location {
            Location::Top => "top_side",
            Location::TopRight => "top_right_corner",
            Location::Right => "right_side",
            Location::BottomRight => "bottom_right_corner",
            Location::Bottom => "bottom_side",
            Location::BottomLeft => "bottom_left_corner",
            Location::Left => "left_side",
            Location::TopLeft => "top_left_corner",
            _ => "left_ptr",
        };
        if let Pointer::Themed(ref themed) = self.pointer {
            themed.set_cursor(name, serial);
        }
    }
}

pub(crate) fn pointer_implementation<ID>() -> wl_pointer::Implementation<PointerIData<ID>> {
    wl_pointer::Implementation {
        enter: |_, idata, _, serial, surface, x, y| {
            idata.pstate.coordinates = (x, y);
            idata.pstate.pointer_entered(surface, serial);
        },
        leave: |_, idata, _, serial, _| {
            idata.pstate.pointer_left(serial);
        },
        motion: |evqh, idata, _, _, x, y| if idata.pstate.location != Location::None {
            idata.pstate.coordinates = (x, y);
            let need_redraw = idata.pstate.update(None, false);
            if need_redraw {
                let mut user_idata = idata.idata.borrow_mut();
                (idata.implementation.refresh)(evqh, &mut *user_idata);
            }
        },
        button: |evqh, idata, _, serial, _, button, state| {
            if button != 0x110 {
                return;
            }
            if let wl_pointer::ButtonState::Released = state {
                return;
            }
            match compute_pointer_action(idata.pstate.location) {
                PointerAction::Resize(direction) => idata.pstate.shell_surface.resize(
                    &idata.pstate.seat,
                    serial,
                    direction,
                ),
                PointerAction::Move => idata.pstate.shell_surface._move(&idata.pstate.seat, serial),
                PointerAction::Button(b) => match b {
                    UIButton::Minimize => {
                        idata.pstate.shell_surface.set_minimized();
                    }
                    UIButton::Maximize => {
                        let maximize = {
                            let meta = idata.pstate.meta.lock().unwrap();
                            if meta.max_size.is_some() {
                                // there is a max size, the button is greyed
                                return;
                            }
                            !meta.maximized
                        };
                        if maximize {
                            idata.pstate.shell_surface.set_maximized();
                        } else {
                            idata.pstate.shell_surface.unset_maximized();
                        }
                    }
                    UIButton::Close => {
                        let mut user_idata = idata.idata.borrow_mut();
                        (idata.implementation.close)(evqh, &mut *user_idata);
                    }
                },
                PointerAction::None => {}
            }
        },
        axis: |_, _, _, _, _, _| {},
        axis_discrete: |_, _, _, _, _| {},
        axis_source: |_, _, _, _| {},
        axis_stop: |_, _, _, _, _| {},
        frame: |_, _, _| {},
    }
}

enum PointerAction {
    Resize(wl_shell_surface::Resize),
    Move,
    None,
    Button(UIButton),
}

fn compute_pointer_action(location: Location) -> PointerAction {
    use self::wl_shell_surface::Resize;
    match location {
        Location::Top => PointerAction::Resize(Resize::Top),
        Location::TopLeft => PointerAction::Resize(Resize::TopLeft),
        Location::Left => PointerAction::Resize(Resize::Left),
        Location::BottomLeft => PointerAction::Resize(Resize::BottomLeft),
        Location::Bottom => PointerAction::Resize(Resize::Bottom),
        Location::BottomRight => PointerAction::Resize(Resize::BottomRight),
        Location::Right => PointerAction::Resize(Resize::Right),
        Location::TopRight => PointerAction::Resize(Resize::TopRight),
        Location::TopBar => PointerAction::Move,
        Location::Button(b) => PointerAction::Button(b),
        Location::None | Location::Inside => PointerAction::None,
    }
}

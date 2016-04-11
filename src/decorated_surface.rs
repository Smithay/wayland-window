use std::cmp::max;
use std::fs::File;
use std::io::{Seek, SeekFrom, Write};
use std::os::unix::io::AsRawFd;

use byteorder::{WriteBytesExt, NativeEndian};

use tempfile::tempfile;

use wayland_client::{EventIterator, ProxyId, Proxy, Event};
use wayland_client::wayland::compositor::{WlCompositor, WlSurface};
use wayland_client::wayland::seat::{WlSeat, WlPointer, WlPointerButtonState};
use wayland_client::wayland::shell::{WlShell, WlShellSurface, WlShellSurfaceResize};
use wayland_client::wayland::shm::{WlBuffer, WlShm, WlShmPool,WlShmFormat};
use wayland_client::wayland::subcompositor::{WlSubcompositor, WlSubsurface};

use super::themed_pointer::ThemedPointer;

// The surfaces handling the borders, 8 total, are organised this way:
//
//        0
// ---|-------|---
//    |       |
//  3 | user  | 1
//    |       |
// ---|-------|---
//        2
//
pub const BORDER_TOP         : usize = 0;
pub const BORDER_RIGHT       : usize = 1;
pub const BORDER_BOTTOM      : usize = 2;
pub const BORDER_LEFT        : usize = 3;

const DECORATION_SIZE     : i32 = 8;
const DECORATION_TOP_SIZE : i32 = 24;

#[derive(Debug,Copy,Clone,PartialEq,Eq)]
enum PtrLocation {
    None,
    Top,
    Right,
    Bottom,
    Left
}

enum Pointer {
    Plain(WlPointer),
    Themed(ThemedPointer),
    None
}

struct PointerState {
    surfaces: Vec<ProxyId>,
    location: PtrLocation,
    coordinates: (f64, f64),
    cornered: bool,
    topped: bool,
    surface_width: i32,
    pointer: Pointer
}

impl PointerState {
    fn pointer_entered(&mut self, sid: ProxyId, serial: u32) {
        if self.surfaces[BORDER_TOP] == sid {
            self.location = PtrLocation::Top;
        } else if self.surfaces[BORDER_RIGHT] == sid {
            self.location = PtrLocation::Right
        } else if self.surfaces[BORDER_BOTTOM] == sid {
            self.location = PtrLocation::Bottom;
        } else if self.surfaces[BORDER_LEFT] == sid {
            self.location = PtrLocation::Left
        } else {
            // should probably never happen ?
            self.location = PtrLocation::None;
        }
        self.update(Some(serial), true);
    }

    fn pointer_left(&mut self) {
        self.location = PtrLocation::None;
        self.change_pointer("left_ptr", None)
    }

    fn update(&mut self, serial: Option<u32>, force: bool) {
        let old_cornered = self.cornered;
        self.cornered = (self.location == PtrLocation::Top || self.location == PtrLocation::Bottom) &&
                        (self.coordinates.0 <= DECORATION_SIZE as f64 ||
                         self.coordinates.0 >= (self.surface_width + DECORATION_SIZE) as f64);
        let old_topped = self.topped;
        self.topped = self.location == PtrLocation::Top && self.coordinates.1 <= DECORATION_SIZE as f64;
        if force || (self.cornered ^ old_cornered) || (old_topped ^ self.topped) {
            let name = if self.cornered {
                match self.location {
                    PtrLocation::Top => if self.coordinates.0 <= DECORATION_SIZE as f64 {
                        "top_left_corner"
                    } else {
                        "top_right_corner"
                    },
                    PtrLocation::Bottom => if self.coordinates.0 <= DECORATION_SIZE as f64 {
                        "bottom_left_corner"
                    } else {
                        "bottom_right_corner"
                    },
                    _ => unreachable!()
                }
            } else {
                match self.location {
                    PtrLocation::Top => if self.topped { "top_side" } else { "left_ptr" },
                    PtrLocation::Bottom => "bottom_side",
                    PtrLocation::Right => "right_side",
                    PtrLocation::Left => "left_side",
                    _ => "left_ptr"
                }
            };
            self.change_pointer(name, serial)
        }
    }

    fn change_pointer(&self, name: &str, serial: Option<u32>) {
        if let Pointer::Themed(ref themed) = self.pointer {
            themed.set_cursor(name, serial);
        }
    }
}

/// A wrapper for a decorated surface.
///
/// This is the main object of this crate. It wraps a user provided
/// wayland surface into a `ShellSurface` and gives you acces to it
/// via the `.get_shell()` method.
///
/// It also handles the drawing of minimalistic borders allowing the
/// resizing and moving of the window. See the root documentation of
/// this crate for explanations about how to use it.
pub struct DecoratedSurface {
    shell_surface: WlShellSurface,
    border_surfaces: Vec<(WlSurface, WlSubsurface)>,
    buffers: Vec<WlBuffer>,
    tempfile: File,
    pool: WlShmPool,
    height: i32,
    width: i32,
    buffer_capacity: usize,
    pointer_state: PointerState,
    eventiter: EventIterator,
    seat: Option<WlSeat>
}

impl DecoratedSurface {
    /// Resizes the borders to given width and height.
    ///
    /// These values should be the dimentions of the internal surface of the
    /// window (the decorated window will thus be a little larger).
    pub fn resize(&mut self, width: i32, height: i32) {
        let new_pxcount = max(DECORATION_TOP_SIZE * (DECORATION_SIZE * 2 + width),
            max(DECORATION_TOP_SIZE * width, DECORATION_SIZE * height)
        ) as usize;
        if new_pxcount * 4 > self.buffer_capacity {
            // reallocation needed !
            self.tempfile.set_len((new_pxcount * 4) as u64).unwrap();
            self.pool.resize((new_pxcount * 4) as i32);
            self.buffer_capacity = new_pxcount * 4;
        }
        self.width = width;
        self.height = height;
        self.pointer_state.surface_width = width;
        // rewrite the data
        self.tempfile.seek(SeekFrom::Start(0)).unwrap();
        for _ in 0..(new_pxcount*4) {
            // write a dark gray
            let _ = self.tempfile.write_u32::<NativeEndian>(0xFF444444);
        }
        self.tempfile.flush().unwrap();
        // resize the borders
        self.buffers.clear();
        // top
        {
            let buffer = self.pool.create_buffer(
                0,
                self.width as i32 + (DECORATION_SIZE as i32) * 2,
                DECORATION_TOP_SIZE as i32,
                (self.width as i32 + (DECORATION_SIZE as i32) * 2) * 4,
                WlShmFormat::Argb8888 as u32
            );
            self.border_surfaces[BORDER_TOP].0.attach(Some(&buffer), 0, 0);
            self.border_surfaces[BORDER_TOP].1.set_position(
                -(DECORATION_SIZE as i32),
                -(DECORATION_TOP_SIZE as i32)
            );
            self.buffers.push(buffer);
        }
        // right
        {
            let buffer = self.pool.create_buffer(
                0, DECORATION_SIZE as i32,
                self.height as i32, (DECORATION_SIZE*4) as i32,
                WlShmFormat::Argb8888 as u32
            );
            self.border_surfaces[BORDER_RIGHT].0.attach(Some(&buffer), 0, 0);
            self.border_surfaces[BORDER_RIGHT].1.set_position(self.width as i32, 0);
            self.buffers.push(buffer);
        }
        // bottom
        {
            let buffer = self.pool.create_buffer(
                0,
                self.width as i32 + (DECORATION_SIZE as i32) * 2,
                DECORATION_SIZE as i32,
                (self.width as i32 + (DECORATION_SIZE as i32) * 2) * 4,
                WlShmFormat::Argb8888 as u32
            );
            self.border_surfaces[BORDER_BOTTOM].0.attach(Some(&buffer), 0, 0);
            self.border_surfaces[BORDER_BOTTOM].1.set_position(-(DECORATION_SIZE as i32), self.height as i32);
            self.buffers.push(buffer);
        }
        // left
        {
            let buffer = self.pool.create_buffer(
                0, DECORATION_SIZE as i32,
                self.height as i32, (DECORATION_SIZE*4) as i32,
                WlShmFormat::Argb8888 as u32
            );
            self.border_surfaces[BORDER_LEFT].0.attach(Some(&buffer), 0, 0);
            self.border_surfaces[BORDER_LEFT].1.set_position(-(DECORATION_SIZE as i32), 0);
            self.buffers.push(buffer);
        }

        for s in &self.border_surfaces { s.0.commit(); }
    }

    /// Creates a new decorated window around given surface.
    pub fn new(surface: &WlSurface, width: i32, height: i32,
               compositor: &WlCompositor, subcompositor: &WlSubcompositor,
               shm: &WlShm, shell: &WlShell, seat: Option<WlSeat>)
        -> Result<DecoratedSurface, ()>
    {
        let evts = EventIterator::new();
        // handle Shm
        let pxcount = max(DECORATION_TOP_SIZE * DECORATION_SIZE,
            max(DECORATION_TOP_SIZE * width, DECORATION_SIZE * height)
        ) as usize;

        let tempfile = match tempfile() {
            Ok(t) => t,
            Err(_) => return Err(())
        };

        match tempfile.set_len((pxcount *4) as u64) {
            Ok(()) => {},
            Err(_) => return Err(())
        };

        let mut pool = shm.create_pool(tempfile.as_raw_fd(), (pxcount * 4) as i32);
        pool.set_evt_iterator(&evts);

        // create surfaces
        let border_surfaces: Vec<_> = (0..4).map(|_| {
            let mut s = compositor.create_surface();
            s.set_evt_iterator(&evts);
            let mut ss = subcompositor.get_subsurface(&s, surface);
            ss.set_evt_iterator(&evts);
            (s, ss)
        }).collect();
        for s in &border_surfaces { s.1.set_desync() }

        let mut shell_surface = shell.get_shell_surface(surface);
        shell_surface.set_evt_iterator(&evts);
        shell_surface.set_toplevel();

        // Pointer
        let pointer_state = {
            let mut surfaces = Vec::with_capacity(4);
            let pointer = seat.as_ref().map(|seat| seat.get_pointer())
                              .map(|mut pointer| {
                // let (mut pointer, _) = pointer.set_cursor(Some(comp.create_surface()), (0,0));
                for s in &border_surfaces {
                    surfaces.push(s.0.id());
                }
                pointer.set_evt_iterator(&evts);
                pointer
            });

            let pointer = match pointer.map(|pointer| ThemedPointer::load(pointer, None, &compositor, &shm)) {
                Some(Ok(themed)) => Pointer::Themed(themed),
                Some(Err(plain)) => Pointer::Plain(plain),
                None => Pointer::None
            };
            PointerState {
                surfaces: surfaces,
                location: PtrLocation::None,
                coordinates: (0., 0.),
                surface_width: width,
                cornered: false,
                topped: false,
                pointer: pointer
            }
        };

        let mut me = DecoratedSurface {
            shell_surface: shell_surface,
            border_surfaces: border_surfaces,
            buffers: Vec::new(),
            tempfile: tempfile,
            pool: pool,
            height: height,
            width: width,
            buffer_capacity: pxcount * 4,
            pointer_state: pointer_state,
            eventiter: evts,
            seat: seat,
        };

        me.resize(width, height);

        Ok(me)
    }

    /// Set a short title for the surface.
    ///
    /// This string may be used to identify the surface in a task bar, window list, or other user
    /// interface elements provided by the compositor.
    pub fn set_title(&self, title: String) {
        self.shell_surface.set_title(title)
    }

    /// Set a class for the surface.
    ///
    /// The surface class identifies the general class of applications to which the surface
    /// belongs. A common convention is to use the file name (or the full path if it is a
    /// non-standard location) of the application's .desktop file as the class.
    pub fn set_class(&self, class: String) {
        self.shell_surface.set_class(class)
    }
}

impl Iterator for DecoratedSurface {
    type Item = (WlShellSurfaceResize::WlShellSurfaceResize, i32, i32);

    fn next(&mut self) -> Option<(WlShellSurfaceResize::WlShellSurfaceResize, i32, i32)> {
        use wayland_client::wayland::WaylandProtocolEvent;
        use wayland_client::wayland::seat::WlPointerEvent;
        use wayland_client::wayland::shell::WlShellSurfaceEvent;

        for e in &mut self.eventiter {
        match e {
            Event::Wayland(WaylandProtocolEvent::WlPointer(_pid,
                WlPointerEvent::Enter(serial, sid, x, y)
            )) => {
                self.pointer_state.coordinates = (x, y);
                self.pointer_state.pointer_entered(sid, serial);
            },
            Event::Wayland(WaylandProtocolEvent::WlPointer(_pid,
                WlPointerEvent::Leave(_serial, _sid)
            )) => {
                self.pointer_state.pointer_left();
            },
            Event::Wayland(WaylandProtocolEvent::WlPointer(_pid,
                WlPointerEvent::Motion(_time, x, y)
            )) => {
                self.pointer_state.coordinates = (x, y);
                self.pointer_state.update(None, false);
            }
            Event::Wayland(WaylandProtocolEvent::WlPointer(_pid,
                WlPointerEvent::Button(serial, _time, button, state)
            )) => {
                if button != 0x110 { continue; }
                if let WlPointerButtonState::Released = state { continue; }
                let (x, y) = self.pointer_state.coordinates;
                let w = self.pointer_state.surface_width;
                let (direction, resize) = match self.pointer_state.location {
                    PtrLocation::Top => {
                        if y < DECORATION_SIZE as f64 {
                            if x < DECORATION_SIZE as f64 {
                                (WlShellSurfaceResize::TopLeft, true)
                            } else if x > w as f64 + DECORATION_SIZE as f64 {
                                (WlShellSurfaceResize::TopRight, true)
                            } else {
                                (WlShellSurfaceResize::Top, true)
                            }
                        } else {
                            if x < DECORATION_SIZE as f64 {
                                (WlShellSurfaceResize::Left, true)
                            } else if x > w as f64 + DECORATION_SIZE as f64 {
                                (WlShellSurfaceResize::Right, true)
                            } else {
                                (WlShellSurfaceResize::None, false)
                            }
                        }
                    },
                    PtrLocation::Bottom => {
                        if x < DECORATION_SIZE as f64 {
                            (WlShellSurfaceResize::BottomLeft, true)
                        } else if x > w as f64 + DECORATION_SIZE as f64 {
                            (WlShellSurfaceResize::BottomRight, true)
                        } else {
                            (WlShellSurfaceResize::Bottom, true)
                        }
                    },
                    PtrLocation::Left => (WlShellSurfaceResize::Left, true),
                    PtrLocation::Right => (WlShellSurfaceResize::Right, true),
                    PtrLocation::None => (WlShellSurfaceResize::None, true)
                };
                if let Some(ref seat) = self.seat {
                    if resize {
                        self.shell_surface.resize(&seat, serial, direction);
                    } else {
                        self.shell_surface.move_(&seat, serial);
                    }
                }
            },
            Event::Wayland(WaylandProtocolEvent::WlShellSurface(_pid,
                WlShellSurfaceEvent::Ping(serial)
            )) => {
                self.shell_surface.pong(serial);
            },
            Event::Wayland(WaylandProtocolEvent::WlShellSurface(_,
                WlShellSurfaceEvent::Configure(r, x, y)
            )) => {
                // forward configure
                return Some((r, x, y))
            },
            _ => {}
        }}
        // nothing more ?
        None
    }
}

/// Substracts the border dimensions from the given dimensions.
pub fn substract_borders(width: i32, height: i32) -> (i32, i32) {
    (
        width - 2*(DECORATION_SIZE as i32),
        height - DECORATION_SIZE as i32 - DECORATION_TOP_SIZE as i32
    )
}

/// Adds the border dimensions to the given dimensions.
pub fn add_borders(width: i32, height: i32) -> (i32, i32) {
    (
        width + 2*(DECORATION_SIZE as i32),
        height + DECORATION_SIZE as i32 + DECORATION_TOP_SIZE as i32
    )
}

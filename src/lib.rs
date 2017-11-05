//! Wayland Window, a minimalistic decoration-drawing library for
//! wayland applications.
//!
//! This crate is only usable in conjuction of the
//! [`wayland-client`](https://crates.io/crates/wayland-client) crate.
//!
//! ## Creating a window with decorations
//!
//! Creating a decorated frame for your window is simply done using the provided init function:
//!
//! ```ignore
//! use wayland_window::create_frame;
//! // if using the legacy wl_shell global
//! let shell = Shell::Wl(my_wl_shell);
//! // if using the new not-yet-stable xdg_shell
//! let shell = Shell::Xdg(my_xdh_shell);
//! let frame = create_frame(
//!        &mut event_queue, my_implementation, my_implementation_data,
//!        &my_surface, width, height, &compositor, &subcompositor, &shm, &shell, Some(seat)
//! ).unwrap(); // creation can fail
//! ```
//!
//! As you can see, you need to pass several references to global objects as well as a `WlSeat`.
//! It is required for the library to be able to create the surfaces to draw the borders, react
//! to user input in the borders, for resizing and move. It will use the events provided on the
//! seat you passed as argument. (So if you are on a setup with more than one pointer,
//! only the one associated with this seat will be able to resize the window).
//!
//! See next section for example use of the `my_implementation` and
//! `my_implementation_data` arguments.
//!
//! ## Configure events
//!
//! The `Frame` object will not resize your window itself, as it cannot do it.
//!
//! When the user clicks on a border and starts a resize, the server will start to generate a
//! number of `configure` events on the shell surface. You'll need to process the events generated
//! by the surface to handle them.
//!
//! The wayland server can (and will) generate a ton of `configure` events during a single
//! `WlDisplay::dispatch()` if the user is currently resizing the window. You are only required to
//! process the last one, and if you try to handle them all your aplication will be very
//! laggy.
//!
//! The proper way is to accumulate them in your subhandler, overwriting the the previous one
//! each time, and manually checking if one has been received in the main loop of your program.
//! For example like this
//!
//! ```no_run
//! # extern crate wayland_client;
//! # extern crate wayland_window;
//! use wayland_window::{Frame, create_frame, FrameImplementation};
//!
//! // define a state to accumulate sizes
//! struct ConfigureState {
//!     new_size: Option<(i32,i32)>
//! }
//!
//! # fn main() {
//! # let (display, mut event_queue) = wayland_client::default_connect().unwrap();
//! // insert it in your event queue state
//! let configure_token = event_queue.state().insert(ConfigureState { new_size: None });
//!
//! // use it in your implementation:
//! let my_implementation = FrameImplementation {
//!     configure: |evqh, token, _, newsize| {
//!         let configure_state: &mut ConfigureState = evqh.state().get_mut(token);
//!         configure_state.new_size = newsize;
//!     },
//!     close: |_, _| { /* ... */ },
//!     refresh: |_, _| { /* ... */ }
//! };
//!
//! # let (my_surface,width,height,compositor,subcompositor,shm,shell,seat) = unimplemented!();
//! // create the decorated surface:
//! let frame = create_frame(
//!     &mut event_queue,          // the event queue
//!     my_implementation,         // our implementation
//!     configure_token.clone(),   // the implementation data
//!     &my_surface, width, height, &compositor, &subcompositor, &shm, &shell, Some(seat)
//! ).unwrap();
//!
//! // then, while running your event loop
//! loop {
//!     display.flush().unwrap();
//!     event_queue.dispatch().unwrap();
//!
//!     // check if a resize is needed
//!     let mut configure_state = event_queue.state().get_mut(&configure_token);
//!     if let Some((w, h)) = configure_state.new_size.take() {
//!         // The compositor suggests we take a new size of (w, h)
//!         // Handle it as needed (see next section)
//!     }
//! }
//!
//! # }
//! ```
//!
//! ## Resizing the surface
//!
//! When resizing your main surface, you need to tell the `Frame` that it
//! must update its dimensions. This is very simple:
//!
//! ```ignore
//! // update your contents size (here by attaching a new buffer)
//! surface.attach(Some(&new_buffer));
//! surface.commit();
//! // update the borders size
//! frame.resize(width, height);
//! // refresh the frame so that it actually draws the new size
//! frame.refresh();
//! ```
//!
//! If you do this as a response of a `configure` event, note the following points:
//!
//! - You do not have to respect the exact sizes provided by the compositor, it is
//!   just a hint. You can even ignore it if you don't want the window to be resized.
//! - In case you chose to ignore the resize, it can be appropiate to still resize your
//!   window to its current size (update the buffer to the compositor), as the compositer
//!   might have resized your window without telling you.
//! - The size hint provided to your implementation is a size hint for the interior of the
//!   window: the dimensions of the border has been subtracted from the hint the compositor
//!   gave. If you need to compute dimensions taking into account the sizes of the borders,
//!   you can use the `add_borders` and `subtract_borders` functions.

#![warn(missing_docs)]

extern crate memmap;
extern crate tempfile;
extern crate wayland_client;
extern crate wayland_protocols;

mod frame;
mod pointer;
mod theme;
mod themed_pointer;
mod shell;

pub use frame::{Frame, State};
use pointer::{Pointer, PointerState};
pub use shell::{Configure, Shell};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
pub use theme::{add_borders, subtract_borders};
use themed_pointer::ThemedPointer;
use wayland_client::{EventQueueHandle, Proxy};
use wayland_client::protocol::*;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) enum Location {
    None,
    Top,
    TopRight,
    Right,
    BottomRight,
    Bottom,
    BottomLeft,
    Left,
    TopLeft,
    TopBar,
    Inside,
    Button(UIButton),
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) enum UIButton {
    Minimize,
    Maximize,
    Close,
}

pub(crate) struct FrameIData<ID> {
    pub(crate) implementation: FrameImplementation<ID>,
    pub(crate) meta: Arc<Mutex<::frame::FrameMetadata>>,
    pub(crate) idata: Rc<RefCell<ID>>,
}

pub(crate) struct PointerIData<ID> {
    pub(crate) implementation: FrameImplementation<ID>,
    pub(crate) pstate: PointerState,
    pub(crate) idata: Rc<RefCell<ID>>,
}

impl<ID> Clone for FrameIData<ID> {
    fn clone(&self) -> FrameIData<ID> {
        FrameIData {
            implementation: self.implementation.clone(),
            meta: self.meta.clone(),
            idata: self.idata.clone(),
        }
    }
}

/// For handling events that occur to a Frame.
pub struct FrameImplementation<ID> {
    /// Called whenever the Frame has been resized.
    ///
    /// **Note:** if you've not set a minimum size, `width` and `height` will not always be
    /// positive values. Values can be negative if a user attempts to resize the window past
    /// the left or top borders.
    pub configure:
        fn(evqh: &mut EventQueueHandle, idata: &mut ID, cfg: shell::Configure, newsize: Option<(i32, i32)>),
    /// Called when the Frame is closed.
    pub close: fn(evqh: &mut EventQueueHandle, idata: &mut ID),
    /// Called when the Frame wants to be refreshed
    pub refresh: fn(evqh: &mut EventQueueHandle, idata: &mut ID),
}

impl<ID> Copy for FrameImplementation<ID> {}
impl<ID> Clone for FrameImplementation<ID> {
    fn clone(&self) -> FrameImplementation<ID> {
        *self
    }
}

/// Create a decoration frame for a wl_surface
///
/// This will create a decoration and declare it as a shell surface to
/// the wayland compositor.
///
/// See crate documentations for details about how to use it.
pub fn create_frame<ID: 'static>(evqh: &mut EventQueueHandle, implementation: FrameImplementation<ID>,
                                 idata: ID, surface: &wl_surface::WlSurface, width: i32, height: i32,
                                 compositor: &wl_compositor::WlCompositor,
                                 subcompositor: &wl_subcompositor::WlSubcompositor, shm: &wl_shm::WlShm,
                                 shell: &Shell, seat: Option<wl_seat::WlSeat>)
                                 -> Result<Frame, ()> {
    // create the frame
    let mut frame = Frame::new(
        surface,
        width,
        height,
        compositor,
        subcompositor,
        shm,
        shell,
    )?;

    let frame_idata = FrameIData {
        implementation: implementation,
        meta: frame.meta.clone(),
        idata: Rc::new(RefCell::new(idata)),
    };

    // create the pointer
    if let Some(seat) = seat {
        let pointer = seat.get_pointer().expect("Received a defunct seat.");
        frame.pointer = pointer.clone();
        let pointer = ThemedPointer::load(pointer, None, &compositor, &shm)
            .map(Pointer::Themed)
            .unwrap_or_else(Pointer::Plain);
        let pstate = PointerState::new(
            frame.meta.clone(),
            pointer,
            frame.surface.clone().unwrap(),
            frame.shell_surface.clone().unwrap(),
            seat,
        );
        let pointer_idata = PointerIData {
            implementation: implementation,
            pstate: pstate,
            idata: frame_idata.idata.clone(),
        };
        evqh.register(
            frame.pointer.as_ref().unwrap(),
            ::pointer::pointer_implementation(),
            pointer_idata,
        );
    }

    frame.shell_surface.register_to(evqh, frame_idata);

    Ok(frame)
}

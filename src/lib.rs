//! Wayland Window, a minimalistic decoration-drawing library for
//! wayland applications.
//!
//! This crate is only usable in conjuction of the
//! [`wayland-client`](https://crates.io/crates/wayland-client) crate.
//!
//! ## Creating a decorated shell surface
//!
//! Creating a decorated window is as simple as wrapping it in a
//! `DecoratedSurface`:
//!
//! ```ignore
//! use wayland_window::{DecoratedSurface, Shell};
//! // if using the legacy wl_shell global
//! let shell = Shell::Wl(my_wl_shell);
//! // if using the new not-yet-stable xdg_shell
//! let shell = Shell::Xdg(my_xdh_shell);
//! let decorated = DecoratedSurface::new(&my_surface, width, height, &compositor, &subcompositor, &shm, &shell, Some(seat));
//! ```
//!
//! As you can see, you need to pass several references to global objects as well as a `WlSeat`.
//! It is required for the library to be able to create the surfaces to draw the borders, react
//! to user input in the borders, for resizing and move. It will use the events provided on the
//! seat you passed as argument. (So if you are on a setup with more than one pointer,
//! only the one associated with this seat will be able to resize the window).
//!
//! ## Processing the events
//!
//! In order to process the events, you need to provide a sub-handler to this DecoratedSurface, which itself
//! must be inserted in your event loop.
//!
//! This sub-handler needs to implement the `Handler` trait provided by this crate and will receive the events
//! that cannot be automatically handled for you. See the documentation of this trait for the detail of these
//! events.
//!
//! ```ignore
//! // setup the subhandler
//! *(decorated_surface.handler()) = Some(My_sub_handler);
//! // insert it in the event queue
//! let decorated_surface_id = event_queue.add_handler_with_init(decorated_surface);
//! ```
//!
//! ## Configure events
//!
//! The `DecoratedSurface` object will not resize your window itself, as it cannot do it.
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
//! The proper way is to accumulate them in your subhandler, overwriting the the previous one each time,
//! and manually checking if one has been received in the main loop of your program. For example like this
//!
//! ```ignore
//! // define the sub-handler to accumulate sizes
//! struct MySubHandler {
//!     new_size: Option<(i32,i32)>
//! }
//!
//! impl wayland_window::Handler for Window {
//!     fn configure(&mut self, _: &mut EventQueueHandle, _conf: wayland_window::Configure, width: i32, height: i32) {
//!         self.newsize = Some((width, height))
//!     }
//!     // ...
//! }
//!
//! // then, while running your event loop
//! loop {
//!     display.flush().unwrap();
//!     event_queue.dispatch().unwrap();
//!
//!     // check if a resize is needed
//!     let mut state = event_queue.state();
//!     let mut decorated_surface = state.get_mut_handler::<DecoratedSurface<MySubHandler>>(decorated_surface_id);
//!     if let Some((w, h)) = decorated_surface.handler().as_mut().unwrap().newsize.take() {
//!         // The compositor suggests we take a new size of (w, h)
//!         // Handle it as needed (see next section)
//!     }
//! }
//! ```
//!
//! ## Resizing the surface
//!
//! When resizing your main surface, you need to tell the `DecoratedSurface` that it
//! must update its dimensions. This is very simple:
//!
//! ```ignore
//! // update the borders size
//! decorated_surface.resize(width, height);
//! // update your contents size (here by attaching a new buffer)
//! surface.attach(Some(&new_buffer));
//! surface.commit();
//! ```
//!
//! If you do this as a response of a `configure` event, note the following points:
//!
//! - You do not have to respect the exact sizes provided by the compositor, it is
//!   just a hint. You can even ignore it if you don't want the window to be resized.
//! - In case you chose to ignore the resize, it can be appropiate to still resize your
//!   window to its current size (update the buffer to the compositor), as the compositer
//!   might have resized your window without telling you.
//! - The size hint provided to your sub-handler is a size hint for the interior of the
//!   window: the dimensions of the border has been subtracted from the hint the compositor
//!   gave. If you need to compute dimensions taking into account the sizes of the borders,
//!   you can use the `add_borders` and `subtract_borders` functions.

extern crate byteorder;
extern crate tempfile;
#[macro_use]
extern crate wayland_client;
extern crate wayland_protocols;

mod decorated_surface;
mod themed_pointer;
mod shell;

pub use decorated_surface::{DecoratedSurface, subtract_borders, add_borders, Handler};
pub use shell::{Configure, Shell};

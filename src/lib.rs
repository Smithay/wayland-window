//! Wayland Window, a minimalistic decoration-drawing library for
//! wayland applications.
//!
//! This crate is only usable in conjuction of the
//! [`wayland-client`](https://crates.io/crates/wayland-client) crate.
//!
//! ## Creating a decorated shell surface
//!
//! Creating a decorated window is simply done using the provided init function:
//!
//! ```ignore
//! use wayland_window::{init_decorated_surface};
//! // if using the legacy wl_shell global
//! let shell = Shell::Wl(my_wl_shell);
//! // if using the new not-yet-stable xdg_shell
//! let shell = Shell::Xdg(my_xdh_shell);
//! let decorated_surface = init_decorated_surface(
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
//! The proper way is to accumulate them in your subhandler, overwriting the the previous one
//! each time, and manually checking if one has been received in the main loop of your program.
//! For example like this
//!
//! ```no_run
//! # extern crate wayland_client;
//! # extern crate wayland_window;
//! use wayland_window::{DecoratedSurface, init_decorated_surface,
//!                      DecoratedSurfaceImplementation};
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
//! let my_implementation = DecoratedSurfaceImplementation {
//!     configure: |evqh, token, _, newsize| {
//!         let configure_state: &mut ConfigureState = evqh.state().get_mut(token);
//!         configure_state.new_size = newsize;
//!     },
//!     close: |_, _| { /* ... */ }
//! };
//!
//! # let (my_surface,width,height,compositor,subcompositor,shm,shell,seat) = unimplemented!();
//! // create the decorated surface:
//! let decorated_surface = init_decorated_surface(
//!     &mut event_queue,          // the event queue
//!     my_implementation,         // our implementation
//!     configure_token.clone(),   // the implementation data
//!     &my_surface, width, height, &compositor, &subcompositor, &shm, &shell, Some(seat), true
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
//! - The size hint provided to your implementation is a size hint for the interior of the
//!   window: the dimensions of the border has been subtracted from the hint the compositor
//!   gave. If you need to compute dimensions taking into account the sizes of the borders,
//!   you can use the `add_borders` and `subtract_borders` functions.

extern crate byteorder;
extern crate tempfile;
extern crate wayland_client;
extern crate wayland_protocols;

mod decorated_surface;
mod themed_pointer;
mod shell;

pub use decorated_surface::{add_borders, init_decorated_surface, subtract_borders, DecoratedSurface,
                            DecoratedSurfaceImplementation};
pub use shell::{Configure, Shell};

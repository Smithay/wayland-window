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
//! use wayland_window::DecoratedSurface;
//! let decorated = DecoratedSurface::new(my_surface, width, height, &compositor, &subcompositor, &shm, &shell, Some(seat));
//! ```
//!
//! As you can see, you need to pass several references to global objects as well as a `WlSeat`.
//! It is required for the library to be able to create the surfaces to draw the borders, react
//! to user input in the borders, for resizeing and move. It will use the events provided on the
//! seat you passed as argument. (So if you are on a setup with more than one pointer,
//! only the one associated with this seat will be able to resize the window).
//!
//! ## Processing the events
//!
//! The `DecoratedSurface` object will not resize your window itself, as it cannot do it.
//!
//! When the user clicks on a border and starts a resize, the server will start to generate a
//! number of `configure` events on the shell surface. You'll need to process the events generated
//! by the surface to handle them, as the surface is also an event iterator :
//!
//! ```ignore
//! for (time, x, y) in &mut decorated_surface {
//!     // handle the event
//! }
//! ```
//!
//! The wayland server can (and will) generate a ton of `configure` events during a single
//! `WlDisplay::dispatch()` if the user is currently resizing the window. You are only required to
//! process the last one, and if you try to handle them all your aplication will be very
//! laggy.
//!
//! The proper way is to prcess the iterator and only store them in a container, overwriting the
//! the previous one each time, and manually checking if one has been received in the main loop
//! of your program, like this:
//!
//! ```ignore
//! let mut newsize = None;
//! for (_, x, y) in &mut decorated_surface {
//!     newsize = Some((x, y))
//! }
//! if let Some((x, y)) = newsize {
//!     let (x, y) = substract_borders(x, y);
//!     window.resize(x, y);
//! }
//! ```
//!
//! ## Resizing the surface
//!
//! When resizing your main surface, you need to tell the `DecoratedSurface` that it
//! must update its dimensions. This is very simple:
//!
//! ```ignore
//! /* update the borders size */
//! surface.attach(Some(&new_buffer));
//! decorated_surface.resize(width, height);
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
//! - The size hint provided by the compositor counts the borders size, to get the real
//!   size hint for your interior surface, use the function `substract_borders(..)` provided
//!   by this library.

extern crate byteorder;
extern crate tempfile;
extern crate wayland_client;

mod decorated_surface;

pub use decorated_surface::{DecoratedSurface, substract_borders, add_borders};

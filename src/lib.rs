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
//! let decorated = DecoratedSurface::new(my_surface, width, height, &registry, &seat);
//! ```
//!
//! As you can see, you need to pass the `Registry` and a `Seat` as well. It is required
//! for the library to be able to create the surfaces to draw the borders, and register
//! the callback to detect user input in the borders, for resizeing and move. These callback
//! will be registered on the seat you passed as argument. (So if you are on a setup with more
//! than one pointer, only one of them will be able to resize the window).
//!
//! ## Processing the events
//!
//! The `DecoratedSurface` object will not resize your window itself, as it cannot do it.
//!
//! When the user clicks on a border and starts a resize, the server will start to generate a
//! number of `configure` events on the shell surface. You'll need to register a callback on
//! it to handle them:
//!
//! ```ignore
//! decorated.get_shell().set_configure_callback(move |edge, width, height| {
//!     /* ... */
//! });
//! ```
//!
//! The wayland server can (and will) generate a ton of `configure` events during a single
//! `Display::dispatch()` if the user is currently resizing the window. You are only required to
//! process the last one, and if you try to handle them all your aplication will be very
//! laggy.
//!
//! The proper way is to use the callback to only store them in a container, overwriting the
//! the previous one each time, and manually checking if one has been received in the main loop
//! of your program, like this:
//!
//! ```ignore
//! // create a shared storage: (width, heigh, need_resize ?)
//! let need_resize = Arc::new(Mutex::new((0, 0, false)));
//! // clone it and put it in the callback
//! let my_need_resize = need_resize.clone();
//! decorated.w.get_shell().set_configure_callback(move |_edge, width, height| {
//!     let mut guard = my_newsize.lock().unwrap();
//!     // we overwrite it each time, to only keep the last one
//!     *guard = (width, height, true);
//! });
//! // then handle all this in the main loop:
//! loop {
//!     display.dispatch();
//!     let guard = need_resize.lock().unwrap();
//!     let (width, height, resize) = *guard;
//!     if resize {
//!         /* handle the resizing here */
//!     }
//!     // reset the storage
//!     *guard = (0, 0, false);
//! }
//! ```
//!
//! ## Resizing the surface
//!
//! When resizing your main surface, you need to tell the `DecoratedSurface` that it
//! must update its dimensions. This is very simple:
//!
//! ```ignore
//! /* update the buffer of decorated */
//! decorated.resize(width, height);
//! decorated.get_shell().commit();
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

pub use decorated_surface::{DecoratedSurface, substract_borders};
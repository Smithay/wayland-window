extern crate byteorder;
extern crate tempfile;
extern crate wayland_client as wayland;

mod decorated_surface;

pub use decorated_surface::{DecoratedSurface, substract_borders};
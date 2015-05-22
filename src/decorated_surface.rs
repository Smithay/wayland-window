use std::ops::Deref;

use wayland::core::{SubSurface, Surface, WSurface, Registry};

// The surfaces handling the borders, 8 total, are organised this way:
//
//  0 |   1   | 2
// ---|-------|---
//    |       |
//  7 | user  | 3
//    |       |
// ---|-------|---
//  6 |   5   | 4
//
pub const BORDER_TOP_LEFT    : usize = 0;
pub const BORDER_TOP         : usize = 1;
pub const BORDER_TOP_RIGHT   : usize = 2;
pub const BORDER_RIGHT       : usize = 3;
pub const BORDER_BOTTOM_RIGHT: usize = 4;
pub const BORDER_BOTTOM      : usize = 5;
pub const BORDER_BOTTOM_LEFT : usize = 6;
pub const BORDER_LEFT        : usize = 7;

/// A decorated surface, wrapping a wayalnd surface and handling its decorations.
pub struct DecoratedSurface<S: Surface> {
    main_surface: WSurface,
    user_surface: SubSurface<S>,
    border_surfaces: Vec<SubSurface<WSurface>>
}

impl<S: Surface> DecoratedSurface<S> {
    /// Creates a new decorated window around given surface.
    ///
    /// If the creation failed (likely if the registry was not ready), hands back the surface.
    pub fn new(user_surface: S, registry: &Registry) -> Result<DecoratedSurface<S>,S> {
        // fetch the global 
        let comp = match registry.get_compositor() {
            Some(c) => c,
            None => return Err(user_surface)
        };
        let subcomp = match registry.get_subcompositor() {
            Some(c) => c,
            None => return Err(user_surface)
        };
        let main_surface = comp.create_surface();
        let user_subsurface = subcomp.get_subsurface(user_surface, &main_surface);
        let border_surfaces = (0..8).map(|_|
            subcomp.get_subsurface(comp.create_surface(), &main_surface)
        ).collect();
        Ok(DecoratedSurface {
            main_surface: main_surface,
            user_surface: user_subsurface,
            border_surfaces: border_surfaces,
        })
    }

    /// Destroys the decorated window and gives back the wrapped surface.
    pub fn unwrap(self) -> S {
        self.user_surface.destroy()
    }
}

impl<S: Surface> Deref for DecoratedSurface<S> {
    type Target = S;
    fn deref(&self) -> &S {
        &*self.user_surface
    }
}
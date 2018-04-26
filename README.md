**Deprecation**: this library is deprecated in favor of
[Smithay's Client Toolkit](https://github.com/Smithay/client-toolkit), and will not be
ported to wayland-client 0.20.

# wayland-window
A simple window-decorations library built on top of wayland-client.

It draws simplistic decorations around a given wayland surface, and registers
callbacks to allow resizing and moving the surface useing the borders.

It is currently more aiming at usability than design quality, so the drawn
borders are plain grey and not customizable. I'll possible improve in future
releases.

## Usage
Instructions of use are on the main page of the documentation,
readable [on docs.rs](http://docs.rs/wayland-window/).

Docs for the master branch are also available online:
https://smithay.github.io/wayland-window

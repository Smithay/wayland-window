# Change Log

## Unreleased

## 0.13.2 -- 2017-02-05

- Fix buttons colors
- Drop image dependency

## 0.13.1 -- 2017-02-03

- Bugfix: decorations were drawn even when disabled or fullscreen

## 0.13.0 -- 2017-02-03

- Rework of the whole decoration implementation
- Add buttons for minimize/maximize/close

## 0.12.0 -- 2017-10-29

- Update to wayland-rs 0.12

## 0.11.0 -- 2017-10-16

- Rework API to not store a state value

## 0.10.0 -- 2017-10-09

- Update to wayland-client-0.11

## 0.9.0 -- 2017-09-21

- Update to wayland-client-0.10 new API

## 0.8.0 -- 2017-08-07

- Improve `xdg_shell` support for configure events (breaking change of `Handler` API)
- Add `set_min_size` and `set_max_size` methods

## 0.7.0 -- 2017-07-05

Add `xdg_shell` support (thanks to @mitchmindtree)

## 0.6.1 -- 2017-01-02

Migrate repo to smithay org.

## 0.6.0 -- 2017-05-29

0.5.1 should have been 0.6.0

## 0.5.1 -- 2017-03-19 [yanked]

wayland-window is compatible with wayland-client-0.9.x

## 0.5.0 -- 2017-03-02

Upgrate wayland-client dependency

## 0.4.4 -- 2017-02-02

Upgrade byteorder dependency

## 0.4.3 -- 2016-12-24

- Bugfix suface damaging on wl-surfaces of version <= 3 (kudos to @emilio for finding the bug,
  details on https://github.com/vberger/wayland-client-rs/issues/75 )

## 0.4.2 -- 2016-10-08

- Better handling of cursor theming
- Ability to diable decorations & go fullscreen

## 0.4.1 -- 2016-10-08

DecoratedSurface is now Send and require a Send handler.

## 0.4.0 -- 2016-10-03

Update to wayland-client-0.7

## 0.3.0 -- 2016-05-29

Update to wayland-client-0.6

## 0.2.3 -- 2016-04-11

Update dependencies.

## 0.2.2 -- 2015-12-13

### Added

- `set_title` for decorated surfaces
- `set_class` for decorated surfaces
- `add_borders` free helper function

## 0.2.1 -- 2015-12-09

Update to wayland-client 0.5.

## 0.2.0 -- 2015-11-30

Update the lib to new wayland-client API.

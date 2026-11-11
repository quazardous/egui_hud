# egui_hud

[<img alt="github" src="https://img.shields.io/badge/github-quazardous/egui__hud-8da0cb?logo=github" height="20">](https://github.com/quazardous/egui_hud)
[![CI](https://github.com/quazardous/egui_hud/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/quazardous/egui_hud/actions/workflows/ci.yml)
[![Demo](https://github.com/quazardous/egui_hud/actions/workflows/pages.yml/badge.svg?branch=main)](https://quazardous.github.io/egui_hud/)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

**[Live demo](https://quazardous.github.io/egui_hud/)**: a mixer channel
and a light, declared as data, in a box laid out as it is resized, beside
previews of what each control does.

Controls as data for [egui](https://github.com/emilk/egui) applications:
what a control is (purpose, value, units, ranges, feel, detents,
read-out), the faces that draw it and the representations they take on
each surface (panel row, compact field, HUD chip), cards laid out on a slot engine
([egui_fit](https://github.com/quazardous/egui_fit)) and
resolved in context, and a value flow of gestures (begin, preview,
commit as one undo step through a vetoable change step, or cancel with
no trace).

The crate knows nothing of the application it serves. It asks for:

- a store, through the `ValueSource` and `ValueStore` traits (read
  values and parameter descriptors; begin, preview, commit and cancel
  gestures). `MemoryStore` is a small one in memory, for tests and
  benches;
- parameter descriptors (`ParamDescriptor`), from which a parameter no
  control is patched to gets an implicit control;
- a command registry of its own: the `controls.*` commands are plain
  functions (`commands::COMMANDS`, `commands::run`) the host declares.

The renderers on [egui-pop](https://github.com/quazardous/egui-pop) are
behind the `render` feature; without it the crate has no egui
dependency. The `bench` feature adds the bench: a shelf of a host's
controls, composed onto a card and tried live (context bar, modes,
presets, undo and redo, and why each control takes its form).

## Controls and faces

A **control** is a value (its type, range, unit, default) and a
behaviour (its connector: jacks, gestures, intents, signals). It says
nothing about drawing. A **face** is a way to draw a value type, with
its slot sizes per representation; `egui_hud::face` holds the library
every control shares:

| value | faces |
|---|---|
| bool | checkbox, switch, push button, lamp (read-only) |
| number | slider, fader, knob, endless knob, scrub field, meter (read-only), read-out (read-only) |
| one-of | menu, segmented buttons, radio list |
| points, colour, text | curve and point list, colour wheel, text field |

A face fits values, not only a type: the slider, fader and knob need a
bounded number; the endless knob needs one that `wraps` (an angle, a
hue) or is `open`, and moves it by relative nudges. A control may name
the faces it accepts, in order (a gain: `["fader", "knob"]`); otherwise
it accepts every face that fits its value. A kit names faces by id, or
by an object for what one control changes
(`{"face": "fader", "integration": ...}`). Which face shows is
presentation: the first accepted by default, another when the context's
slot is too small for it, or the one the person picks on the bench.

The switch and the endless knob are drawn for now by the nearest
existing renderer (the checkbox, the knob); the model already names
them.

## Generic controls first

A checkbox stays a checkbox whatever its domain. There is one **generic
control** per value type (a number, a whole number, a toggle, a choice,
a curve, a colour, a text), and a domain use of one is a **configured
control**: the generic control plus its settings (`bench::Configured`,
`bench::Settings`):

- a name (its label) and one line saying, in plain words, what it does
  (shown on the shelf and when the pointer rests on the control);
- per value type: a range, step, unit and scale for a number, choices
  for a choice, a default for each.

A kit's `configured` controls are presets of the generic controls,
listed on the shelf under their domain group; in a kit they read flat:

```json
{"name": "low-cut", "group": "Mixer channel", "generic": "toggle",
 "label": "Low cut", "line": "Removes the lowest frequencies, rumble and hum"}
```

On the bench, a generic control dropped from the shelf (under
"Miscellaneous") opens its set-up: type its name and its line, give a
number its range and unit, a choice its choices. **Keep on the shelf**
keeps it as a configured control, saved with the bench and loaded back.
Agents do the same with `controls.bench.add {control, settings}`,
`controls.bench.configure {key, settings}` and
`controls.bench.keep {key, name, group}`.

**The exception**: register a control of its own only when it adds
something beyond its value type and settings: a feel and detents (a
gain in decibels with a detent at 0 dB), a face of its own or a choice
of faces (a fader and a knob), a value that wraps. A module can likewise
register a face overriding how a value type is drawn (a host's
`Drawings`). Everything else is a configured generic control.

## Demo

**[Try it in a browser](https://quazardous.github.io/egui_hud/)**:
a mixer channel and a light, declared as data, in a box you resize,
with a channel strip and a lamp that show what each control does
([source](demo)). The demo (`demo/`, not published) runs in a browser with
[Trunk](https://trunkrs.dev):

```sh
rustup target add wasm32-unknown-unknown
cargo install --locked trunk
cd demo
trunk serve    # then open http://127.0.0.1:8080
```

or on the desktop with `cargo run -p egui_hud_demo`.

## Features

- `render`: the renderers (panel rows, HUD chips) on egui-pop. Without it
  the crate is the model, the registry and the value flow, with no egui.
- `bench`: the bench (implies `render`).

## Status

Version 0, not published. The API may change. egui_hud was developed
inside [Helios](https://github.com/quazardous/helios), a photo editor that
uses it for its module cards, and moved to its own repository once its
model settled.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in the work by you, as defined in the Apache-2.0
license, shall be dual licensed as above, without any additional terms or
conditions.

# egui_hud demo

A web demo of [egui_hud](..), the controls framework, laid out by
[egui_fit](https://github.com/quazardous/egui_fit) and drawn
with [egui-pop](https://github.com/quazardous/egui-pop).

**Live:** <https://quazardous.github.io/egui_hud/>

## What it shows

One page, as the [egui_fit demo](https://github.com/quazardous/egui_fit)
lays it out, with real egui_hud controls in the box:

- **the box**, in the middle: each control drawn by egui_hud with its
  face, its value live. Drag the box's right edge (or the Width slider)
  and every control takes the face, form (full, compact, simplified) and
  orientation that fit its slot, with hysteresis: the gain is a fader two
  columns wide, a knob under one column, a scrub field under three
  quarters of one. Rest the pointer on a control to read what it does;
  right-click one with several faces to pick another. What the detail
  level folds goes under "More (n)";
- **on the left**: the presets, the controls to add (the kit's groups),
  and the list of controls in the box, in logical order: move them up
  and down, remove them, tick some and group them into a case under a
  legend, take a case apart;
- **the toolbar**: the width, the density (which sets the column), the
  slot columns drawn behind the box, and Wobble, which swings the width
  around its value to show the layout holding;
- **on the right, two previews**, so every control visibly does
  something:
  - **a channel strip**, driven by the mixer controls: a simulated
    signal as a small spectrum (the low cut removes the bass at its left
    end, the EQ curve reshapes it live), left and right meters (gain and
    mute drive the level, pan splits it), the channel's name on its
    colour tag;
  - **a lamp**, driven by the light controls: power, intensity
    (brightness), temperature (warm to cool white), the colour wheel and
    hue (a tint), the effect (steady, fade, pulse, strobe).

  A control not in the box stays at its default.

The controls are a neutral catalogue declared as data in
[`kit.json`](kit.json), with no application behind it: a mixer channel
(gain fader, pan, mute, level meter following a simulated signal, colour
tag, name, an EQ curve and a low cut) and a light (power, intensity,
temperature, colour wheel, hue, effect). The page's model is egui_hud's
`Bench` (feature `bench`): the composition, the values through the value
flow, and the resolution in the box.

## Running it

In a browser, with [Trunk](https://trunkrs.dev):

```bash
cd demo
trunk serve        # or: trunk build --release
```

Natively: `cargo run -p egui_hud_demo`.

The tests run headless, the page included (with egui_kittest):
`cargo test -p egui_hud_demo`.

## The catalogue

`kit.json` holds:

- `controls`: egui_hud controls (`egui_hud::model::Control`) that add
  something beyond their value type (the gain in decibels, the level
  meter, pan, the light's intensity, temperature and hue), each with its
  value and connector, feel, detents and read-out, and the faces it
  accepts: references to egui_hud's face library (`"knob"`), or an
  object naming one (`{"face": "fader", "integration": ...}`) for what
  the control changes, here who draws it on which surface and its
  fallback. A control naming no faces (the hue, a number that wraps)
  accepts every face that fits its value: the endless knob and the
  scrub field;
- `groups`: each control's group, as the Add list shows it;
- `configured`: configured controls, presets of the generic controls,
  each a generic control (`toggle`, `choice`, `curve`, `colour`,
  `text`…) with its settings: its name (`label`), its `line` in plain
  words, and its range, unit, choices or default (mute, power, the low
  cut: toggles; the colour tag and the effect: choices; the EQ curve,
  the colour wheel, the channel's name);
- `implicit`: whether the bench offers the generic controls, one per
  value type, under "Miscellaneous" (the page leaves them out: they are
  named and set up through the library);
- `presets`: the boxes as data (composition, rules, context, mode,
  width);
- `tools`: the tools a context may name.

## License

MIT OR Apache-2.0, as egui_hud.

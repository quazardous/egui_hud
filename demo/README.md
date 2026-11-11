# egui_hud demo

A web demo of [egui_hud](..), the controls framework, laid out by
[egui_fit](https://github.com/quazardous/egui_fit) and drawn
with [egui-pop](https://github.com/quazardous/egui-pop).

**Live:** <https://quazardous.github.io/egui_hud/>

## What it shows

A neutral catalogue declared as data in [`kit.json`](kit.json), with no
application behind it:

- **a mixer channel**: a gain fader, a pan knob, a mute toggle, a level
  meter (an indicator, following a simulated signal), a colour tag, and an
  equaliser case under its legend: an EQ curve drawn by egui-pop's curve
  editor where it has room, as rows of points where it has not, and a low
  cut toggle;
- **a light**: power, intensity, colour temperature, a colour wheel (its
  swatch and channels where the wheel does not fit) and an effect.

Above the bench, two previews show what every control does:

- **a channel strip**, driven by the mixer controls: a simulated signal
  as a small spectrum (the low cut removes the bass at its left end, the
  EQ curve reshapes it live), left and right meters (gain and mute drive
  the level, pan splits it), the channel's name on its colour tag;
- **a lamp**, driven by the light controls: power, intensity
  (brightness), temperature (warm to cool white), the colour wheel and
  hue (a tint), the effect (steady, fade, pulse, strobe).

A control not on the bench stays at its default. Each control says what
it does, in one line, when the pointer rests on it and on the shelf.

They sit on egui_hud's **bench** (`egui_hud::bench`, feature `bench`):

- the shelf, searchable; drag a control onto the bench, or click it;
- a generic control (under Miscellaneous) dropped on the bench opens its
  set-up: its name, its line, its range and unit or its choices; **Keep
  on the shelf** keeps it as a configured control, saved with the bench;
- **Arrange**: move, select, group into a case under a legend, remove;
- the handle on the bench's right edge sets its width; every control takes
  the form that fits (full, compact, simplified), its orientation and its
  slot, with hysteresis;
- the values live in a `MemoryStore`, through the value flow: each gesture
  is one undo step, with undo and redo;
- the context bar: task, detail, image flags, tool, input device,
  density;
- the modes: panel, dock, toolbar, HUD (chips), float;
- the Placements pane says, per control, its slot, form and orientation,
  and why: the detail level against its tier, a rule of the card, the
  slot engine, a fallback;
- presets that show the resizing: the gain is a fader two columns wide, a
  knob under one column, a scrub field under three quarters of one; at the
  essential detail the other parts fold under "More (n)";
- the bench saved and loaded as text (and downloaded as a file in a
  browser).

## Running it

In a browser, with [Trunk](https://trunkrs.dev):

```bash
cd demo
trunk serve        # or: trunk build --release
```

Natively: `cargo run -p egui_hud_demo`.

The model's tests run headless: `cargo test -p egui_hud_demo`.

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
- `groups`: each control's shelf group;
- `configured`: configured controls, presets of the generic controls,
  each a generic control (`toggle`, `choice`, `curve`, `colour`,
  `text`…) with its settings: its name (`label`), its `line` in plain
  words, and its range, unit, choices or default (mute, power, the low
  cut: toggles; the colour tag and the effect: choices; the EQ curve,
  the colour wheel, the channel's name);
- `implicit`: whether the shelf lists the generic controls, one per
  value type, under "Miscellaneous";
- `presets`: benches as data (composition, rules, context, mode, width);
- `tools`: the tools the context bar offers.

## License

MIT OR Apache-2.0, as egui_hud.

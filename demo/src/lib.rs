//! The `egui_hud` demo: a box you resize, filled with real `egui_hud`
//! controls (a mixer channel and a light, declared as data in [`KIT`]),
//! laid out by its slot engine (`egui_fit`), beside two previews that show
//! what every control does.
//!
//! - [`kit`] reads the catalogue: the controls that specialise their value
//!   type (the gain in decibels, the meter, pan, the light's intensity,
//!   temperature and hue), the configured controls (presets of the
//!   generic controls: mute and power toggles, the colour tag and effect
//!   choices, an EQ curve, a colour wheel, each with its name and its
//!   line) and the presets.
//! - [`DemoDrawings`] draws the integrated controls: the gain's fader, the
//!   level meter, and (through `egui_hud`'s editor drawings) the curve and
//!   the wheel. Where a drawing cannot go, `egui_hud` draws the control's
//!   declared fallback: the gain's knob, the curve's rows of points, the
//!   wheel's swatch and channels. It also offers each control's faces in a
//!   menu on its box.
//! - [`Demo`] is the page: presets and the list of controls on the left,
//!   the box in the middle, the previews on the right. Its model is
//!   `egui_hud`'s [`Bench`]: the composition, the values with their
//!   gestures, and the resolution in the box.
//! - [`preview`] shows what every control does: a channel strip driven by
//!   the mixer controls, a lamp driven by the light controls.

mod page;
pub mod preview;

use std::collections::{BTreeMap, BTreeSet};

use egui_hud::bench::{Bench, Configured, EditorDrawings, Kit, Preset};
use egui_hud::model::{Control, Representation};
use egui_hud::render::{self, Drawings, Drawn, Dressing, Edit, Look};
use egui_hud::slot::{Integration, Orientation};
use egui_hud::{Registry, Value};
use egui_pop::egui::{self, Color32, Rect, Role, Sense, Stroke, StrokeKind, Ui, WidgetInfo};
use serde::Deserialize;

/// The window's and the page's title.
pub const TITLE: &str = "egui_hud demo";

/// The catalogue, as data.
pub const KIT: &str = include_str!("../kit.json");

/// The gain control's id: drawn as a fader where it has room.
pub const GAIN: &str = "gain";
/// The mute toggle's control id: the generic toggle, configured by the
/// kit.
pub const MUTE: &str = "implicit:shelf/mute";
/// The level meter's id: an indicator, drawn as a bar.
pub const METER: &str = "meter";

/// The source code, linked from the title.
pub const SOURCE: &str = "https://github.com/quazardous/egui_hud/tree/main/demo";

/// The catalogue file's shape.
#[derive(Deserialize)]
struct KitData {
    controls: Vec<Control>,
    groups: BTreeMap<String, String>,
    #[serde(default)]
    configured: Vec<Configured>,
    #[serde(default)]
    implicit: bool,
    #[serde(default)]
    presets: Vec<Preset>,
    #[serde(default)]
    tools: Vec<String>,
}

/// The kit of a catalogue written as [`KIT`] is: every control checked
/// as it is registered.
pub fn kit_of(text: &str) -> Result<Kit, String> {
    let data: KitData = serde_json::from_str(text).map_err(|e| format!("the catalogue: {e}"))?;
    let kit = Kit {
        registry: Registry::with(data.controls)?,
        groups: data.groups,
        configured: data.configured,
        implicit: data.implicit,
        presets: data.presets,
        tools: data.tools,
    };
    kit.check()?;
    Ok(kit)
}

/// The demo's kit, from [`KIT`].
pub fn kit() -> Result<Kit, String> {
    kit_of(KIT)
}

/// The demo's bench, its first preset loaded.
pub fn bench() -> Result<Bench, String> {
    let mut b = Bench::new(kit()?);
    b.load_sample();
    Ok(b)
}

/// The meter's lowest reading, in decibels: silence.
pub const SILENCE: f64 = -60.0;
/// The meter's highest reading, in decibels.
pub const PEAK: f64 = 6.0;

/// The simulated signal's level at time `t` (seconds), in decibels:
/// the gain's level moving like music, or silence when muted.
pub fn signal(gain: f64, muted: bool, t: f64) -> f64 {
    if muted {
        return SILENCE;
    }
    let beat = (t * 4.1).sin().abs() * (t * 0.73).sin().abs();
    (gain - 18.0 + 16.0 * beat).clamp(SILENCE, PEAK)
}

/// The demo's drawings: the gain's fader and the level meter, then
/// `egui_hud`'s curve editor and colour wheel; and a menu of faces on
/// each control that has more than one.
#[derive(Default)]
pub struct DemoDrawings {
    /// The meter's reading, in decibels, set each frame by the page.
    pub level: f64,
    /// The face each control shows now, by key, set before drawing.
    pub faces: BTreeMap<String, String>,
    /// A face picked in a control's menu this frame: its key and face.
    pub picked: Option<(String, String)>,
}

impl DemoDrawings {
    /// The gain as a fader (vertical when its slot is taller than wide).
    fn fader(
        ui: &mut Ui,
        key: &str,
        control: &Control,
        rect: Rect,
        value: &mut Value,
    ) -> Vec<Edit> {
        let mut v = value.as_f64().unwrap_or(0.0);
        let look = Look {
            representation: Representation::Full,
            orientation: if rect.height() > rect.width() {
                Orientation::Vertical
            } else {
                Orientation::Horizontal
            },
            variant: Some("fader".into()),
            integration: Integration::Managed,
        };
        let out = render::number_at(ui, control, &look, rect, &mut v);
        *value = Value::Float(v);
        out.changes
            .into_iter()
            .map(|change| Edit::Number {
                key: key.to_owned(),
                change,
            })
            .collect()
    }

    /// The meter: a bar from silence to the peak, green, then yellow over
    /// -12 dB, red over 0 dB; its line and reading on hover.
    fn meter(ui: &mut Ui, control: &Control, rect: Rect, level: f64) {
        let r = ui.allocate_rect(rect, Sense::hover());
        let reading = if level <= SILENCE {
            "silent".to_owned()
        } else {
            format!("{level:+.1} dB")
        };
        let name = format!("{}: {reading}", control.label);
        r.widget_info(|| WidgetInfo::labeled(Role::ProgressIndicator, true, &name));
        let _ = r.on_hover_ui(|ui| {
            render::line(ui, control);
            ui.weak(&reading);
        });
        let vertical = rect.height() > rect.width();
        let bar = if vertical {
            Rect::from_center_size(
                rect.center(),
                egui::vec2(rect.width().min(18.0), rect.height()),
            )
        } else {
            Rect::from_center_size(
                rect.center(),
                egui::vec2(rect.width(), rect.height().min(14.0)),
            )
        };
        let v = ui.visuals();
        let painter = ui.painter();
        painter.rect(
            bar,
            2.0,
            v.extreme_bg_color,
            v.widgets.noninteractive.bg_stroke,
            StrokeKind::Inside,
        );
        let share = ((level - SILENCE) / (PEAK - SILENCE)).clamp(0.0, 1.0) as f32;
        let colour = if level > 0.0 {
            Color32::from_rgb(220, 70, 60)
        } else if level > -12.0 {
            Color32::from_rgb(220, 190, 70)
        } else {
            Color32::from_rgb(90, 190, 110)
        };
        let inner = bar.shrink(2.0);
        let fill = if vertical {
            Rect::from_min_max(
                egui::pos2(inner.left(), inner.bottom() - inner.height() * share),
                inner.max,
            )
        } else {
            Rect::from_min_max(
                inner.min,
                egui::pos2(inner.left() + inner.width() * share, inner.bottom()),
            )
        };
        if share > 0.0 {
            painter.rect_filled(fill, 1.0, colour);
        }
        // The 0 dB mark.
        let zero = ((0.0 - SILENCE) / (PEAK - SILENCE)) as f32;
        let mark = Stroke::new(1.0, v.text_color());
        if vertical {
            let y = inner.bottom() - inner.height() * zero;
            painter.hline(bar.x_range(), y, mark);
        } else {
            let x = inner.left() + inner.width() * zero;
            painter.vline(x, bar.y_range(), mark);
        }
    }
}

impl Drawings for DemoDrawings {
    fn draw(
        &mut self,
        ui: &mut Ui,
        key: &str,
        control: &Control,
        rect: Rect,
        value: &mut Value,
    ) -> Option<Vec<Edit>> {
        match control.id.as_str() {
            GAIN => Some(Self::fader(ui, key, control, rect, value)),
            METER => {
                Self::meter(ui, control, rect, self.level);
                Some(Vec::new())
            }
            _ => EditorDrawings.draw(ui, key, control, rect, value),
        }
    }

    /// A right click on a control with several faces opens its menu of
    /// faces.
    fn dress(&mut self, _ui: &mut Ui, d: Dressing<'_>) {
        if d.control.faces.len() < 2 {
            return;
        }
        let Some(r) = d.label.or(d.response) else {
            return;
        };
        let now = self.faces.get(d.key).cloned();
        let mut picked = None;
        let _ = r.context_menu(|ui| {
            ui.weak("Face");
            for v in &d.control.faces {
                let label = egui_hud::face::face(&v.id).map_or(v.id.as_str(), |f| f.label);
                if ui
                    .selectable_label(now.as_deref() == Some(v.id.as_str()), label)
                    .clicked()
                {
                    picked = Some((d.key.to_owned(), v.id.clone()));
                    ui.close();
                }
            }
        });
        if picked.is_some() {
            self.picked = picked;
        }
    }
}

/// The demo's page and its state.
pub struct Demo {
    /// The model: the controls in the box, their values, their layout.
    pub bench: Bench,
    /// Why the catalogue did not load, when it did not.
    pub error: Option<String>,
    drawings: DemoDrawings,
    /// The box's width, in points, as set (the wobble swings around it).
    pub width: f32,
    /// Swing the width around its value, to see the layout hold.
    pub wobble: bool,
    /// The slot columns drawn behind the controls.
    pub grid: bool,
    /// The preset loaded last.
    pub preset: String,
    /// The controls ticked in the list, to group into a case.
    pub marked: BTreeSet<String>,
    /// How each control was drawn in the last frame, by key.
    pub drawn: BTreeMap<String, Drawn>,
    /// Where the box was drawn in the last frame.
    pub zone: Option<Rect>,
    /// The last resolution drawn.
    pub last: Option<egui_hud::CardPlacement>,
    /// The edge's drag: the box's width and the pointer's x where it
    /// started.
    resize_from: Option<(f32, f32)>,
}

impl Default for Demo {
    fn default() -> Self {
        Self::new()
    }
}

impl Demo {
    /// The demo with its first preset in the box.
    pub fn new() -> Self {
        let (bench, error) = match bench() {
            Ok(b) => (b, None),
            Err(e) => (Bench::new(Kit::default()), Some(e)),
        };
        let preset = bench
            .presets
            .first()
            .map(|p| p.name.clone())
            .unwrap_or_default();
        Self {
            width: bench.width,
            bench,
            error,
            drawings: DemoDrawings::default(),
            wobble: false,
            grid: true,
            preset,
            marked: BTreeSet::new(),
            drawn: BTreeMap::new(),
            zone: None,
            last: None,
            resize_from: None,
        }
    }

    /// Loads the kit's preset `name`: its controls, width and context.
    pub fn load_preset(&mut self, name: &str) -> Result<(), String> {
        self.bench.load_preset(name)?;
        self.width = self.bench.width;
        name.clone_into(&mut self.preset);
        self.marked.clear();
        Ok(())
    }

    /// Sets the box's width, held to the bench's range.
    pub fn set_width(&mut self, width: f32) {
        self.bench.set_width(width);
        self.width = self.bench.width;
    }

    /// The first value in the box of a control, by its id.
    fn value_of(&self, control: &str) -> Option<Value> {
        let b = &self.bench;
        b.composition
            .pieces()
            .into_iter()
            .find(|(p, _)| p.control == control)
            .and_then(|(p, _)| b.value(&p.key))
    }

    /// What a preview reads of a shelf entry (`gain`, `low-cut`): the
    /// first value in the box of its control, or its default.
    pub fn read(&self, name: &str) -> Option<Value> {
        let e = self.bench.shelf.get(name)?;
        self.value_of(&e.control)
            .or_else(|| Some(e.descriptor.default.clone()))
    }

    /// The mixer controls, as the channel strip preview reads them.
    pub fn channel(&self) -> preview::Channel {
        let num = |n: &str, or: f64| self.read(n).and_then(|v| v.as_f64()).unwrap_or(or);
        let on = |n: &str| self.read(n) == Some(Value::Bool(true));
        let text = |n: &str, or: &str| match self.read(n) {
            Some(Value::Text(s) | Value::Choice(s)) => s,
            _ => or.to_owned(),
        };
        preview::Channel {
            gain: num(GAIN, 0.0),
            muted: on("mute"),
            pan: num("pan", 0.0),
            low_cut: on("low-cut"),
            eq: match self.read("eq") {
                Some(Value::Floats(f)) => f,
                _ => Vec::new(),
            },
            name: text("name", ""),
            tag: text("tag", "None"),
        }
    }

    /// The light controls, as the lamp preview reads them.
    pub fn light(&self) -> preview::Light {
        let d = preview::Light::default();
        let num = |n: &str, or: f64| self.read(n).and_then(|v| v.as_f64()).unwrap_or(or);
        preview::Light {
            power: self
                .read("power")
                .map_or(d.power, |v| v == Value::Bool(true)),
            intensity: num("intensity", d.intensity),
            temperature: num("temperature", d.temperature),
            colour: match self.read("light-colour") {
                Some(Value::Floats(f)) if f.len() >= 3 => [f[0], f[1], f[2]],
                _ => d.colour,
            },
            hue: num("hue", d.hue),
            effect: match self.read("effect") {
                Some(Value::Choice(s)) => s,
                _ => d.effect,
            },
        }
    }

    /// Whether a control is in the box, by its id.
    pub fn holds(&self, control: &str) -> bool {
        self.bench
            .composition
            .pieces()
            .iter()
            .any(|(p, _)| p.control == control)
    }

    /// The meter's reading at time `t`: the first gain and mute in the box
    /// (0 dB and not muted without them).
    pub fn level_at(&self, t: f64) -> f64 {
        let gain = self.value_of(GAIN).and_then(|v| v.as_f64()).unwrap_or(0.0);
        let muted = self.value_of(MUTE) == Some(Value::Bool(true));
        signal(gain, muted, t)
    }

    /// Shows control `key` with face `face` (one its control accepts).
    pub fn pick_face(&mut self, key: &str, face: &str) {
        if let Some(p) = self.bench.composition.piece_mut(key) {
            p.variant = Some(face.to_owned());
        }
    }
}

//! The renderers (feature `render`): a control drawn with egui-pop's
//! visual parts, under the ergonomic contract. A renderer never holds the
//! value: it is handed the value of the frame and reports the changes,
//! which the caller writes through the value flow ([`crate::flow`]).
//!
//! - [`card`]: a whole card from a [`resolve`](mod@crate::resolve) result:
//!   every shown part at its rect, its chrome, and the folded parts under
//!   the "More (n)" row ([`more`]), whose hover gives the reasons and
//!   whose rows ask for `controls.reveal` ([`CardEvent`]).
//! - [`part`]: one part at its rect, by its placement's [`Look`]
//!   (representation, orientation, variant, integration). A managed part
//!   is drawn by its value type's renderer; an integrated one by the
//!   host's [`Drawings`] where its own drawing fits (its full
//!   representation), else by its declared fallback ([`Fallback`]):
//!   another variant, the value type's standard renderer, or a case's
//!   parts.
//! - Numbers: the `slider` variant as egui-pop's slider (full: label and
//!   value inside the bar; compact: the value alone, the label in the
//!   tooltip; vertical: a fader, the value at the top and the label at the
//!   bottom), the `knob` variant as egui-pop's knob, the `scrub` variant
//!   and every simplified form as a scrub field ([`scrub`]).
//! - Other value types: a toggle, a drop-down (its entries with a
//!   preview each in the `preview-menu` face, [`Preview`]), text as a
//!   field. `points`: egui-pop's curve editor (the `curve` face, full),
//!   a list folded into a summary that opens its values under the card
//!   (`folded-list`, [`Folded`]), read-out lines under the card
//!   (`readout-lines`, [`Line`]), else numbered rows of x and y. A
//!   `colour`: egui-pop's offset colour wheel (the `colour-wheel` face,
//!   full), else a swatch and its channels. The editors report
//!   [`Edit::Tool`] and [`Edit::Cancel`].
//! - [`error_line`]: a card that could not be resolved, said in a line.
//! - [`configure`] and [`detent_hook`] give egui-pop's slider and knob a
//!   control's steps, sign and detents.
//! - A host dresses what the framework draws through [`Drawings`]: the
//!   label shown (a translation), a choice's words, its own touches on a
//!   slider (a picker button), a verdict on each slider change, the marks
//!   beside a part ([`Marks`]: changed, held, moved) and, once a part is
//!   drawn, its response ([`Dressing`]: names, menus, focus, hover).
//! - [`chip`]: the HUD read-out (`Exposure +0.67 EV`).
//! - [`hud_rect`] and [`hud_surface`]: a HUD card's rectangle and its
//!   surface between [`crate::hud`] and egui.
//! - [`handle`]: a handle dragged to edit a value, registered as an edit
//!   area with egui-pop's area registry (cursor, hover card, inert while
//!   its panel is stale). [`declare_handle`]: a handle whose drag
//!   another widget owns (egui-pop's viewport overlay), its edit area and
//!   hover card only; [`overlay_kind`] maps a [`crate::handle::Look`] to
//!   the overlay's handle kind.
//! - [`show`]: a control in a representation and a variant, outside a
//!   card.

use std::collections::BTreeMap;

use egui_pop::core::areas::{self, AreaSpec};
use egui_pop::core::icon::{self, Icon, IconWidget};
use egui_pop::core::indicator::{self, Indicator, Led, ValueMark};
use egui_pop::core::input::Input;
use egui_pop::core::kit::Tone;
use egui_pop::core::knob::{Endless, Knob};
use egui_pop::core::progress::Meter;
use egui_pop::core::sections;
use egui_pop::core::slider::{self, Slider, Verdict};
use egui_pop::core::theme::TextRole;
use egui_pop::core::thumbnail::Thumbnail;
use egui_pop::core::tooltip::TooltipExt as _;
use egui_pop::core::ui::UiPop;
use egui_pop::core::widgets::{self, SelectableRow, Switch, ToggleChip};
use egui_pop::egui::{self, Layout, Rect, Ui, Vec2, pos2, vec2};
use egui_pop::viewport::change::{
    Cause as ToolCause, Change as ToolChange, Verdict as ToolVerdict,
};
use egui_pop::viewport::curve::{self as curves, CurveEditor};
use egui_pop::viewport::wheel::{ColorWheel, WheelValue};

use crate::card::ItemKind;
use crate::flow::Cause;
use crate::model::{Control, HIT_AREA, Mode, Representation, Response, ValueSpec, ValueType};
use crate::registry::Registry;
use crate::resolve::{CardPlacement, Placement};
use crate::slot::{Fallback, Integration, Orientation};
use crate::value::Value;

/// One change a renderer reports.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Change {
    pub to: f64,
    pub cause: Cause,
    /// The gesture is over (every change but a drag's frames).
    pub done: bool,
    /// The drag was given up (Escape): back to `to`, no trace.
    pub cancelled: bool,
}

/// What a renderer did in a frame.
#[derive(Debug)]
pub struct Output {
    pub response: egui::Response,
    pub changes: Vec<Change>,
}

/// A slider cause as a control's.
pub fn cause_of(c: slider::Cause) -> Cause {
    match c {
        slider::Cause::Drag => Cause::Drag,
        slider::Cause::Key => Cause::Key,
        slider::Cause::Wheel => Cause::Wheel,
        slider::Cause::Typed => Cause::Typed,
        slider::Cause::Reset => Cause::Reset,
        slider::Cause::Access => Cause::Access,
    }
}

/// Gives egui-pop's slider a control's steps, sign, unit and default.
pub fn configure<'a>(slider: Slider<'a>, control: &Control) -> Slider<'a> {
    let Some(n) = control.value.number() else {
        return slider;
    };
    let mut s = slider
        .hard(n.hard.0..=n.hard.1)
        .step(n.step)
        .coarse(n.coarse)
        .precision(n.precision as usize)
        .display_scale(n.display_scale)
        .default(n.default);
    if let Some(u) = &n.unit {
        s = s.unit(u);
    }
    if n.signed {
        s = s.signed();
    }
    if n.integer {
        s = s.integer();
    }
    s
}

/// Whether the modifier that turns detents off (Alt) is held.
pub fn detents_off(ui: &Ui) -> bool {
    ui.input(|i| i.modifiers.alt)
}

/// A slider hook holding a drag on the control's detents (unless `free`):
/// the drag stays relative, so moving on crosses the detent.
pub fn detent_hook(control: &Control, free: bool) -> impl FnMut(&slider::Change) -> Verdict + '_ {
    move |c| {
        if c.cause != slider::Cause::Drag {
            return Verdict::Go;
        }
        let held = control.detented(c.to, free);
        if held == c.to {
            Verdict::Go
        } else {
            Verdict::Redirect(held)
        }
    }
}

/// The `slider` variant: egui-pop's slider with the control's steps and
/// detents.
pub fn slider_row(ui: &mut Ui, control: &Control, label: &str, value: &mut f64) -> Output {
    let soft = control.value.number().map_or((0.0, 1.0), |n| n.soft);
    let free = detents_off(ui);
    let out = configure(Slider::new(value, label, soft.0..=soft.1), control)
        .on_change(detent_hook(control, free))
        .show(ui);
    let mut changes: Vec<Change> = out
        .changes
        .iter()
        .map(|c| Change {
            to: c.to,
            cause: cause_of(c.cause),
            done: c.done,
            cancelled: false,
        })
        .collect();
    if out.response.drag_stopped() && !changes.iter().any(|c| c.done) {
        changes.push(Change {
            to: *value,
            cause: Cause::Drag,
            done: true,
            cancelled: true,
        });
    }
    Output {
        response: out.response,
        changes,
    }
}

/// The `scrub` variant: egui-pop's scrub field, whose value drags at the
/// control's sensitivity (its soft range over `span / sensitivity`
/// points). Ctrl is the fine mode, Alt turns detents off, Escape gives
/// the drag up, a double click resets, the arrows step (Page Up and Page
/// Down the coarse step), the wheel steps it under the pointer. An
/// endless number (one that wraps or is open) has no field: it is the
/// endless knob.
pub fn scrub(
    ui: &mut Ui,
    control: &Control,
    label: Option<&str>,
    value: &mut f64,
    width: f32,
) -> Output {
    let free = detents_off(ui);
    scrub_named(
        ui,
        control,
        label,
        &control.label,
        value,
        width,
        true,
        detent_hook(control, free),
    )
}

/// [`scrub`], named `name` to assistive technology and in its tooltip,
/// `menu` whether a right click opens egui-pop's value menu, `hook` seeing
/// each change (the detents, the host's verdict).
#[allow(
    clippy::too_many_arguments,
    reason = "a part's control, label, name, value and width, and its menu and hook"
)]
fn scrub_named<'a>(
    ui: &mut Ui,
    control: &Control,
    label: Option<&str>,
    name: &str,
    value: &'a mut f64,
    width: f32,
    menu: bool,
    hook: impl FnMut(&slider::Change) -> Verdict + 'a,
) -> Output {
    let height = ui.spacing().interact_size.y.max(HIT_AREA);
    if let Some(n) = control.value.number()
        && !n.bounded()
    {
        let out = knob(control, name, value, menu)
            .size(vec2(width, height))
            .on_change(hook)
            .show(ui);
        return scrubbed(slider_output(out.response, &out.changes, *value));
    }
    let soft = control.value.number().map_or((0.0, 1.0), |n| n.soft);
    let mut s = configure(
        Slider::new(value, label.unwrap_or(name), soft.0..=soft.1),
        control,
    )
    .scrub()
    .size(vec2(width, height))
    .menu(menu);
    let span = soft.1 - soft.0;
    if control.feel.sensitivity > 0.0 && span > 0.0 {
        s = s.travel((span / control.feel.sensitivity) as f32);
    }
    // Without a label it shows the value alone, the name in its tooltip.
    if label.is_none() {
        s = s.compact();
    }
    let out = s.on_change(hook).show(ui);
    scrubbed(slider_output(out.response, &out.changes, *value))
}

/// A scrub's drags reported as scrubs.
fn scrubbed(mut out: Output) -> Output {
    for c in &mut out.changes {
        if c.cause == Cause::Drag {
            c.cause = Cause::Scrub;
        }
    }
    out
}

/// The HUD read-out: one line, the control's label and value, in
/// egui-pop's card.
pub fn chip(ui: &mut Ui, control: &Control, value: f64) -> egui::Response {
    let shown = control
        .value
        .number()
        .map_or_else(String::new, |n| n.format(value));
    ui.card(Tone::Neutral, |ui| {
        ui.horizontal(|ui| {
            ui.muted(&control.label);
            ui.strong_text(shown);
        });
    })
    .response
}

/// A control in a representation, its variant `variant` (the default when
/// `None` or unknown), outside a card: in the width available, one row
/// high, horizontal, through the same renderers as a card's parts.
pub fn show(
    ui: &mut Ui,
    control: &Control,
    representation: Representation,
    variant: Option<&str>,
    value: &mut f64,
) -> Output {
    if representation == Representation::Simplified {
        return Output {
            response: chip(ui, control, *value),
            changes: Vec::new(),
        };
    }
    let row = ui.spacing().interact_size.y.max(HIT_AREA);
    let width = match representation {
        Representation::Full => ui.available_width().clamp(120.0, 320.0),
        _ => 110.0,
    };
    let look = Look {
        representation,
        orientation: Orientation::Horizontal,
        variant: variant.map(str::to_owned),
        integration: Integration::Managed,
    };
    ui.sized_ui(
        vec2(width, row),
        Layout::left_to_right(egui::Align::Center),
        |ui| {
            let rect = ui.max_rect();
            number_at(ui, control, &look, rect, value)
        },
    )
    .inner
}

/// How a part is drawn: what its placement says.
#[derive(Clone, Debug, PartialEq)]
pub struct Look {
    pub representation: Representation,
    pub orientation: Orientation,
    /// The variant (the control's default when `None`).
    pub variant: Option<String>,
    /// Who draws it on this surface.
    pub integration: Integration,
}

impl Look {
    /// A placement's look: its representation (full when it has none),
    /// orientation, variant and integration.
    pub fn of(p: &Placement) -> Self {
        Self {
            representation: p.representation.unwrap_or(Representation::Full),
            orientation: p.orientation.unwrap_or(Orientation::Horizontal),
            variant: p.variant.clone(),
            integration: p.integration.clone().unwrap_or_default(),
        }
    }

    /// Managed, in its default variant.
    pub fn managed(representation: Representation, orientation: Orientation) -> Self {
        Self {
            representation,
            orientation,
            variant: None,
            integration: Integration::Managed,
        }
    }
}

/// One edit a card's renderer reports, by the part's key.
#[derive(Clone, Debug, PartialEq)]
pub enum Edit {
    /// A number's change, inside a gesture or at its end.
    Number { key: String, change: Change },
    /// Another value set (a toggle, a choice, a curve's point, a
    /// colour's channel); `done` at the end of its gesture.
    Set { key: String, to: Value, done: bool },
    /// A value set by one of egui-pop's editors (a curve's points, a
    /// wheel's colour), by `cause`: a drag's frames are not `done`, its
    /// release is (one undo step); a key, a click, a point added or
    /// deleted are each `done`.
    Tool {
        key: String,
        to: Value,
        cause: ToolCause,
        done: bool,
    },
    /// An editor's drag given up (the cancel key): what its frames set
    /// is put back, with no trace.
    Cancel { key: String },
}

impl Edit {
    pub fn key(&self) -> &str {
        match self {
            Edit::Number { key, .. }
            | Edit::Set { key, .. }
            | Edit::Tool { key, .. }
            | Edit::Cancel { key } => key,
        }
    }

    /// The same edit for part `key`.
    fn rekeyed(self, key: String) -> Edit {
        match self {
            Edit::Number { change, .. } => Edit::Number { key, change },
            Edit::Set { to, done, .. } => Edit::Set { key, to, done },
            Edit::Tool {
                to, cause, done, ..
            } => Edit::Tool {
                key,
                to,
                cause,
                done,
            },
            Edit::Cancel { .. } => Edit::Cancel { key },
        }
    }
}

/// How an editor part (a curve, a colour wheel) is set up, as the host
/// knows it: the value its reset goes back to, how many points a curve
/// takes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Editor {
    /// The value a reset puts back: a curve's points (`x0, y0, x1, …`),
    /// a wheel's hue (in turns), chroma and luminance. `None`: a curve's
    /// identity, a wheel's neutral centre.
    pub default: Option<Value>,
    /// The most points a curve holds.
    pub max_points: Option<usize>,
}

/// A long list folded into a summary (`12 points`): its row shows the
/// label and the summary, a click opens its values under the card.
#[derive(Clone, Debug, PartialEq)]
pub struct Folded {
    /// What the row says of the list (`12 points`).
    pub summary: String,
    /// Its values open, a line each (a point, so many values).
    pub lines: Vec<String>,
    /// The hint on the folded row (`Show the values`).
    pub show: String,
    /// The hint on the open row (`Hide the values`).
    pub hide: String,
}

impl Folded {
    /// A list's values `per_line` a line, numbered from 1, at
    /// `precision` decimals; its summary counts the values.
    pub fn of(value: &Value, per_line: usize, precision: usize) -> Self {
        let values: Vec<String> = match value {
            Value::Floats(l) => l.iter().map(|x| format!("{x:.precision$}")).collect(),
            Value::Ints(l) => l.iter().map(ToString::to_string).collect(),
            _ => Vec::new(),
        };
        let lines = values
            .chunks(per_line.max(1))
            .enumerate()
            .map(|(i, line)| format!("{}: {}", i + 1, line.join("  ")))
            .collect();
        Self {
            summary: format!("{} values", values.len()),
            lines,
            show: "Show the values".into(),
            hide: "Hide the values".into(),
        }
    }
}

/// One read-out line under a card (a member's value: `Circle: 80 %`),
/// read only, with what it is in its hint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub text: String,
    pub hint: Option<String>,
}

impl Line {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            hint: None,
        }
    }

    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    /// A list's values as lines, numbered from 1.
    pub fn numbered(value: &Value) -> Vec<Line> {
        let values: Vec<String> = match value {
            Value::Floats(l) => l.iter().map(|x| format!("{x:.3}")).collect(),
            Value::Ints(l) => l.iter().map(ToString::to_string).collect(),
            _ => Vec::new(),
        };
        values
            .into_iter()
            .enumerate()
            .map(|(i, v)| Line::new(format!("{}: {v}", i + 1)))
            .collect()
    }
}

/// A choice's preview in the `preview-menu` face: a picture before its
/// words, in the menu and before the menu for the chosen one.
#[derive(Clone, Debug, PartialEq)]
pub struct Preview {
    pub image: PreviewImage,
    /// What the preview shows, in words (its accessible name and hint).
    pub hint: Option<String>,
}

/// What a preview shows.
#[derive(Clone, Debug, PartialEq)]
pub enum PreviewImage {
    /// One of egui-pop's icons.
    Icon(Icon),
    /// A small picture the host made (a texture and its size in pixels),
    /// drawn as egui-pop's thumbnail.
    Texture {
        texture: egui::TextureId,
        size: [u32; 2],
    },
}

impl Preview {
    pub fn icon(icon: Icon) -> Self {
        Self {
            image: PreviewImage::Icon(icon),
            hint: None,
        }
    }

    pub fn texture(texture: egui::TextureId, size: [u32; 2]) -> Self {
        Self {
            image: PreviewImage::Texture { texture, size },
            hint: None,
        }
    }

    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
}

/// The marks beside a part, as the host knows them: drawn in a gutter on
/// each side of the part's slot (the changed dot before it, the held and
/// moved marks after it).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Marks {
    /// Changed from its default: a dot that resets on a click.
    pub changed: bool,
    /// Held by someone else's gesture: who.
    pub held: Option<String>,
    /// Just moved by someone else: who.
    pub moved: Option<String>,
}

/// The gutter before a part that has marks, in points (the changed dot).
pub const MARK_BEFORE: f32 = 10.0;
/// The gutter after a part that has marks, in points (held, moved).
pub const MARK_AFTER: f32 = 12.0;

/// A part just drawn, handed to [`Drawings::dress`].
#[derive(Debug)]
pub struct Dressing<'a> {
    pub key: &'a str,
    pub control: &'a Control,
    /// The part's own rect (its slot less the marks' gutters).
    pub rect: Rect,
    /// The widget that answers for it, when one does (not for a host's
    /// own drawing).
    pub response: Option<&'a egui::Response>,
    /// Its label's own widget, when drawn apart from it (a drop-down's).
    pub label: Option<&'a egui::Response>,
    /// Where its changed dot is, when shown.
    pub changed: Option<Rect>,
    /// Where its moved mark is, when shown.
    pub moved: Option<Rect>,
}

/// The host's own drawings, for its integrated controls (a curve
/// editor, colour wheels): the framework knows nothing of them. The
/// other methods dress what the framework draws; each does nothing by
/// default.
pub trait Drawings {
    /// Draws part `key` (control `control`) in `rect` with the host's own
    /// drawing; `None` when the host has none for it, or it does not fit:
    /// the part's fallback is drawn instead.
    fn draw(
        &mut self,
        ui: &mut Ui,
        key: &str,
        control: &Control,
        rect: Rect,
        value: &mut Value,
    ) -> Option<Vec<Edit>>;

    /// The label shown for part `key` (a translation); the control's
    /// when `None`.
    fn label(&mut self, _ctx: &egui::Context, _key: &str, _control: &Control) -> Option<String> {
        None
    }

    /// The words shown for one choice of part `key`; the choice itself
    /// when `None`.
    fn choice(
        &mut self,
        _ctx: &egui::Context,
        _key: &str,
        _control: &Control,
        _choice: &str,
    ) -> Option<String> {
        None
    }

    /// The host's touches on part `key`'s slider before it shows (a
    /// picker button).
    fn slider<'a>(
        &mut self,
        _ctx: &egui::Context,
        _key: &str,
        _control: &Control,
        slider: Slider<'a>,
    ) -> Slider<'a> {
        slider
    }

    /// The host's verdict on a slider's or a knob's change of part `key`,
    /// once the control's detents have held it: a veto or a redirect wins.
    fn verdict(&mut self, _key: &str, _control: &Control, _change: &slider::Change) -> Verdict {
        Verdict::Go
    }

    /// Whether the sliders and knobs of part `key` open egui-pop's own
    /// value menu on a right click (or the context menu key): "Type a
    /// value", and "Reset" with a default. On by default; `false` for a
    /// host that gives the part a menu of its own (in [`dress`](Self::dress)),
    /// so a right click opens that menu alone. Its own "Type a value"
    /// item can call egui-pop's `slider::request_typed_entry`.
    fn value_menu(&mut self, _key: &str, _control: &Control) -> bool {
        true
    }

    /// The marks beside part `key`; `None` keeps the part's whole slot.
    fn marks(&mut self, _key: &str) -> Option<Marks> {
        None
    }

    /// Called once a part is drawn: the host's own touches on its
    /// response (names, menus, focus, hover text).
    fn dress(&mut self, _ui: &mut Ui, _dressing: Dressing<'_>) {}

    /// How part `key`'s editor (a curve, a colour wheel) is set up.
    fn editor(&mut self, _key: &str, _control: &Control) -> Editor {
        Editor::default()
    }

    /// The host's verdict on a change of part `key`'s curve before it is
    /// written (a step on its bus that may veto or redirect it).
    fn curve_change(
        &mut self,
        _key: &str,
        _control: &Control,
        _change: &ToolChange<Vec<[f32; 2]>>,
    ) -> ToolVerdict<Vec<[f32; 2]>> {
        ToolVerdict::Allow
    }

    /// The host's verdict on a change of part `key`'s colour wheel.
    fn wheel_change(
        &mut self,
        _key: &str,
        _control: &Control,
        _change: &ToolChange<WheelValue>,
    ) -> ToolVerdict<WheelValue> {
        ToolVerdict::Allow
    }

    /// Part `key`'s list folded (`folded-list`): its summary and its
    /// lines in the host's words; [`Folded::of`] when `None`.
    fn folded(
        &mut self,
        _ctx: &egui::Context,
        _key: &str,
        _control: &Control,
        _value: &Value,
    ) -> Option<Folded> {
        None
    }

    /// Part `key`'s read-out lines (`readout-lines`, a member's value
    /// each); [`Line::numbered`] when `None`.
    fn lines(
        &mut self,
        _ctx: &egui::Context,
        _key: &str,
        _control: &Control,
        _value: &Value,
    ) -> Option<Vec<Line>> {
        None
    }

    /// The preview of one choice of part `key` (`preview-menu`); none
    /// when `None`.
    fn preview(
        &mut self,
        _ctx: &egui::Context,
        _key: &str,
        _control: &Control,
        _choice: &str,
    ) -> Option<Preview> {
        None
    }
}

/// No drawings of the host's own: every integrated part falls back.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoDrawings;

impl Drawings for NoDrawings {
    fn draw(
        &mut self,
        _: &mut Ui,
        _: &str,
        _: &Control,
        _: Rect,
        _: &mut Value,
    ) -> Option<Vec<Edit>> {
        None
    }
}

/// What drew a part.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Drawn {
    /// The standard renderer for its value type.
    Managed,
    /// The host's own drawing.
    Integrated,
    /// Its fallback, as declared.
    Fallback(Fallback),
    /// An integrated part with no fallback and no drawing here: a
    /// placeholder (the exception the inspector counts).
    Missing,
}

/// What a part did in a frame.
#[derive(Debug)]
pub struct PartOutput {
    pub edits: Vec<Edit>,
    pub drawn: Drawn,
    /// The widget that answers for it, when one does.
    pub response: Option<egui::Response>,
    /// Its label's own widget, when drawn apart from it.
    pub label: Option<egui::Response>,
    /// Its quad button (a picker) was clicked.
    pub quad: bool,
    /// What it shows under the card (an open list's values, read-out
    /// lines), drawn by [`card`] after its box.
    pub below: Option<Below>,
}

/// What a part shows under its card, after the card's box: an open
/// list's values, or read-out lines.
#[derive(Clone, Debug, PartialEq)]
pub enum Below {
    Lines(Vec<Line>),
}

/// A card's request to its host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CardEvent {
    /// Show a folded part: `controls.reveal {node, key}`.
    Reveal(String),
    /// Show every part: `controls.reveal_all {node}`.
    RevealAll,
    /// Put a part back to its default (its changed dot clicked).
    Reset(String),
    /// A part's quad button (a picker) was clicked.
    Quad(String),
}

/// What a card did in a frame.
#[derive(Debug)]
pub struct CardOutput {
    pub response: egui::Response,
    pub edits: Vec<Edit>,
    pub events: Vec<CardEvent>,
    /// How each shown part was drawn, by key.
    pub drawn: BTreeMap<String, Drawn>,
}

/// The gap between two parts, in points.
const GAP: f32 = 2.0;

/// A whole card from its resolution: every shown part at its rect (from
/// the cursor, in the rack's width), its chrome, then the "More (n)" row
/// when anything is folded. `values` holds each part's value by key; the
/// renderers write into it as they go and report each change.
pub fn card(
    ui: &mut Ui,
    registry: &Registry,
    placement: &CardPlacement,
    values: &mut BTreeMap<String, Value>,
    drawings: &mut dyn Drawings,
) -> CardOutput {
    let (width, height) = placement.layout.as_ref().map_or_else(
        || (ui.available_width(), 0.0),
        |l| (l.rack.width, l.height()),
    );
    let mut edits = Vec::new();
    let mut drawn = BTreeMap::new();
    let mut events = Vec::new();
    let mut below = Vec::new();
    // The card's box at the cursor; its parts at their rects in it.
    let response = ui
        .sized_ui(
            vec2(width, height),
            Layout::top_down(egui::Align::Min),
            |ui| {
                let origin = ui.max_rect().min;
                for p in placement.items.iter().filter(|p| p.visibility.is_shown()) {
                    let Some(r) = p.rect else { continue };
                    let slot = Rect::from_min_size(origin + vec2(r.x, r.y), vec2(r.w, r.h))
                        .shrink(GAP / 2.0);
                    if p.kind == ItemKind::Chrome {
                        part_ui(ui, &p.key, slot, |ui| chrome(ui, p, slot));
                        continue;
                    }
                    let Some(control) = p.control.as_deref().and_then(|id| registry.get(id)) else {
                        continue;
                    };
                    let marks = drawings.marks(&p.key);
                    let rect = if marks.is_some() {
                        Rect::from_min_max(
                            slot.min + vec2(MARK_BEFORE, 0.0),
                            slot.max - vec2(MARK_AFTER, 0.0),
                        )
                    } else {
                        slot
                    };
                    let (changed, moved) = match &marks {
                        Some(m) => draw_marks(ui, &p.key, m, slot, rect, &mut events),
                        None => (None, None),
                    };
                    let value = values
                        .entry(p.key.clone())
                        .or_insert_with(|| default_of(control));
                    let out = part_ui(ui, &p.key, rect, |child| {
                        let out = part(
                            child,
                            registry,
                            &p.key,
                            control,
                            &Look::of(p),
                            rect,
                            value,
                            drawings,
                        );
                        drawings.dress(
                            child,
                            Dressing {
                                key: &p.key,
                                control,
                                rect,
                                response: out.response.as_ref(),
                                label: out.label.as_ref(),
                                changed,
                                moved,
                            },
                        );
                        out
                    });
                    if out.quad {
                        events.push(CardEvent::Quad(p.key.clone()));
                    }
                    // Its line, in plain words, when the pointer rests on it.
                    if !control.purpose.is_empty()
                        && let Some(r) = out.label.as_ref().or(out.response.as_ref())
                    {
                        let _ = r.clone().tip_ui(|ui| line(ui, control));
                    }
                    edits.extend(out.edits);
                    drawn.insert(p.key.clone(), out.drawn);
                    below.extend(out.below.map(|b| (p.key.clone(), b)));
                }
            },
        )
        .response;
    for (key, b) in &below {
        below_ui(ui, key, b);
    }
    if let Some(m) = &placement.more
        && m.count + m.inapplicable.len() > 0
    {
        let toolbar = placement
            .items
            .first()
            .is_some_and(|p| p.mode == Mode::Toolbar);
        events.extend(more(ui, placement, toolbar));
    }
    CardOutput {
        response,
        edits,
        events,
        drawn,
    }
}

/// What a part shows under its card, indented: its lines, wrapped to
/// the card's width, each with its hint.
fn below_ui(ui: &mut Ui, key: &str, below: &Below) {
    let Below::Lines(lines) = below;
    ui.push_id(("hud-below", key), |ui| {
        ui.indent("lines", |ui| {
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
            for l in lines {
                let r = ui.text_role(TextRole::Body, &l.text);
                if let Some(h) = &l.hint {
                    let _ = r.tip(h);
                }
            }
        });
    });
}

/// A card that could not be resolved (its resolution's error), said in
/// a line in the danger tone, where the card would be.
pub fn error_line(ui: &mut Ui, message: &str) -> egui::Response {
    ui.scope(|ui| {
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
        ui.text_tone(Tone::Danger, message)
    })
    .inner
}

/// A control's line, as its tooltip shows it: its name, then what it
/// does (its purpose), in plain words.
pub fn line(ui: &mut Ui, control: &Control) {
    ui.set_max_width(280.0);
    ui.strong_text(&control.label);
    ui.text_role(TextRole::Body, &control.purpose);
}

/// A part's value at its control's default.
pub fn default_of(control: &Control) -> Value {
    match &control.value {
        ValueSpec::Number(n) => Value::Float(n.default),
        ValueSpec::Bool { default } => Value::Bool(*default),
        ValueSpec::OneOf { default, .. } => Value::Choice(default.clone()),
        ValueSpec::Other { stored } => match stored {
            ValueType::Text => Value::Text(String::new()),
            _ => Value::Floats(Vec::new()),
        },
    }
}

/// `add` for one part in `rect`: left to right, clipped to it.
fn part_ui<R>(ui: &mut Ui, key: &str, rect: Rect, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.push_id(("hud-part", key), |ui| {
        ui.rect_ui(rect, Layout::left_to_right(egui::Align::Center), add)
            .inner
    })
    .inner
}

/// A part's marks in its gutters, egui-pop's value marks: the changed
/// dot before it (a button asking for a reset), then the held and moved
/// marks after it. Returns where the changed dot and the moved mark went.
fn draw_marks(
    ui: &mut Ui,
    key: &str,
    marks: &Marks,
    slot: Rect,
    part: Rect,
    events: &mut Vec<CardEvent>,
) -> (Option<Rect>, Option<Rect>) {
    let y = part.center().y;
    let row = 18.0_f32.min(slot.height());
    let mark = |ui: &mut Ui, id: (&str, &str), at: Rect, m: Indicator| {
        ui.push_id(id, |ui| {
            ui.rect_ui(
                at,
                Layout::centered_and_justified(egui::Direction::LeftToRight),
                |ui| ui.add(m.size(indicator::Size::Small)),
            )
            .inner
        })
        .inner
    };
    let mut changed = None;
    if marks.changed {
        let dot = Rect::from_center_size(
            pos2(slot.left() + MARK_BEFORE / 2.0, y),
            vec2(MARK_BEFORE, row),
        );
        let r = mark(
            ui,
            ("hud-changed", key),
            dot,
            Indicator::mark(ValueMark::Changed).interactive("Reset to its default"),
        );
        if r.clicked() {
            events.push(CardEvent::Reset(key.to_owned()));
        }
        changed = Some(r.rect);
    }
    let after = |dy: f32| {
        Rect::from_center_size(
            pos2(slot.right() - MARK_AFTER / 2.0, y + dy),
            vec2(MARK_AFTER, row / 2.0),
        )
    };
    // Both: the held mark above, the moved one below.
    let both = marks.held.is_some() && marks.moved.is_some();
    let shift = if both { row / 4.0 } else { 0.0 };
    if let Some(by) = &marks.held {
        mark(
            ui,
            ("hud-held", key),
            after(-shift),
            Indicator::mark(ValueMark::Held(by.clone())),
        );
    }
    let mut moved = None;
    if let Some(by) = &marks.moved {
        let r = mark(
            ui,
            ("hud-moved", key),
            after(shift),
            Indicator::mark(ValueMark::Moved(by.clone())),
        );
        moved = Some(r.rect);
    }
    (changed, moved)
}

/// A piece of chrome in its rect: a legend, a separator, a help line, an
/// icon's room.
fn chrome(ui: &mut Ui, p: &Placement, rect: Rect) {
    let kind = p
        .key
        .strip_prefix("chrome:")
        .and_then(|k| k.split('#').next())
        .unwrap_or_default();
    let text = p.label.clone().unwrap_or_default();
    match kind {
        "legend" => {
            ui.section_title(text);
        }
        "help" => {
            // A help line wraps in its rect, as a part's row does not.
            ui.scope(|ui| {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                ui.meta(text);
            });
        }
        "separator" => {
            // A rule across the slot.
            ui.rect_ui(
                rect,
                Layout::centered_and_justified(egui::Direction::TopDown),
                UiPop::divider,
            );
        }
        _ => {}
    }
}

/// One part in `rect`, by its look: the host's drawing where its own
/// drawing fits, else its fallback; a managed part by its value type.
#[allow(
    clippy::too_many_arguments,
    reason = "a part's key, control, look, rect and value, and where to draw"
)]
pub fn part(
    ui: &mut Ui,
    registry: &Registry,
    key: &str,
    control: &Control,
    look: &Look,
    rect: Rect,
    value: &mut Value,
    drawings: &mut dyn Drawings,
) -> PartOutput {
    let only = |edits: Vec<Edit>, drawn: Drawn| PartOutput {
        edits,
        drawn,
        response: None,
        label: None,
        quad: false,
        below: None,
    };
    let with = |m: Managed, drawn: Drawn| PartOutput {
        edits: m.edits,
        drawn,
        response: m.response,
        label: m.label,
        quad: m.quad,
        below: m.below,
    };
    if let Integration::Integrated { fallback } = &look.integration {
        // Its own drawing goes only where its full form was given.
        if look.representation == Representation::Full
            && let Some(edits) = drawings.draw(ui, key, control, rect, value)
        {
            return only(edits, Drawn::Integrated);
        }
        let Some(fallback) = fallback else {
            missing(ui, control, rect);
            return only(Vec::new(), Drawn::Missing);
        };
        let drawn = Drawn::Fallback(fallback.clone());
        return match fallback {
            Fallback::Variant(id) => {
                let look = Look {
                    variant: Some(id.clone()),
                    integration: Integration::Managed,
                    ..look.clone()
                };
                with(
                    managed(ui, key, control, &look, rect, value, drawings),
                    drawn,
                )
            }
            Fallback::ValueType => {
                // A list's or a colour's own rows, not its face's editor.
                let plain = matches!(
                    control.value.value_type(),
                    ValueType::Points | ValueType::Colour
                );
                let look = Look {
                    integration: Integration::Managed,
                    variant: if plain {
                        Some(VALUE_TYPE.to_owned())
                    } else {
                        look.variant.clone()
                    },
                    ..look.clone()
                };
                with(
                    managed(ui, key, control, &look, rect, value, drawings),
                    drawn,
                )
            }
            Fallback::Parts => only(
                parts(ui, registry, key, control, rect, value, drawings),
                drawn,
            ),
        };
    }
    with(
        managed(ui, key, control, look, rect, value, drawings),
        Drawn::Managed,
    )
}

/// An integrated part with nothing to draw here: a muted card saying so.
fn missing(ui: &mut Ui, control: &Control, rect: Rect) {
    ui.rect_ui(rect, Layout::left_to_right(egui::Align::Center), |ui| {
        ui.card(Tone::Muted, |ui| {
            ui.muted(format!("{}: no managed form", control.label));
        })
    });
}

/// A case's parts, each in its managed slot, laid out in `rect` as a
/// card of their own (the case's fallback). Their values are the case
/// value's members, by part name, when it is an object of them.
fn parts(
    ui: &mut Ui,
    registry: &Registry,
    key: &str,
    control: &Control,
    rect: Rect,
    value: &mut Value,
    drawings: &mut dyn Drawings,
) -> Vec<Edit> {
    let Some(case) = &control.case else {
        missing(ui, control, rect);
        return Vec::new();
    };
    let card = crate::card::Card::from_case(case);
    let keys = crate::context::ContextKeys::default();
    let columns = (rect.width() / keys.density.column()).max(0.25);
    let mut keys = keys;
    keys.set_box(crate::context::CardBox::new(
        crate::model::Surface::Panel,
        (columns * 12.0).floor() as u32,
        (rect.height() / keys.density.row() * 12.0).floor() as u32,
    ));
    let Ok(placement) = crate::resolve::resolve(
        registry,
        &card,
        &keys,
        &crate::resolve::Layouts::default(),
        &crate::resolve::Values::new(),
        &crate::resolve::Moment::default(),
        None,
    ) else {
        missing(ui, control, rect);
        return Vec::new();
    };
    // The case's value holds no members: each part starts at its default.
    let mut inner = BTreeMap::new();
    let out = card_at(ui, registry, &placement, &mut inner, drawings);
    let _ = value;
    out.into_iter()
        .map(|e| {
            let k = format!("{key}.{}", e.key());
            e.rekeyed(k)
        })
        .collect()
}

/// A card drawn from the cursor (a case's parts), its edits only.
fn card_at(
    ui: &mut Ui,
    registry: &Registry,
    placement: &CardPlacement,
    values: &mut BTreeMap<String, Value>,
    drawings: &mut dyn Drawings,
) -> Vec<Edit> {
    card(ui, registry, placement, values, drawings).edits
}

/// What a managed part drew.
struct Managed {
    edits: Vec<Edit>,
    response: Option<egui::Response>,
    label: Option<egui::Response>,
    quad: bool,
    below: Option<Below>,
}

impl Managed {
    fn edits(edits: Vec<Edit>) -> Self {
        Self {
            edits,
            response: None,
            label: None,
            quad: false,
            below: None,
        }
    }

    fn of(edits: Vec<Edit>, response: egui::Response) -> Self {
        Self {
            edits,
            response: Some(response),
            label: None,
            quad: false,
            below: None,
        }
    }
}

/// The variant a value type's fallback is drawn in: its plain rows (a
/// list's points, a colour's swatch and channels), not a face's editor.
const VALUE_TYPE: &str = "value-type";

/// A managed part by its value type, dressed by the host.
fn managed(
    ui: &mut Ui,
    key: &str,
    control: &Control,
    look: &Look,
    rect: Rect,
    value: &mut Value,
    drawings: &mut dyn Drawings,
) -> Managed {
    let ctx = ui.ctx().clone();
    let label = drawings
        .label(&ctx, key, control)
        .unwrap_or_else(|| control.label.clone());
    let label = label.as_str();
    let set = |value: &Value, done: bool| {
        vec![Edit::Set {
            key: key.to_owned(),
            to: value.clone(),
            done,
        }]
    };
    match &control.value {
        ValueSpec::Number(n) => {
            let mut v = value.as_f64().unwrap_or(n.default);
            let (out, quad) = number_with(ui, key, control, label, look, rect, &mut v, drawings);
            *value = if n.integer {
                Value::Int(v.round() as i64)
            } else {
                Value::Float(v)
            };
            let edits = out
                .changes
                .into_iter()
                .map(|change| Edit::Number {
                    key: key.to_owned(),
                    change,
                })
                .collect();
            Managed {
                quad,
                ..Managed::of(edits, out.response)
            }
        }
        ValueSpec::Bool { default } => {
            let mut on = match value {
                Value::Bool(b) => *b,
                _ => *default,
            };
            let shown = if look.representation == Representation::Full {
                label
            } else {
                ""
            };
            let face = control
                .face_or_default(look.variant.as_deref())
                .map_or("checkbox", |v| v.id.as_str());
            let r = match face {
                "push-button" => ui.add(
                    ToggleChip::new(&mut on, if shown.is_empty() { "●" } else { shown })
                        .name(label),
                ),
                "lamp" => lamp(ui, on, shown),
                "switch" if shown.is_empty() => ui.add(Switch::without_text(&mut on).name(label)),
                "switch" => widgets::switch(ui, &mut on, shown),
                _ => widgets::checkbox(ui, &mut on, shown),
            };
            let r = if shown.is_empty() {
                r.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, true, on, label));
                r.tip(label)
            } else {
                r
            };
            let mut edits = Vec::new();
            if r.changed() {
                *value = Value::Bool(on);
                edits = set(value, true);
            }
            Managed::of(edits, r)
        }
        ValueSpec::OneOf { choices, default } => {
            let mut chosen = match value {
                Value::Choice(c) => c.clone(),
                _ => default.clone(),
            };
            let before = chosen.clone();
            let full = look.representation == Representation::Full;
            let label_r = full.then(|| ui.text_role(TextRole::Body, label));
            let width = ui.available_width();
            let words: Vec<String> = choices
                .iter()
                .map(|c| {
                    drawings
                        .choice(&ctx, key, control, c)
                        .unwrap_or_else(|| c.clone())
                })
                .collect();
            let shown = choices
                .iter()
                .position(|c| *c == chosen)
                .map_or_else(|| chosen.clone(), |i| words[i].clone());
            let face = control
                .face_or_default(look.variant.as_deref())
                .map_or("menu", |v| v.id.as_str());
            let r = match face {
                "segmented" => {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 1.0;
                        for (c, w) in choices.iter().zip(&words) {
                            widgets::selectable_value(ui, &mut chosen, c.clone(), w);
                        }
                    })
                    .response
                }
                "radio" => {
                    ui.vertical(|ui| {
                        for (c, w) in choices.iter().zip(&words) {
                            widgets::radio_value(ui, &mut chosen, c.clone(), w);
                        }
                    })
                    .response
                }
                "preview-menu" => {
                    let previews: Vec<Option<Preview>> = choices
                        .iter()
                        .map(|c| drawings.preview(&ctx, key, control, c))
                        .collect();
                    let at = choices.iter().position(|c| *c == chosen);
                    ui.horizontal(|ui| {
                        if let Some(p) = at.and_then(|i| previews[i].as_ref()) {
                            preview_ui(ui, p, &shown);
                        }
                        let combo = widgets::combo_box(("hud-choice", key))
                            .selected_text(shown.clone())
                            .width(ui.available_width());
                        ui.combo_box(combo, |ui| {
                            for ((c, w), p) in choices.iter().zip(&words).zip(&previews) {
                                ui.horizontal(|ui| {
                                    if let Some(p) = p {
                                        preview_ui(ui, p, w);
                                    }
                                    widgets::selectable_value(ui, &mut chosen, c.clone(), w);
                                });
                            }
                        })
                        .response
                    })
                    .inner
                }
                _ => {
                    let combo = widgets::combo_box(("hud-choice", key))
                        .selected_text(shown)
                        .width(width);
                    ui.combo_box(combo, |ui| {
                        for (c, w) in choices.iter().zip(&words) {
                            widgets::selectable_value(ui, &mut chosen, c.clone(), w);
                        }
                    })
                    .response
                }
            };
            let r = match &label_r {
                Some(l) => r.labelled_by(l.id),
                None => r.tip(label),
            };
            let mut edits = Vec::new();
            if chosen != before {
                *value = Value::Choice(chosen);
                edits = set(value, true);
            }
            Managed {
                label: label_r,
                ..Managed::of(edits, r)
            }
        }
        ValueSpec::Other { stored } => match stored {
            ValueType::Points => {
                let face = if look.variant.as_deref() == Some(VALUE_TYPE) {
                    VALUE_TYPE
                } else {
                    control
                        .face_or_default(look.variant.as_deref())
                        .map_or("point-list", |v| v.id.as_str())
                };
                let full = look.representation == Representation::Full;
                match face {
                    "curve" if full => curve(ui, key, control, label, rect, value, drawings),
                    "folded-list" => {
                        let folded = drawings
                            .folded(&ctx, key, control, value)
                            .unwrap_or_else(|| Folded::of(value, 8, 3));
                        folded_list(ui, key, label, &folded)
                    }
                    "readout-lines" => {
                        let lines = drawings
                            .lines(&ctx, key, control, value)
                            .unwrap_or_else(|| Line::numbered(value));
                        let r = ui.text_role(TextRole::Body, label);
                        Managed {
                            below: Some(Below::Lines(lines)),
                            ..Managed::of(Vec::new(), r)
                        }
                    }
                    _ => {
                        let menu = drawings.value_menu(key, control);
                        Managed::edits(points(ui, key, label, rect, value, menu))
                    }
                }
            }
            ValueType::Colour => {
                let face = if look.variant.as_deref() == Some(VALUE_TYPE) {
                    VALUE_TYPE
                } else {
                    control
                        .face_or_default(look.variant.as_deref())
                        .map_or("colour-wheel", |v| v.id.as_str())
                };
                if face == "colour-wheel" && look.representation == Representation::Full {
                    wheel(ui, key, control, label, rect, value, drawings)
                } else {
                    let menu = drawings.value_menu(key, control);
                    Managed::edits(colour(ui, key, label, rect, value, menu))
                }
            }
            ValueType::Text => {
                let mut text = match value {
                    Value::Text(t) => t.clone(),
                    _ => String::new(),
                };
                // A compact cell: one row of the part's width.
                let row = ui.spacing().interact_size.y.min(rect.height());
                let out = Input::text(&mut text, label)
                    .id_salt(("hud-text", key))
                    .cell(vec2(rect.width(), row))
                    .show(ui);
                let r = out.response.tip(label);
                let mut edits = Vec::new();
                if out.changed {
                    *value = Value::Text(text);
                }
                // Typing is reported as it goes; leaving the field ends it.
                if out.changed || r.lost_focus() {
                    edits = set(value, r.lost_focus());
                }
                Managed::of(edits, r)
            }
            _ => {
                let r = ui.text_role(TextRole::Body, format!("{label}: {}", value.to_json()));
                Managed::of(Vec::new(), r)
            }
        },
    }
}

/// A choice's preview: egui-pop's icon, or the host's picture as a
/// thumbnail, named by its hint (or the choice's words).
fn preview_ui(ui: &mut Ui, preview: &Preview, words: &str) -> egui::Response {
    let name = preview.hint.as_deref().unwrap_or(words);
    let r = match &preview.image {
        PreviewImage::Icon(icon) => ui.add(Indicator::icon(*icon).name(name)),
        PreviewImage::Texture { texture, size } => {
            let h = ui.spacing().interact_size.y;
            let w = (h * size[0] as f32 / size[1].max(1) as f32).clamp(h / 2.0, h * 3.0);
            ui.add(Thumbnail::new(name).image(*texture, *size).size(vec2(w, h)))
        }
    };
    match &preview.hint {
        Some(h) => r.tip(h),
        None => r,
    }
}

/// A tool's changes as edits of part `key`: each as it was written, by
/// its cause; a drag that stopped with no change done was given up.
fn tool_edits<T>(
    key: &str,
    changes: &[ToolChange<T>],
    response: &egui::Response,
    value: impl Fn(&T) -> Value,
) -> Vec<Edit> {
    let mut edits: Vec<Edit> = changes
        .iter()
        .map(|c| Edit::Tool {
            key: key.to_owned(),
            to: value(&c.to),
            cause: c.cause,
            done: c.done,
        })
        .collect();
    if response.drag_stopped() && !changes.iter().any(|c| c.done) {
        edits.push(Edit::Cancel {
            key: key.to_owned(),
        });
    }
    edits
}

/// A curve's points from a list (`x0, y0, x1, y1, …`).
pub fn curve_points(list: &[f64]) -> Vec<[f32; 2]> {
    list.chunks_exact(2)
        .map(|p| [p[0] as f32, p[1] as f32])
        .collect()
}

/// A wheel's value from a list (hue in turns, chroma, luminance).
pub fn wheel_value(list: &[f64]) -> WheelValue {
    let at = |i: usize| list.get(i).copied().unwrap_or(0.0) as f32;
    WheelValue {
        hue: (at(0) * 360.0).rem_euclid(360.0),
        chroma: at(1),
        luminance: at(2),
    }
}

fn floats(value: &Value) -> Vec<f64> {
    match value {
        Value::Floats(l) => l.clone(),
        Value::Ints(l) => l.iter().map(|&x| x as f64).collect(),
        _ => Vec::new(),
    }
}

/// The `curve` face, full: its label, then egui-pop's curve editor over
/// the list's points, square in the slot; each change through the
/// host's verdict ([`Drawings::curve_change`]).
fn curve(
    ui: &mut Ui,
    key: &str,
    control: &Control,
    label: &str,
    rect: Rect,
    value: &mut Value,
    drawings: &mut dyn Drawings,
) -> Managed {
    let setup = drawings.editor(key, control);
    let list = floats(value);
    let mut points = if list.len() >= 4 {
        curve_points(&list)
    } else {
        curves::identity()
    };
    let defaults = match setup.default.as_ref().map(floats) {
        Some(l) if l.len() >= 4 => curve_points(&l),
        _ => curves::identity(),
    };
    let row = ui.spacing().interact_size.y;
    let side = rect.width().min(rect.height() - row).max(48.0);
    let out = ui
        .vertical(|ui| {
            let l = ui.text_role(TextRole::Body, label);
            let mut editor = CurveEditor::new(&mut points, label)
                .default_points(defaults)
                .size(vec2(side, side))
                .on_change(|c| drawings.curve_change(key, control, c));
            if let Some(n) = setup.max_points {
                editor = editor.max_points(n);
            }
            (l, editor.show(ui))
        })
        .inner;
    let (l, out) = out;
    let to_value = |p: &Vec<[f32; 2]>| {
        Value::Floats(
            p.iter()
                .flat_map(|[x, y]| [f64::from(*x), f64::from(*y)])
                .collect(),
        )
    };
    if !out.changes.is_empty() {
        *value = to_value(&points);
    }
    let edits = tool_edits(key, &out.changes, &out.response, to_value);
    Managed {
        label: Some(l),
        ..Managed::of(edits, out.response)
    }
}

/// The `colour-wheel` face, full: its label, then egui-pop's offset
/// wheel and its luminance slider over the list's hue (in turns),
/// chroma and luminance; each change through the host's verdict
/// ([`Drawings::wheel_change`]).
fn wheel(
    ui: &mut Ui,
    key: &str,
    control: &Control,
    label: &str,
    rect: Rect,
    value: &mut Value,
    drawings: &mut dyn Drawings,
) -> Managed {
    let setup = drawings.editor(key, control);
    let mut w = wheel_value(&floats(value));
    let default = match setup.default.as_ref().map(floats) {
        Some(l) if l.len() >= 3 => wheel_value(&l),
        _ => WheelValue::default(),
    };
    let row = ui.spacing().interact_size.y;
    // Room for the label above and the luminance slider below.
    let side = rect.width().min(rect.height() - 2.0 * row).max(40.0);
    let (l, out) = ui
        .vertical(|ui| {
            let l = ui.text_role(TextRole::Body, label);
            let out = ColorWheel::offset(&mut w, label)
                .default(default)
                .size(side)
                .on_change(|c| drawings.wheel_change(key, control, c))
                .show(ui);
            (l, out)
        })
        .inner;
    let to_value = |w: &WheelValue| {
        Value::Floats(vec![
            f64::from(w.hue) / 360.0,
            f64::from(w.chroma),
            f64::from(w.luminance),
        ])
    };
    if !out.changes.is_empty() {
        *value = to_value(&w);
    }
    let edits = tool_edits(key, &out.changes, &out.response, to_value);
    Managed {
        label: Some(l),
        ..Managed::of(edits, out.response)
    }
}

/// The `folded-list` face: a chevron, the label and the list's summary
/// (`12 points`); a click on the chevron or the summary opens its values
/// under the card, or closes them.
fn folded_list(ui: &mut Ui, key: &str, label: &str, folded: &Folded) -> Managed {
    let id = ui.make_persistent_id(("hud-folded", key));
    let open = ui.ctx().data(|m| m.get_temp::<bool>(id)).unwrap_or(false);
    let (chevron_icon, hint) = if open {
        (Icon::ChevronDown, folded.hide.as_str())
    } else {
        (Icon::ChevronRight, folded.show.as_str())
    };
    let chevron = ui
        .add(
            chevron_icon
                .tone(icon::Tone::Muted)
                .label(hint)
                .sense_click(),
        )
        .tip(hint);
    let l = ui.text_role(TextRole::Body, label);
    let summary = ui
        .add(
            ToggleChip::new(&mut open.clone(), &folded.summary)
                .name(format!("{label}: {}", folded.summary)),
        )
        .tip(hint);
    if chevron.clicked() || summary.clicked() {
        ui.ctx().data_mut(|m| m.insert_temp(id, !open));
    }
    let lines = folded.lines.iter().map(Line::new).collect();
    Managed {
        label: Some(l),
        below: open.then_some(Below::Lines(lines)),
        ..Managed::of(Vec::new(), summary)
    }
}

/// The `lamp` face: egui-pop's LED, lit when the value is on, and its
/// label; read only.
fn lamp(ui: &mut Ui, on: bool, label: &str) -> egui::Response {
    let r = ui
        .horizontal(|ui| {
            let led = if on { Led::On } else { Led::Off };
            ui.add(Indicator::led(led).name(label));
            if !label.is_empty() {
                ui.text_role(TextRole::Body, label);
            }
        })
        .response;
    r.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, false, on, label));
    r
}

/// The `points` value type's standard renderer: one numbered row per
/// point, its x and y as compact sliders over 0 to 1, in as many rows as
/// the rect holds (scrolled beyond); `menu`, whether each slider opens
/// egui-pop's value menu.
fn points(
    ui: &mut Ui,
    key: &str,
    label: &str,
    rect: Rect,
    value: &mut Value,
    menu: bool,
) -> Vec<Edit> {
    let mut list = match value {
        Value::Floats(l) => l.clone(),
        _ => Vec::new(),
    };
    let mut edits = Vec::new();
    let row = ui.spacing().interact_size.y;
    let index_w = 18.0;
    let gap = ui.spacing().item_spacing.x;
    let half = ((rect.width() - index_w - 2.0 * gap) / 2.0).max(slider::MIN_WIDTH);
    let mut changed: Option<bool> = None;
    let rows = |ui: &mut Ui, list: &mut Vec<f64>, changed: &mut Option<bool>| {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            for i in 0..list.len() / 2 {
                ui.horizontal(|ui| {
                    ui.sized_ui(
                        vec2(index_w, row),
                        Layout::centered_and_justified(egui::Direction::LeftToRight),
                        |ui| ui.muted(format!("{}", i + 1)),
                    );
                    for (j, axis) in ["x", "y"].into_iter().enumerate() {
                        let at = 2 * i + j;
                        let out = Slider::new(
                            &mut list[at],
                            &format!("{label} {} {axis}", i + 1),
                            0.0..=1.0,
                        )
                        .step(0.001)
                        .precision(3)
                        .compact()
                        .size(vec2(half, row))
                        .menu(menu)
                        .show(ui);
                        for c in &out.changes {
                            *changed = Some(changed.unwrap_or(false) || c.done);
                        }
                    }
                });
            }
        });
    };
    if (list.len() / 2) as f32 * (row + 1.0) > rect.height() + 0.5 {
        let area = egui::ScrollArea::vertical()
            .id_salt(("hud-points", key))
            .max_height(rect.height())
            .auto_shrink([false, true]);
        ui.scroll_area(area, |ui| rows(ui, &mut list, &mut changed));
    } else {
        rows(ui, &mut list, &mut changed);
    }
    if let Some(done) = changed {
        *value = Value::Floats(list);
        edits.push(Edit::Set {
            key: key.to_owned(),
            to: value.clone(),
            done,
        });
    }
    edits
}

/// The `colour` value type's standard renderer: a swatch, then its
/// channels (hue in turns, chroma, luminance) as compact sliders;
/// `menu`, whether each opens egui-pop's value menu.
fn colour(
    ui: &mut Ui,
    key: &str,
    label: &str,
    rect: Rect,
    value: &mut Value,
    menu: bool,
) -> Vec<Edit> {
    let mut list = match value {
        Value::Floats(l) => l.clone(),
        _ => Vec::new(),
    };
    list.resize(list.len().max(3), 0.0);
    let row = ui.spacing().interact_size.y;
    let gap = ui.spacing().item_spacing.x;
    let side = row.min(rect.height());
    let hsva = egui::ecolor::Hsva::new(
        list[0].rem_euclid(1.0) as f32,
        list[1].clamp(0.0, 1.0) as f32,
        (0.5 + list[2] * 0.5).clamp(0.0, 1.0) as f32,
        1.0,
    );
    ui.sized_ui(
        Vec2::splat(side),
        Layout::centered_and_justified(egui::Direction::LeftToRight),
        |ui| ui.add(Indicator::swatch(egui::Color32::from(hsva)).name(label)),
    )
    .inner
    .tip(label);
    let names = ["Hue", "Chroma", "Luminance"];
    let ranges = [(0.0, 1.0), (0.0, 1.0), (-1.0, 1.0)];
    let w = ((rect.width() - side - 3.0 * gap) / 3.0).max(slider::MIN_WIDTH);
    let mut changed: Option<bool> = None;
    for (i, name) in names.into_iter().enumerate() {
        let (lo, hi) = ranges[i];
        let out = Slider::new(&mut list[i], &format!("{label} {name}"), lo..=hi)
            .step(0.001)
            .precision(3)
            .compact()
            .size(vec2(w, row.min(rect.height())))
            .menu(menu)
            .show(ui);
        for c in &out.changes {
            changed = Some(changed.unwrap_or(false) || c.done);
        }
    }
    match changed {
        Some(done) => {
            *value = Value::Floats(list);
            vec![Edit::Set {
                key: key.to_owned(),
                to: value.clone(),
                done,
            }]
        }
        None => Vec::new(),
    }
}

/// A number in `rect` by its look: the `knob` variant as a knob, the
/// `scrub` variant and every simplified form as a scrub field, the
/// slider (horizontal or vertical, full or compact) otherwise.
pub fn number_at(
    ui: &mut Ui,
    control: &Control,
    look: &Look,
    rect: Rect,
    value: &mut f64,
) -> Output {
    let label = control.label.clone();
    number_with(
        ui,
        &control.id,
        control,
        &label,
        look,
        rect,
        value,
        &mut NoDrawings,
    )
    .0
}

/// A slider or knob hook: the control's detents (unless `free`), then
/// the host's verdict.
fn hooked<'a>(
    key: &'a str,
    control: &'a Control,
    free: bool,
    drawings: &'a mut dyn Drawings,
) -> impl FnMut(&slider::Change) -> Verdict + 'a {
    move |c| {
        let mut change = *c;
        if c.cause == slider::Cause::Drag {
            change.to = control.detented(c.to, free);
        }
        match drawings.verdict(key, control, &change) {
            Verdict::Veto => return Verdict::Veto,
            Verdict::Redirect(v) => change.to = v,
            Verdict::Go => {}
        }
        if change.to == c.to {
            Verdict::Go
        } else {
            Verdict::Redirect(change.to)
        }
    }
}

/// [`number_at`] for part `key`, labelled `label`, dressed by the host;
/// also says whether its quad button was clicked.
#[allow(
    clippy::too_many_arguments,
    reason = "a part's key, control, label, look, rect and value, and its host"
)]
fn number_with(
    ui: &mut Ui,
    key: &str,
    control: &Control,
    label: &str,
    look: &Look,
    rect: Rect,
    value: &mut f64,
    drawings: &mut dyn Drawings,
) -> (Output, bool) {
    let face = control
        .face_or_default(look.variant.as_deref())
        .map_or("slider", |v| v.id.as_str());
    let row = ui.spacing().interact_size.y;
    let vertical = look.orientation == Orientation::Vertical;
    // A one-row form keeps the row's height, centred in its slot.
    // A fader keeps a fader's width, centred in a wide slot.
    let rect = if vertical {
        Rect::from_center_size(
            rect.center(),
            vec2(rect.width().min(row * 3.0), rect.height()),
        )
    } else {
        rect
    };
    let bar = |h: f32| {
        if vertical {
            rect
        } else {
            Rect::from_center_size(rect.center(), vec2(rect.width(), h.min(rect.height())))
        }
    };
    let ctx = ui.ctx().clone();
    let free = detents_off(ui);
    let menu = drawings.value_menu(key, control);
    let body = |ui: &mut Ui| match (face, look.representation) {
        ("meter", Representation::Full) => (
            meter(ui, control, label, *value, rect.size(), vertical),
            false,
        ),
        // A meter in less room reads its value.
        ("readout" | "meter", _) => (readout(ui, control, label, *value, rect.width()), false),
        ("scrub", Representation::Full) => (
            scrub_named(
                ui,
                control,
                Some(label),
                label,
                value,
                rect.width(),
                menu,
                hooked(key, control, free, drawings),
            ),
            false,
        ),
        (_, Representation::Simplified) | ("scrub", _) => (
            scrub_named(
                ui,
                control,
                None,
                label,
                value,
                rect.width(),
                menu,
                hooked(key, control, free, drawings),
            ),
            false,
        ),
        ("knob" | "endless-knob", _) => (
            knob_in(ui, key, control, label, rect, value, drawings),
            false,
        ),
        (_, representation) => {
            let soft = control.value.number().map_or((0.0, 1.0), |n| n.soft);
            let size = if vertical {
                rect.size()
            } else {
                vec2(rect.width(), row.min(rect.height()))
            };
            let mut s = configure(Slider::new(value, label, soft.0..=soft.1), control)
                .size(size)
                .menu(menu);
            if control.feel.response == Response::Logarithmic {
                s = s.logarithmic();
            }
            if vertical {
                s = s.vertical();
            }
            if representation == Representation::Compact {
                s = s.compact();
            }
            let s = drawings.slider(&ctx, key, control, s);
            let out = s.on_change(hooked(key, control, free, drawings)).show(ui);
            let quad = out.quad_clicked();
            (slider_output(out.response, &out.changes, *value), quad)
        }
    };
    ui.rect_ui(
        bar(row.max(HIT_AREA)),
        Layout::left_to_right(egui::Align::Center),
        body,
    )
    .inner
}

/// The `readout` face: the value as the control formats it, read only.
fn readout(ui: &mut Ui, control: &Control, label: &str, value: f64, width: f32) -> Output {
    let text = control
        .value
        .number()
        .map_or_else(|| value.to_string(), |n| n.format(value));
    let height = ui.spacing().interact_size.y.max(HIT_AREA);
    let response = ui
        .sized_ui(
            vec2(width, height),
            Layout::centered_and_justified(egui::Direction::LeftToRight),
            |ui| ui.text_role(TextRole::Body, &text),
        )
        .inner;
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::Role::Label, true, format!("{label} {text}"))
    });
    Output {
        response: response.tip(label),
        changes: Vec::new(),
    }
}

/// The `meter` face: egui-pop's meter, a bar filled to the value over
/// the soft range, read only, along its orientation.
fn meter(
    ui: &mut Ui,
    control: &Control,
    label: &str,
    value: f64,
    size: egui::Vec2,
    vertical: bool,
) -> Output {
    let (lo, hi) = control.value.number().map_or((0.0, 1.0), |n| n.soft);
    let text = control
        .value
        .number()
        .map_or_else(|| value.to_string(), |n| n.format(value));
    let mut m = Meter::new(value, lo, hi.max(lo))
        .size(size)
        .label(label)
        .text(&text);
    if vertical {
        m = m.vertical();
    }
    let response = ui.add(m);
    Output {
        response: response.tip(format!("{label}: {text}")),
        changes: Vec::new(),
    }
}

/// A knob in `rect`, with the control's steps and detents.
fn knob_in(
    ui: &mut Ui,
    key: &str,
    control: &Control,
    label: &str,
    rect: Rect,
    value: &mut f64,
    drawings: &mut dyn Drawings,
) -> Output {
    let free = detents_off(ui);
    let menu = drawings.value_menu(key, control);
    let out = knob(control, label, value, menu)
        .size(rect.size())
        .on_change(hooked(key, control, free, drawings))
        .show(ui);
    slider_output(out.response, &out.changes, *value)
}

/// egui-pop's knob with the control's steps, unit, default and
/// sensitivity; endless when its number wraps (an angle, a hue) or is
/// open.
fn knob<'a>(control: &Control, label: &str, value: &'a mut f64, menu: bool) -> Knob<'a> {
    let soft = control.value.number().map_or((0.0, 1.0), |n| n.soft);
    let mut k = Knob::new(value, label, soft.0..=soft.1).menu(menu);
    if let Some(n) = control.value.number() {
        k = k
            .hard(n.hard.0..=n.hard.1)
            .step(n.step)
            .coarse(n.coarse)
            .precision(n.precision as usize)
            .display_scale(n.display_scale)
            .default(n.default);
        if let Some(u) = &n.unit {
            k = k.unit(u);
        }
        if n.signed {
            k = k.signed();
        }
        if n.integer {
            k = k.integer();
        }
        if n.wraps {
            k = k.endless(Endless::Wrap);
        } else if n.open {
            k = k.endless(Endless::Unbounded);
        }
        // The control's sensitivity: its soft range over that many points.
        let span = n.soft.1 - n.soft.0;
        if control.feel.sensitivity > 0.0 && span > 0.0 {
            k = k.travel((span / control.feel.sensitivity) as f32);
        }
    }
    k
}

/// A slider's or a knob's changes as a renderer's: a drag given up
/// (Escape) comes back as a cancelled change.
fn slider_output(response: egui::Response, changes: &[slider::Change], value: f64) -> Output {
    let mut changes: Vec<Change> = changes
        .iter()
        .map(|c| Change {
            to: c.to,
            cause: cause_of(c.cause),
            done: c.done,
            cancelled: false,
        })
        .collect();
    if response.drag_stopped() && !changes.iter().any(|c| c.done) {
        changes.push(Change {
            to: value,
            cause: Cause::Drag,
            done: true,
            cancelled: true,
        });
    }
    Output { response, changes }
}

/// A handle: an area a person drags to edit a value (a control moved on a
/// card, a width, a point on the image), registered with egui-pop's area
/// registry as an edit area once sensed, so the framework shows its drag
/// cursor and a hover card naming what it writes and its value, and
/// lists it for agents (`pop.ui.areas`). `area` is
/// `AreaSpec::edit(id, control).value(..).path(..).cursor(..)`; a handle
/// is visible at rest, so it needs no grip.
///
/// While the panel it is drawn in is stale (inside egui-pop's
/// `areas::scope` with `inert`), the handle senses hover only: it shows
/// no card and no edit cursor, and a drag does nothing.
pub fn handle(ui: &mut Ui, rect: Rect, id: egui::Id, area: AreaSpec) -> egui::Response {
    let sense = if areas::inert(ui) {
        egui::Sense::hover()
    } else {
        egui::Sense::click_and_drag()
    };
    let r = ui.interact(rect, id, sense);
    areas::register(ui, &r, area);
    r
}

/// A handle whose drag another widget owns (egui-pop's viewport overlay,
/// which begins, updates, ends and cancels its drags, and takes its
/// double clicks and wheel turns): its edit area and hover card only.
/// It senses the hover alone, so a press over it still reaches the
/// widget under it; the area is registered as [`handle`]'s is (cursor,
/// hover card, `pop.ui.areas`), inert while its panel is stale.
///
/// `at` is the handle's centre on screen; its area is egui-pop's
/// overlay hit target ([`HANDLE_TARGET`] wide).
///
/// It also takes the keyboard focus (never by a click, which stays the
/// overlay's): Tab and Shift+Tab walk the declared handles, and the
/// focused one gets egui-pop's focus ring. For AccessKit it is a node
/// named by the area's control, its value the area's value; with
/// [`handle_keys`] it becomes a slider (a size, an angle) with its
/// number. Inert, it takes no focus.
pub fn declare_handle(ui: &mut Ui, at: egui::Pos2, id: egui::Id, area: AreaSpec) -> egui::Response {
    let rect = Rect::from_center_size(at, Vec2::splat(HANDLE_TARGET));
    let sense = if areas::inert(ui) {
        egui::Sense::hover()
    } else {
        egui::Sense::hover() | egui::Sense::FOCUSABLE
    };
    let r = ui.interact(rect, id, sense);
    let (name, value) = (area.control.clone(), area.value.clone());
    areas::register(ui, &r, area);
    r.widget_info(|| {
        let mut info = egui::WidgetInfo::new(egui::accesskit::Role::Group);
        info.label.clone_from(&name);
        info.current_text_value.clone_from(&value);
        info
    });
    // The ring is egui-pop's focus plugin's, drawn around a widget the
    // keyboard focused: egui_hud paints no stroke of its own.
    r
}

/// What the keyboard asks of a focused declared handle this frame
/// ([`handle_keys`]).
#[derive(Clone, Debug, PartialEq)]
pub enum HandleKeys {
    /// One change, its writes as [`crate::handle::Handle::keyed`] gives
    /// them: the host applies it as one undo step through the value flow
    /// ([`crate::flow::apply_multi`], cause [`Cause::Key`]): a point
    /// writes its x and y together.
    Change(Vec<(String, f64)>),
    /// Escape: the host cancels a change under way
    /// ([`crate::flow::Gesture::cancel`]); the handle gives its focus up.
    Cancel,
}

/// The keyboard on a handle [`declare_handle`] returned, read with its
/// model and its parameters now: arrow keys move a point (Shift by a
/// larger step), Up and Down or `+` and `-` step a size or an angle,
/// Escape cancels ([`HandleKeys`]). The keys it reads are taken from the
/// frame, so arrows do not also move the focus. It also gives the
/// handle's AccessKit node its role (a slider for a size or an angle)
/// and its number; a point (or a host's own maths) keeps its group, with
/// two read-only children named `x` and `y` holding its position. `None`
/// unless focused and a key asked something.
pub fn handle_keys(
    ui: &mut Ui,
    response: &egui::Response,
    handle: &crate::handle::Handle,
    params: &dyn crate::handle::Params,
) -> Option<HandleKeys> {
    use crate::handle::Key;
    if let Some(v) = handle.value(params) {
        ui.ctx().accesskit_node_builder(response.id, |node| {
            node.set_role(egui::accesskit::Role::Slider);
            node.set_numeric_value(v);
        });
    } else {
        point_values(ui, response, handle, params);
    }
    if !response.has_focus() {
        return None;
    }
    ui.memory_mut(|m| {
        m.set_focus_lock_filter(
            response.id,
            egui::EventFilter {
                horizontal_arrows: true,
                vertical_arrows: true,
                escape: true,
                tab: false,
            },
        );
    });
    let keys = [
        (egui::Key::ArrowLeft, Key::Left),
        (egui::Key::ArrowRight, Key::Right),
        (egui::Key::ArrowUp, Key::Up),
        (egui::Key::ArrowDown, Key::Down),
        (egui::Key::Plus, Key::More),
        (egui::Key::Equals, Key::More),
        (egui::Key::Minus, Key::Less),
    ];
    let asked = ui.input_mut(|i| {
        if i.consume_key(egui::Modifiers::NONE, egui::Key::Escape) {
            return Some(HandleKeys::Cancel);
        }
        let large = i.modifiers.shift;
        let mods = if large {
            egui::Modifiers::SHIFT
        } else {
            egui::Modifiers::NONE
        };
        for (k, key) in keys {
            if i.consume_key(mods, k) {
                let writes = handle.keyed(params, key, large);
                return (!writes.is_empty()).then_some(HandleKeys::Change(writes));
            }
        }
        None
    });
    if asked == Some(HandleKeys::Cancel) {
        response.surrender_focus();
    }
    asked
}

/// A point's x and y as two AccessKit children of its handle's node: a
/// child `Ui` parented to the handle for AccessKit only, with two widgets
/// that sense nothing and take no room. Nothing is drawn.
fn point_values(
    ui: &mut Ui,
    response: &egui::Response,
    handle: &crate::handle::Handle,
    params: &dyn crate::handle::Params,
) {
    let [x, y] = handle.position(params);
    #[expect(
        clippy::disallowed_methods,
        reason = "an AccessKit-only child under the handle's node: no layout, nothing drawn"
    )]
    let child = ui.new_child(
        egui::UiBuilder::new()
            .id_salt(response.id.with("values"))
            .max_rect(Rect::from_center_size(response.rect.center(), Vec2::ZERO))
            .accessibility_parent(response.id),
    );
    for (axis, v) in [("x", x), ("y", y)] {
        let id = response.id.with(axis);
        let rect = Rect::from_center_size(response.rect.center(), Vec2::ZERO);
        let r = child.interact(rect, id, egui::Sense::empty());
        r.widget_info(|| {
            let mut info = egui::WidgetInfo::new(egui::accesskit::Role::Slider);
            info.label = Some(axis.to_owned());
            info.value = Some(v);
            info
        });
        ui.ctx()
            .accesskit_node_builder(id, |node| node.set_read_only());
    }
}

/// A declared handle's area, in points: egui-pop's overlay hit target.
pub const HANDLE_TARGET: f32 = 2.0 * egui_pop::viewport::overlay::HIT_RADIUS;

/// How egui-pop's overlay draws and hits a handle of this look.
pub fn overlay_kind(look: crate::handle::Look) -> egui_pop::viewport::overlay::HandleKind {
    use crate::handle::Look;
    use egui_pop::viewport::overlay::HandleKind as K;
    match look {
        Look::Move => K::Move,
        Look::Point => K::Point,
        Look::Corner => K::Corner,
        Look::Edge => K::Edge,
        Look::Size => K::Size,
        Look::Rotate => K::Rotate,
        Look::Feather => K::Feather,
        Look::Control => K::Control,
    }
}

/// A handle's position ([`crate::handle::Point`], surface units) as
/// egui's point, with no mapping (the host maps image to screen).
pub fn handle_pos(p: crate::handle::Point) -> egui::Pos2 {
    pos2(p[0] as f32, p[1] as f32)
}

/// A HUD card's rectangle ([`crate::hud::HudLayout::rect`]) as egui's:
/// the same points, no layout maths.
pub fn hud_rect(r: crate::hud::PointRect) -> Rect {
    Rect::from_min_size(pos2(r.x, r.y), vec2(r.w, r.h))
}

/// A HUD surface ([`crate::hud::HudSurface`]) from egui's rectangle.
pub fn hud_surface(r: Rect) -> crate::hud::PointRect {
    crate::hud::PointRect {
        x: r.min.x,
        y: r.min.y,
        w: r.width(),
        h: r.height(),
    }
}

/// The "More (n)" row under a card: egui-pop's section band (the
/// disclosure mark, "More", the count), or in a toolbar its "More tools"
/// button with the count. Its hover lists the folded parts and why; open,
/// it lists them as rows of the card's width under it, each to reveal,
/// and "Show all".
pub fn more(ui: &mut Ui, placement: &CardPlacement, toolbar: bool) -> Vec<CardEvent> {
    let Some(m) = &placement.more else {
        return Vec::new();
    };
    let id = ui.make_persistent_id("hud-more");
    let mut open = ui.data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
    let count = m.count as u64;
    let response = if toolbar {
        // egui-pop's icon button, the count as its badge.
        let title = m.title();
        let r = ui.add(
            IconWidget::new(Icon::More)
                .size(icon::Size::Toolbar)
                .label(&title)
                .sense_click(),
        );
        Indicator::count(count).on(ui, &r, indicator::Anchor::TopEnd);
        r
    } else {
        sections::band(ui, "hud-more", "More", Some(count), open)
    };
    let response = response.tip_ui(|ui| {
        for f in &m.folded {
            ui.text_role(
                TextRole::Body,
                format!(
                    "{}: {}",
                    f.label.as_deref().unwrap_or(&f.key),
                    f.reason.text
                ),
            );
        }
        for f in &m.inapplicable {
            ui.muted(format!(
                "{} (not here): {}",
                f.label.as_deref().unwrap_or(&f.key),
                f.reason.text
            ));
        }
    });
    if response.clicked() {
        open = !open;
        ui.data_mut(|d| d.insert_temp(id, open));
    }
    let mut events = Vec::new();
    if open {
        // Each entry a row of the card, its full width: the card's
        // frame grows to hold them.
        let row = vec2(
            ui.available_width().max(1.0),
            ui.spacing().interact_size.y.max(HIT_AREA),
        );
        let entry = |ui: &mut Ui, text: String| ui.sized(row, SelectableRow::new(false, text));
        for f in &m.folded {
            let name = f.label.as_deref().unwrap_or(&f.key);
            if entry(ui, format!("Show {name}"))
                .tip(&f.reason.text)
                .clicked()
            {
                events.push(CardEvent::Reveal(f.key.clone()));
            }
        }
        if m.folded.len() > 1 && entry(ui, "Show all".to_owned()).clicked() {
            events.push(CardEvent::RevealAll);
        }
    }
    events
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::{Card, CardItem};
    use crate::context::{CardBox, ContextKeys, Tier};
    use crate::model::Surface;
    use crate::registry::implicit;
    use crate::resolve::{Layout, Layouts, Moment, Values, resolve};
    use crate::value::{ParamDescriptor, ParamType, Widget};

    /// A curve, a colour wheel and an expert number, as implicit controls.
    fn card_of_three() -> (Registry, Card) {
        let mut curve = ParamDescriptor::new(
            ParamType::Floats,
            "Curve",
            Value::Floats(vec![0.0, 0.0, 1.0, 1.0]),
        );
        curve.widget = Some(Widget::Curve);
        let mut lift = ParamDescriptor::new(
            ParamType::Floats,
            "Lift",
            Value::Floats(vec![0.0, 0.0, 0.0]),
        );
        lift.widget = Some(Widget::Wheel);
        let mut amount = ParamDescriptor::new(ParamType::Float, "Amount", Value::Float(0.5));
        amount.hard = Some((0.0, 1.0));
        let c = implicit("node/n/curve", &curve);
        let w = implicit("node/n/lift", &lift);
        let a = implicit("node/n/amount", &amount);
        let card = Card::new(vec![
            CardItem::control("curve", &c.id),
            CardItem::control("lift", &w.id),
            CardItem::control("amount", &a.id).tier(Tier::Expert),
        ]);
        (Registry::with([c, w, a]).unwrap(), card)
    }

    fn resolved(registry: &Registry, card: &Card, layout: Layout) -> CardPlacement {
        let keys = ContextKeys::new(CardBox::new(Surface::Panel, 12, 240));
        resolve(
            registry,
            card,
            &keys,
            &Layouts::person(layout),
            &Values::new(),
            &Moment::default(),
            None,
        )
        .unwrap()
    }

    /// The host's own drawing for every integrated part.
    struct Own;

    impl Drawings for Own {
        fn draw(
            &mut self,
            ui: &mut Ui,
            _: &str,
            c: &Control,
            _: Rect,
            _: &mut Value,
        ) -> Option<Vec<Edit>> {
            ui.text_role(TextRole::Body, format!("{} drawn", c.label));
            Some(Vec::new())
        }
    }

    /// The card drawn once, and the names of what it put in the
    /// accessibility tree.
    fn draw(
        registry: &Registry,
        placement: &CardPlacement,
        drawings: &mut dyn Drawings,
    ) -> (CardOutput, Vec<String>) {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut out = None;
        let full = ctx.run_ui(egui::RawInput::default(), |ui| {
            let mut values = BTreeMap::from([
                ("curve".to_owned(), Value::Floats(vec![0.0, 0.0, 1.0, 1.0])),
                ("lift".to_owned(), Value::Floats(vec![0.1, 0.2, 0.3])),
            ]);
            out = Some(card(ui, registry, placement, &mut values, drawings));
        });
        // epaint panics when a frame's texture changes are dropped unread.
        let mut full = full;
        full.textures_delta.clear();
        let names = full
            .platform_output
            .accesskit_update
            .map(|u| {
                u.nodes
                    .iter()
                    .filter_map(|(_, n)| n.label().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        (out.unwrap(), names)
    }

    #[test]
    fn a_curve_and_a_wheel_are_egui_pops_editors_in_full_and_rows_where_they_cannot_go() {
        let (r, c) = card_of_three();
        // Full: egui-pop's editors, managed, whatever the host draws.
        let p = resolved(&r, &c, Layout::default());
        for drawings in [&mut Own as &mut dyn Drawings, &mut NoDrawings] {
            let (out, names) = draw(&r, &p, drawings);
            assert_eq!(out.drawn["curve"], Drawn::Managed);
            assert_eq!(out.drawn["lift"], Drawn::Managed);
            assert!(names.iter().any(|n| n == "Curve"), "{names:?}");
            assert!(names.iter().any(|n| n == "Lift"), "{names:?}");
            assert!(!names.iter().any(|n| n == "Curve 2 y"), "{names:?}");
            assert!(!names.iter().any(|n| n == "Lift Chroma"), "{names:?}");
        }
        // Compact (a row): the value type's rows.
        let compact = Layout::default()
            .represent("curve", Representation::Compact)
            .represent("lift", Representation::Compact);
        let p = resolved(&r, &c, compact);
        assert_eq!(
            p.get("curve").unwrap().representation,
            Some(Representation::Compact)
        );
        let (out, names) = draw(&r, &p, &mut Own);
        assert_eq!(out.drawn["curve"], Drawn::Managed);
        assert!(names.iter().any(|n| n == "Curve 1 x"), "{names:?}");
        assert!(names.iter().any(|n| n == "Lift Chroma"), "{names:?}");
    }

    #[test]
    fn a_curve_declared_integrated_is_the_hosts_in_full_and_falls_back_elsewhere() {
        let (r, c) = card_of_three();
        let mut curve = r.get("implicit:node/n/curve").unwrap().clone();
        for f in &mut curve.faces {
            *f = f
                .clone()
                .integrated(Surface::Panel, Integration::integrated(Fallback::ValueType));
        }
        let mut r = r;
        let _ = r.replace(curve);
        let p = resolved(&r, &c, Layout::default());
        let (out, _) = draw(&r, &p, &mut Own);
        assert_eq!(out.drawn["curve"], Drawn::Integrated);
        let (out, names) = draw(&r, &p, &mut NoDrawings);
        assert_eq!(out.drawn["curve"], Drawn::Fallback(Fallback::ValueType));
        assert!(names.iter().any(|n| n == "Curve 2 y"), "{names:?}");
    }

    #[test]
    fn folded_parts_go_under_the_more_row_named_with_its_count() {
        let (r, c) = card_of_three();
        let p = resolved(&r, &c, Layout::default());
        assert_eq!(p.more.as_ref().map(|m| m.count), Some(1));
        let (out, names) = draw(&r, &p, &mut Own);
        assert!(!out.drawn.contains_key("amount"), "folded, not drawn");
        assert!(names.iter().any(|n| n == "More, 1"), "{names:?}");
        assert!(out.events.is_empty());
    }

    #[test]
    fn a_number_draws_as_its_look_says() {
        let (r, _) = card_of_three();
        let amount = r.get("implicit:node/n/amount").unwrap();
        for (representation, orientation) in [
            (Representation::Full, Orientation::Horizontal),
            (Representation::Compact, Orientation::Horizontal),
            (Representation::Simplified, Orientation::Horizontal),
            (Representation::Full, Orientation::Vertical),
        ] {
            let ctx = egui::Context::default();
            ctx.enable_accesskit();
            let full = ctx.run_ui(egui::RawInput::default(), |ui| {
                let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(120.0, 96.0));
                let mut v = 0.5;
                number_at(
                    ui,
                    amount,
                    &Look::managed(representation, orientation),
                    rect,
                    &mut v,
                );
            });
            let mut full = full;
            full.textures_delta.clear();
            let update = full.platform_output.accesskit_update.unwrap();
            let node = update
                .nodes
                .iter()
                .map(|(_, n)| n)
                .find(|n| n.label() == Some("Amount"))
                .unwrap_or_else(|| panic!("{representation:?} {orientation:?}: no Amount"));
            assert_eq!(node.numeric_value(), Some(0.5));
            let vertical = node.orientation() == Some(egui::accesskit::Orientation::Vertical);
            assert_eq!(
                vertical,
                orientation == Orientation::Vertical && representation == Representation::Full
            );
        }
    }

    /// A host whose parts have a menu of their own.
    struct OwnMenu;

    impl Drawings for OwnMenu {
        fn draw(
            &mut self,
            _: &mut Ui,
            _: &str,
            _: &Control,
            _: Rect,
            _: &mut Value,
        ) -> Option<Vec<Edit>> {
            None
        }

        fn value_menu(&mut self, _: &str, _: &Control) -> bool {
            false
        }
    }

    /// Whether a right click on the amount drawn as `variant` (a slider
    /// or a knob) opens egui-pop's value menu, with `drawings` as host.
    fn right_click_opens_the_value_menu(variant: &str, drawings: &mut dyn Drawings) -> bool {
        let (r, _) = card_of_three();
        let amount = r.get("implicit:node/n/amount").unwrap();
        let look = Look {
            variant: Some(variant.to_owned()),
            ..Look::managed(Representation::Full, Orientation::Horizontal)
        };
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let rect = Rect::from_min_size(pos2(10.0, 10.0), vec2(160.0, 64.0));
        let mut v = 0.5;
        let mut names = Vec::new();
        let click = rect.center();
        let button = |pressed| egui::Event::PointerButton {
            pos: click,
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let inputs = [
            vec![egui::Event::PointerMoved(click)],
            vec![button(true)],
            vec![button(false)],
            Vec::new(),
            Vec::new(),
        ];
        for events in inputs {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(400.0, 300.0))),
                events,
                ..Default::default()
            };
            let mut full = ctx.run_ui(input, |ui| {
                let _ = number_with(
                    ui, "amount", amount, "Amount", &look, rect, &mut v, drawings,
                );
            });
            full.textures_delta.clear();
            names = full
                .platform_output
                .accesskit_update
                .map(|u| {
                    u.nodes
                        .iter()
                        .filter_map(|(_, n)| n.label().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default();
        }
        names.iter().any(|n| n == "Type a value")
    }

    #[test]
    fn a_host_with_its_own_menu_turns_the_value_menu_of_sliders_and_knobs_off() {
        for variant in ["slider", "knob"] {
            assert!(
                right_click_opens_the_value_menu(variant, &mut NoDrawings),
                "{variant}: egui-pop's menu by default"
            );
            assert!(
                !right_click_opens_the_value_menu(variant, &mut OwnMenu),
                "{variant}: the host's menu alone"
            );
        }
    }

    /// A host that renames every part, marks it changed and moved, and
    /// counts what it dressed.
    #[derive(Default)]
    struct Dressed {
        dressed: Vec<(String, bool, bool, bool)>,
    }

    impl Drawings for Dressed {
        fn draw(
            &mut self,
            _: &mut Ui,
            _: &str,
            _: &Control,
            _: Rect,
            _: &mut Value,
        ) -> Option<Vec<Edit>> {
            None
        }

        fn label(&mut self, _: &egui::Context, key: &str, _: &Control) -> Option<String> {
            Some(format!("{key} renamed"))
        }

        fn marks(&mut self, _: &str) -> Option<Marks> {
            Some(Marks {
                changed: true,
                held: None,
                moved: Some("an agent".into()),
            })
        }

        fn dress(&mut self, _: &mut Ui, d: Dressing<'_>) {
            self.dressed.push((
                d.key.to_owned(),
                d.response.is_some(),
                d.changed.is_some(),
                d.moved.is_some(),
            ));
        }
    }

    #[test]
    fn a_host_dresses_each_part_its_label_and_its_marks_in_gutters() {
        let (r, c) = card_of_three();
        let p = resolved(&r, &c, Layout::default());
        let mut host = Dressed::default();
        let (out, names) = draw(&r, &p, &mut host);
        assert!(out.events.is_empty());
        // The curve and the wheel are egui-pop's editors, renamed.
        assert!(names.iter().any(|n| n == "curve renamed"), "{names:?}");
        assert!(names.iter().any(|n| n == "lift renamed"), "{names:?}");
        for (key, _, changed, moved) in &host.dressed {
            assert!(*changed && *moved, "{key}: marks in the gutters");
        }
        assert_eq!(host.dressed.len(), 2, "the folded part is not drawn");
    }

    /// A handle's area as listed after two frames, drawn in a panel
    /// whose state is `stale` or current, and whether it sensed a drag.
    fn handle_area(stale: bool) -> (areas::AreaRecord, bool) {
        let ctx = egui::Context::default();
        let mut drags = false;
        for _ in 0..2 {
            let mut full = ctx.run_ui(egui::RawInput::default(), |ui| {
                areas::scope(ui, "params", stale, |ui| {
                    let r = handle(
                        ui,
                        Rect::from_min_size(pos2(10.0, 10.0), vec2(20.0, 20.0)),
                        ui.make_persistent_id("exposure-handle"),
                        AreaSpec::edit("hud.exposure", "Exposure")
                            .value("+0.30 EV")
                            .path("exposure")
                            .cursor(egui_pop::core::pointer::DragKind::Move),
                    );
                    drags = r.sense.senses_drag();
                });
            });
            full.textures_delta.clear();
        }
        let area = areas::list(&ctx)
            .into_iter()
            .find(|a| a.id == "hud.exposure")
            .expect("the handle is registered");
        (area, drags)
    }

    #[test]
    fn a_handle_is_an_edit_area_inert_while_its_panel_is_stale() {
        let (area, drags) = handle_area(false);
        assert_eq!(area.kind, areas::AreaKind::Edit);
        assert_eq!(area.control.as_deref(), Some("Exposure"));
        assert_eq!(area.value.as_deref(), Some("+0.30 EV"));
        assert_eq!(area.path.as_deref(), Some("exposure"));
        assert_eq!(area.panel.as_deref(), Some("params"));
        assert!(!area.inert && drags);
        let (area, drags) = handle_area(true);
        assert!(area.inert, "stale: inert");
        assert!(!drags, "stale: a drag does nothing");
    }

    /// Runs passes over a surface sensing drags (the overlay's stand-in)
    /// with a declared handle on top, the pointer pressed at the handle
    /// and moved after: whether the surface took the drag, whether the
    /// handle sensed one, and the handle's area.
    fn declared_over_a_surface() -> (bool, bool, areas::AreaRecord) {
        let ctx = egui::Context::default();
        let at = pos2(50.0, 50.0);
        let (mut surface_dragged, mut handle_drags) = (false, false);
        for frame in 0..8_u8 {
            let mut input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(200.0, 200.0))),
                ..Default::default()
            };
            let pointer = at + vec2(f32::from(frame.max(2)) * 4.0 - 8.0, 0.0);
            input.events.push(egui::Event::PointerMoved(pointer));
            if frame == 1 {
                input.events.push(egui::Event::PointerButton {
                    pos: pointer,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                });
            }
            let mut full = ctx.run_ui(input, |ui| {
                let surface = ui.interact(
                    Rect::from_min_size(pos2(0.0, 0.0), vec2(200.0, 200.0)),
                    egui::Id::unique("surface"),
                    egui::Sense::click_and_drag(),
                );
                surface_dragged |= surface.dragged();
                let r = declare_handle(
                    ui,
                    at,
                    egui::Id::unique("radius-handle"),
                    AreaSpec::edit("mask.radius", "Radius")
                        .value("20 px")
                        .path("radius"),
                );
                handle_drags |= r.sense.senses_drag() || r.dragged();
            });
            full.textures_delta.clear();
        }
        let area = areas::list(&ctx)
            .into_iter()
            .find(|a| a.id == "mask.radius")
            .expect("the declared handle is registered");
        (surface_dragged, handle_drags, area)
    }

    #[test]
    fn a_declared_handle_takes_no_drag_and_is_an_edit_area() {
        let (surface_dragged, handle_drags, area) = declared_over_a_surface();
        assert!(!handle_drags, "a declared handle senses no drag");
        assert!(surface_dragged, "the surface under it keeps its drag");
        assert_eq!(area.kind, areas::AreaKind::Edit);
        assert_eq!(area.control.as_deref(), Some("Radius"));
        assert_eq!(area.value.as_deref(), Some("20 px"));
        assert_eq!(area.path.as_deref(), Some("radius"));
        assert!(!area.inert);
        assert!((area.rect.width() - HANDLE_TARGET).abs() < 1e-3);
    }

    #[test]
    fn each_look_maps_to_the_overlay_s_kind() {
        use crate::handle::Look;
        use egui_pop::viewport::overlay::HandleKind as K;
        let all = [
            (Look::Move, K::Move),
            (Look::Point, K::Point),
            (Look::Corner, K::Corner),
            (Look::Edge, K::Edge),
            (Look::Size, K::Size),
            (Look::Rotate, K::Rotate),
            (Look::Feather, K::Feather),
            (Look::Control, K::Control),
        ];
        for (look, kind) in all {
            assert_eq!(overlay_kind(look), kind);
        }
    }
}

#[cfg(test)]
mod handle_keys_tests {
    use super::*;
    use crate::flow::{self, Cause, OneShot, Steps};
    use crate::handle::{Centre, Handle, HandleKind};
    use crate::registry::implicit;
    use crate::value::{MemoryStore, ParamDescriptor, ValueSource};
    use egui::accesskit::Role;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable as _;

    /// A mask's radius handle over a store: its keyboard changes go
    /// through the value flow, Escape counted.
    struct Mask {
        store: MemoryStore,
        cancels: usize,
    }

    fn radius(s: &MemoryStore) -> f64 {
        s.get("radius").and_then(|v| v.as_f64()).unwrap()
    }

    fn harness() -> Harness<'static, Mask> {
        let mut store = MemoryStore::new();
        store.declare("radius", ParamDescriptor::float("radius", 0.0, 500.0, 20.0));
        let handle = Handle::new(
            "radius",
            HandleKind::radius(Centre::new("x", "y"), "radius"),
        );
        Harness::new_ui_state(
            move |ui, m: &mut Mask| {
                let r = radius(&m.store);
                let params = [("x", 50.0), ("y", 50.0), ("radius", r)];
                let resp = declare_handle(
                    ui,
                    pos2(50.0 + r as f32, 50.0),
                    egui::Id::unique("mask.radius"),
                    AreaSpec::edit("mask.radius", "Mask radius")
                        .value(format!("{r} px"))
                        .path("radius"),
                );
                match handle_keys(ui, &resp, &handle, &params) {
                    Some(HandleKeys::Change(writes)) => {
                        let d = &m.store.params("radius")[0].descriptor;
                        let control = implicit("radius", d);
                        for (path, to) in writes {
                            flow::apply(
                                &mut m.store,
                                &Steps::new(),
                                &control,
                                &path,
                                OneShot {
                                    to,
                                    force: false,
                                    origin: "keys",
                                    cause: Cause::Key,
                                    device: "keys",
                                },
                            )
                            .unwrap();
                        }
                    }
                    Some(HandleKeys::Cancel) => m.cancels += 1,
                    None => {}
                }
            },
            Mask { store, cancels: 0 },
        )
    }

    #[test]
    fn tab_reaches_a_handle_and_arrows_change_it_one_step_each() {
        let mut h = harness();
        h.run();
        assert!(!h.get_by_label("Mask radius").is_focused());
        h.key_press(egui::Key::Tab);
        h.run();
        assert!(
            h.get_by_label("Mask radius").is_focused(),
            "Tab reaches the handle"
        );
        h.key_press(egui::Key::ArrowUp);
        h.run();
        assert_eq!(radius(&h.state().store), 21.0);
        assert_eq!(
            h.state().store.undo_steps().len(),
            1,
            "one key, one undo step"
        );
        h.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::ArrowDown);
        h.run();
        assert_eq!(radius(&h.state().store), 11.0, "Shift: a larger step");
        assert_eq!(h.state().store.undo_steps().len(), 2);
        let node = h.get_by_label("Mask radius");
        assert!(node.is_focused(), "arrows do not move the focus away");
        assert_eq!(node.value().as_deref(), Some("11 px"));
        h.get_by(|n| {
            n.role() == Role::Slider
                && n.label().as_deref() == Some("Mask radius")
                && n.numeric_value() == Some(11.0)
        });
        h.key_press(egui::Key::Escape);
        h.run();
        assert_eq!(h.state().cancels, 1);
        assert!(
            !h.get_by_label("Mask radius").is_focused(),
            "Escape gives the focus up"
        );
    }

    /// A mask's centre point over a store: one key, applied with
    /// [`flow::apply_multi`]; the outcomes kept.
    struct Centred {
        store: MemoryStore,
        outcomes: Vec<flow::MultiOutcome>,
    }

    fn at(s: &MemoryStore, p: &str) -> f64 {
        s.get(p).and_then(|v| v.as_f64()).unwrap()
    }

    fn point_harness() -> Harness<'static, Centred> {
        let mut store = MemoryStore::new();
        store.declare("x", ParamDescriptor::float("x", 0.0, 500.0, 40.0));
        store.declare("y", ParamDescriptor::float("y", 0.0, 500.0, 30.0));
        let handle = Handle::new("centre", HandleKind::point(Centre::new("x", "y")));
        Harness::new_ui_state(
            move |ui, m: &mut Centred| {
                let (x, y) = (at(&m.store, "x"), at(&m.store, "y"));
                let params = [("x", x), ("y", y)];
                let resp = declare_handle(
                    ui,
                    pos2(x as f32, y as f32),
                    egui::Id::unique("mask.centre"),
                    AreaSpec::edit("mask.centre", "Mask centre")
                        .value(format!("x {x}, y {y}"))
                        .path("x"),
                );
                if let Some(HandleKeys::Change(writes)) = handle_keys(ui, &resp, &handle, &params) {
                    let d = &m.store.params("x")[0].descriptor;
                    let control = implicit("x", d);
                    let out = flow::apply_multi(
                        &mut m.store,
                        &Steps::new(),
                        &control,
                        &writes,
                        flow::MultiShot {
                            force: false,
                            origin: "keys",
                            cause: Cause::Key,
                            device: "keys",
                        },
                    )
                    .unwrap();
                    m.outcomes.push(out);
                }
            },
            Centred {
                store,
                outcomes: Vec::new(),
            },
        )
    }

    #[test]
    fn one_arrow_on_a_point_is_one_commit_of_x_and_y() {
        let mut h = point_harness();
        h.run();
        h.key_press(egui::Key::Tab);
        h.run();
        assert!(h.get_by_label("Mask centre").is_focused());
        h.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::ArrowUp);
        h.run();
        let s = &h.state().store;
        assert_eq!((at(s, "x"), at(s, "y")), (40.0, 20.0));
        assert_eq!(s.undo_steps().len(), 1, "one key, one undo step");
        assert_eq!(h.state().outcomes.len(), 1, "one commit");
        let flow::MultiOutcome::Committed { changes } = &h.state().outcomes[0] else {
            panic!("committed: {:?}", h.state().outcomes);
        };
        assert_eq!(changes.len(), 1, "x did not move: only y changed");
        assert_eq!((changes[0].path.as_str(), changes[0].to), ("y", 20.0));
        h.key_press(egui::Key::ArrowRight);
        h.run();
        assert_eq!(h.state().store.undo_steps().len(), 2);
        // Undo takes the whole point back to where the key found it.
        h.state_mut().store.undo();
        let s = &h.state().store;
        assert_eq!((at(s, "x"), at(s, "y")), (40.0, 20.0));
    }

    #[test]
    fn a_point_shows_x_and_y_to_accesskit() {
        use egui_kittest::kittest::NodeT as _;
        let mut h = point_harness();
        h.run();
        let node =
            h.get_by(|n| n.role() == Role::Group && n.label().as_deref() == Some("Mask centre"));
        assert_eq!(node.value().as_deref(), Some("x 40, y 30"));
        // Under an unnamed container (the child `Ui`), which readers skip.
        let values: Vec<_> = node
            .children_recursive()
            .map(|c| c.accesskit_node())
            .filter(|c| c.label().is_some())
            .map(|c| (c.label(), c.role(), c.numeric_value(), c.is_read_only()))
            .collect();
        assert_eq!(
            values,
            [
                (Some("x".into()), Role::Slider, Some(40.0), true),
                (Some("y".into()), Role::Slider, Some(30.0), true),
            ],
            "x and y are read-only values under the handle"
        );
    }

    #[test]
    fn keys_step_a_point_a_size_and_an_angle() {
        use crate::handle::{AngleParam, Key, Reach};
        let p = [("x", 10.0), ("y", 10.0), ("r", 0.5), ("a", 179.5)];
        let point = Handle::new("c", HandleKind::point(Centre::new("x", "y")));
        assert_eq!(
            point.keyed(&p, Key::Up, true),
            vec![("x".into(), 10.0), ("y".into(), 0.0)]
        );
        assert!(point.keyed(&p, Key::More, false).is_empty());
        let size = Handle::new("r", HandleKind::radius(Centre::new("x", "y"), "r"));
        assert_eq!(size.keyed(&p, Key::Less, false), vec![("r".into(), 0.0)]);
        let angle = Handle::new(
            "a",
            HandleKind::angle(
                Centre::new("x", "y"),
                AngleParam::new("a"),
                Reach::Fixed(50.0),
            ),
        );
        assert_eq!(
            angle.keyed(&p, Key::Right, false),
            vec![("a".into(), -179.5)]
        );
        assert_eq!(point.value_text(&p), "10, 10");
        assert_eq!(size.value(&p), Some(0.5));
    }
}

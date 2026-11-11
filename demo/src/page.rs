//! The page: a title and its toolbar, the presets and the list of
//! controls on the left, the box in the middle (resized by its right
//! edge), the previews on the right.

use std::collections::BTreeMap;

use egui_hud::bench::{Entry, MISC_GROUP, Spot, WIDTH_RANGE};
use egui_hud::card::CardLayout;
use egui_hud::context::Density;
use egui_hud::render::{self, CardEvent};
use egui_hud::{CardPlacement, Value};
use egui_pop::egui::{
    self, Align2, CursorIcon, Rect, RichText, Sense, Shape, Stroke, StrokeKind, Ui, UiBuilder,
    pos2, vec2,
};
use serde_json::json;

use crate::{Demo, SOURCE, TITLE, preview};

/// The space left of the box and under it, in points.
const MARGIN: f32 = 4.0;

/// One row of the list of controls.
enum Row {
    /// A control, at the top level or in a case.
    Control {
        key: String,
        label: String,
        in_case: bool,
    },
    /// A case's legend.
    Case { key: String, legend: String },
}

/// A control the palette offers: its shelf name, label and line.
type Addable = (String, String, String);

/// What a row of the list asks for.
enum Action {
    Mark(String, bool),
    Move(String, bool),
    Remove(String),
    Ungroup(String),
}

/// A density's name, as the context key says it.
fn density_name(d: Density) -> String {
    match serde_json::to_value(d) {
        Ok(serde_json::Value::String(s)) => {
            let mut c = s.chars();
            c.next()
                .map(|f| f.to_uppercase().chain(c).collect())
                .unwrap_or_default()
        }
        _ => format!("{d:?}"),
    }
}

impl Demo {
    /// The whole page.
    pub fn ui(&mut self, ui: &mut Ui) {
        let t = ui.input(|i| i.time);
        self.drawings.level = self.level_at(t);
        // The meter and the previews move with time.
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(33));
        let width = if self.wobble {
            let column = self.bench.keys().density.column();
            self.width + (t * 2.0).sin() as f32 * column * 0.3
        } else {
            self.width
        };
        self.bench.set_width(width);

        egui::Panel::top("title").show(ui, |ui| self.title(ui));
        egui::Panel::left("controls")
            .resizable(true)
            .default_size(240.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| self.palette(ui));
            });
        egui::Panel::right("previews")
            .resizable(true)
            .default_size(330.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| self.previews(ui, t));
            });
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::both()
                .id_salt("box")
                .auto_shrink([false, false])
                .show(ui, |ui| self.canvas(ui));
        });
    }

    /// The title, the intro and the toolbar.
    fn title(&mut self, ui: &mut Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.heading(TITLE);
            ui.hyperlink_to("Source", SOURCE);
        });
        ui.label(
            "Real egui_hud controls in a box you resize: each takes the face, form and \
             orientation that fit its slot. Rest the pointer on one to read what it does; \
             right-click it to change its face.",
        );
        let density = self.bench.keys().density;
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(&self.preset).weak());
            ui.separator();
            ui.label("Width");
            let mut w = self.width;
            let r = ui.add(
                egui::Slider::new(&mut w, WIDTH_RANGE.0..=WIDTH_RANGE.1)
                    .show_value(false)
                    .integer(),
            );
            let text = format!(
                "{:.0} pt = {:.2} columns",
                self.width,
                self.width / density.column()
            );
            r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Slider, true, "Width"));
            if r.changed() {
                self.set_width(w);
            }
            ui.label(text);
            ui.separator();
            ui.label("Density");
            for d in Density::ALL {
                if ui
                    .selectable_label(density == d, density_name(d))
                    .on_hover_text(format!("One column is {} pt", d.column()))
                    .clicked()
                {
                    let _ = self.bench.set_context(&json!({ "density": d }));
                }
            }
            ui.separator();
            ui.checkbox(&mut self.grid, "Columns")
                .on_hover_text("Draw the slot columns behind the controls");
            ui.checkbox(&mut self.wobble, "Wobble").on_hover_text(
                "Swing the width by 0.3 column around its value: the layout holds \
                 instead of flickering",
            );
        });
        if let Some(e) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, e);
        }
    }

    /// The presets, the controls to add, and the list of those in the box.
    fn palette(&mut self, ui: &mut Ui) {
        ui.heading("Presets");
        let presets: Vec<(String, String)> = self
            .bench
            .presets
            .iter()
            .map(|p| (p.name.clone(), p.help.clone()))
            .collect();
        for (name, help) in presets {
            let r = ui.selectable_label(self.preset == name, &name);
            let r = if help.is_empty() {
                r
            } else {
                r.on_hover_text(help)
            };
            if r.clicked() {
                let _ = self.load_preset(&name);
            }
        }
        ui.separator();
        ui.heading("Add");
        let groups: Vec<(String, Vec<Addable>)> = self
            .bench
            .shelf
            .groups("")
            .into_iter()
            .filter(|(g, _)| g != MISC_GROUP)
            .map(|(g, entries)| {
                let entries = entries
                    .into_iter()
                    .filter(|e| !e.generic || e.configured.is_some())
                    .map(|e| (e.name.clone(), e.label.clone(), e.hint.clone()))
                    .collect();
                (g, entries)
            })
            .collect();
        for (group, entries) in groups {
            ui.label(RichText::new(group).weak());
            ui.horizontal_wrapped(|ui| {
                for (name, label, hint) in entries {
                    if ui.button(label).on_hover_text(hint).clicked() {
                        let _ = self.bench.add(&name, None);
                    }
                }
            });
        }
        ui.separator();
        ui.heading("In the box");
        ui.label(RichText::new("In logical order: Tab, MIDI banks").weak());
        let n = self.marked.len();
        if ui
            .add_enabled(n > 0, egui::Button::new(format!("Group {n} as a case")))
            .on_hover_text(
                "Tick controls below, then group them under a legend: they stay \
                 together, and nothing outside moves into their gaps",
            )
            .clicked()
        {
            self.group_marked();
        }
        let mut action = None;
        for row in self.rows() {
            ui.horizontal(|ui| match &row {
                Row::Case { key, legend } => {
                    ui.strong(legend);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .small_button("x")
                            .on_hover_text("Remove the case")
                            .clicked()
                        {
                            action = Some(Action::Remove(key.clone()));
                        }
                        if ui
                            .small_button("Ungroup")
                            .on_hover_text("Take the case apart: its controls stay")
                            .clicked()
                        {
                            action = Some(Action::Ungroup(key.clone()));
                        }
                    });
                }
                Row::Control {
                    key,
                    label,
                    in_case,
                } => {
                    let mut ticked = self.marked.contains(key);
                    let r = ui
                        .checkbox(&mut ticked, "")
                        .on_hover_text("Mark, to group into a case");
                    r.widget_info(|| {
                        egui::WidgetInfo::selected(
                            egui::Role::CheckBox,
                            true,
                            ticked,
                            format!("Mark {label}"),
                        )
                    });
                    if r.changed() {
                        action = Some(Action::Mark(key.clone(), ticked));
                    }
                    if *in_case {
                        ui.add_space(12.0);
                    }
                    let shown = self.drawn.contains_key(key);
                    let text = RichText::new(label);
                    ui.label(if shown { text } else { text.weak() })
                        .on_hover_text(if shown {
                            "In the box"
                        } else {
                            "Folded under More, or no room for it"
                        });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("x").on_hover_text("Remove").clicked() {
                            action = Some(Action::Remove(key.clone()));
                        }
                        if ui.small_button("v").on_hover_text("Move down").clicked() {
                            action = Some(Action::Move(key.clone(), false));
                        }
                        if ui.small_button("^").on_hover_text("Move up").clicked() {
                            action = Some(Action::Move(key.clone(), true));
                        }
                    });
                }
            });
        }
        if self.bench.composition.entries.is_empty() {
            ui.label(RichText::new("Nothing yet: add controls above.").weak());
        }
        self.act(action);
    }

    /// The list's rows, in logical order.
    fn rows(&self) -> Vec<Row> {
        let label = |control: &str, key: &str| {
            self.bench
                .registry
                .get(control)
                .map_or_else(|| key.to_owned(), |c| c.label.clone())
        };
        let mut rows = Vec::new();
        for e in &self.bench.composition.entries {
            match e {
                Entry::Control(p) => rows.push(Row::Control {
                    key: p.key.clone(),
                    label: label(&p.control, &p.key),
                    in_case: false,
                }),
                Entry::Case(c) => {
                    rows.push(Row::Case {
                        key: c.key.clone(),
                        legend: c.legend.clone(),
                    });
                    rows.extend(c.parts.iter().map(|p| Row::Control {
                        key: p.key.clone(),
                        label: label(&p.control, &p.key),
                        in_case: true,
                    }));
                }
            }
        }
        rows
    }

    fn act(&mut self, action: Option<Action>) {
        match action {
            Some(Action::Mark(key, on)) => {
                if on {
                    self.marked.insert(key);
                } else {
                    self.marked.remove(&key);
                }
            }
            Some(Action::Move(key, up)) => self.step(&key, up),
            Some(Action::Remove(key)) => {
                let _ = self.bench.remove(&key);
                self.marked.remove(&key);
            }
            Some(Action::Ungroup(case)) => {
                let _ = self.bench.composition.ungroup(&case);
            }
            None => {}
        }
    }

    /// Groups the ticked controls into a new case, under a legend.
    pub fn group_marked(&mut self) {
        let keys: Vec<String> = self.marked.iter().cloned().collect();
        let cases = self
            .bench
            .composition
            .entries
            .iter()
            .filter(|e| matches!(e, Entry::Case(_)))
            .count();
        let _ = self.bench.group(&keys, &format!("Case {}", cases + 1));
        self.marked.clear();
    }

    /// Moves `key` one step up or down the list. A control crosses a
    /// case's edge first: it joins the case it meets, or leaves its own.
    /// A case moves as a whole.
    pub fn step(&mut self, key: &str, up: bool) {
        let c = &mut self.bench.composition;
        let Some(spot) = c.find(key) else { return };
        let to = match spot.case {
            None => {
                let i = spot.index;
                let Some(j) = (if up {
                    i.checked_sub(1)
                } else {
                    (i + 1 < c.entries.len()).then_some(i + 1)
                }) else {
                    return;
                };
                match (&c.entries[i], &c.entries[j]) {
                    (Entry::Control(_), Entry::Case(n)) => {
                        Spot::in_case(&n.key, if up { n.parts.len() } else { 0 })
                    }
                    _ => Spot::top(j),
                }
            }
            Some(case) => {
                let n = c.case(&case).map_or(0, |b| b.parts.len());
                let at = c.find(&case).map_or(0, |s| s.index);
                let j = spot.index;
                if up && j > 0 {
                    Spot::in_case(&case, j - 1)
                } else if !up && j + 1 < n {
                    Spot::in_case(&case, j + 1)
                } else if up || n == 1 {
                    // Out of the case, before it (or where it was, when
                    // it goes with its last control).
                    Spot::top(at)
                } else {
                    Spot::top(at + 1)
                }
            }
        };
        let _ = c.place(key, &to);
    }

    /// The box: the controls drawn in their slots, the cases' frames, the
    /// columns behind, the handle on the right edge.
    fn canvas(&mut self, ui: &mut Ui) {
        let (placement, _) = match self.bench.resolve() {
            Ok(p) => p,
            Err(e) => {
                ui.colored_label(ui.visuals().error_fg_color, e);
                return;
            }
        };
        let height = placement
            .layout
            .as_ref()
            .map_or(0.0, CardLayout::height)
            .max(48.0);
        ui.add_space(MARGIN);
        let origin = ui.cursor().min + vec2(MARGIN, 0.0);
        let rect = Rect::from_min_size(origin, vec2(self.bench.width, height));
        // The box's background, set once the card (and its More row) is
        // drawn.
        let background = ui.painter().add(Shape::Noop);
        if self.grid {
            columns(ui, rect, &placement);
        }
        let at =
            |r: &egui_hud::card::Rect| Rect::from_min_size(origin + vec2(r.x, r.y), vec2(r.w, r.h));
        if let Some(layout) = &placement.layout {
            let accent = ui.visuals().selection.bg_fill;
            for g in &layout.groups {
                let Some(r) = &g.rect else { continue };
                let frame = at(r).expand(1.0);
                ui.painter()
                    .rect_filled(frame, 6.0, accent.gamma_multiply(0.08));
                ui.painter().rect_stroke(
                    frame,
                    6.0,
                    Stroke::new(1.0, accent.gamma_multiply(0.7)),
                    StrokeKind::Inside,
                );
            }
        }
        if self.bench.composition.entries.is_empty() {
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                "Add controls from the list on the left",
                egui::TextStyle::Body.resolve(ui.style()),
                ui.visuals().weak_text_color(),
            );
        }

        // The controls, drawn by egui_hud as a module's card is.
        let mut values: BTreeMap<String, Value> = self
            .bench
            .composition
            .keys()
            .into_iter()
            .filter_map(|k| Some((k.to_owned(), self.bench.value(k)?)))
            .collect();
        self.drawings.faces = placement
            .items
            .iter()
            .filter_map(|p| Some((p.key.clone(), p.variant.clone()?)))
            .collect();
        let mut child = ui.new_child(
            UiBuilder::new()
                .max_rect(rect)
                .id_salt("box-card")
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        let out = render::card(
            &mut child,
            &self.bench.registry,
            &placement,
            &mut values,
            &mut self.drawings,
        );
        self.drawn = out.drawn;
        self.bench.apply(out.edits);
        for e in out.events {
            match e {
                CardEvent::Reveal(key) => self.bench.reveal(&key),
                CardEvent::RevealAll => self.bench.reveal_all(),
                CardEvent::Reset(_) | CardEvent::Quad(_) => {}
            }
        }
        if let Some((key, face)) = self.drawings.picked.take() {
            self.pick_face(&key, &face);
        }

        let zone = rect.union(child.min_rect());
        let v = ui.visuals();
        ui.painter().set(
            background,
            egui::epaint::RectShape::new(
                zone.expand(2.0),
                4.0,
                v.faint_bg_color,
                Stroke::new(1.0, v.widgets.noninteractive.bg_stroke.color),
                StrokeKind::Outside,
            ),
        );
        ui.allocate_rect(
            Rect::from_min_max(zone.min, zone.max + vec2(24.0, MARGIN * 4.0)),
            Sense::hover(),
        );
        self.edge(ui, zone);
        self.zone = Some(zone);
        self.last = Some(placement);
    }

    /// The box's right edge: dragged, it sets the width.
    fn edge(&mut self, ui: &mut Ui, zone: Rect) {
        let edge = Rect::from_min_max(
            pos2(zone.right() - 3.0, zone.top()),
            pos2(zone.right() + 9.0, zone.bottom()),
        );
        let drag = ui
            .interact(edge, ui.scope_id().with("right-edge"), Sense::drag())
            .on_hover_cursor(CursorIcon::ResizeHorizontal)
            .on_hover_text("Drag to resize");
        let name = format!("Box width, {:.0} points", self.width);
        drag.widget_info(|| {
            egui::WidgetInfo::labeled(egui::accesskit::Role::Splitter, true, &name)
        });
        let colour = if drag.hovered() || drag.dragged() {
            ui.visuals().selection.bg_fill
        } else {
            ui.visuals().widgets.inactive.fg_stroke.color
        };
        ui.painter().line_segment(
            [
                pos2(zone.right() + 4.0, zone.center().y - 16.0),
                pos2(zone.right() + 4.0, zone.center().y + 16.0),
            ],
            Stroke::new(3.0, colour),
        );
        // The width follows the pointer from the drag's start: past either
        // end of the range, the edge waits for the pointer to come back.
        let x = ui.input(|i| i.pointer.interact_pos()).map(|p| p.x);
        if drag.drag_started() {
            self.resize_from = x.map(|x| (self.width, x));
        }
        if drag.dragged()
            && let (Some((width, from)), Some(x)) = (self.resize_from, x)
        {
            self.set_width(width + x - from);
        }
        if !drag.dragged() && !drag.drag_started() {
            self.resize_from = None;
        }
    }

    /// The channel strip and the lamp, driven by the controls in the box
    /// (each at its default when not in it).
    fn previews(&self, ui: &mut Ui, t: f64) {
        ui.heading("Channel strip");
        preview::paint_strip(ui, &preview::strip(&self.channel(), t));
        ui.label(
            RichText::new(
                "Low cut and EQ shape the spectrum, gain and mute the level, pan splits it; \
                 name and tag label the strip.",
            )
            .weak(),
        );
        ui.add_space(8.0);
        ui.heading("Lamp");
        preview::paint_lamp(ui, &preview::lamp(&self.light(), t));
        ui.label(
            RichText::new("Power, intensity, temperature, colour, hue and effect light the bulb.")
                .weak(),
        );
        ui.add_space(8.0);
        ui.label(RichText::new("A control not in the box stays at its default.").weak());
    }
}

/// The slot columns behind the box: every column, at the density, a
/// line; a quarter of one, a faint line.
fn columns(ui: &Ui, rect: Rect, placement: &CardPlacement) {
    let column = placement.keys.density.column();
    let base = ui.visuals().widgets.noninteractive.bg_stroke.color;
    let faint = Stroke::new(0.5, base.gamma_multiply(0.35));
    let strong = Stroke::new(1.0, base.gamma_multiply(0.9));
    let step = column / 4.0;
    let painter = ui.painter();
    let mut i = 1;
    while i as f32 * step < rect.width() - 0.5 {
        let stroke = if i % 4 == 0 { strong } else { faint };
        painter.vline(rect.left() + i as f32 * step, rect.y_range(), stroke);
        i += 1;
    }
}

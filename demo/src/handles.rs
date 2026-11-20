//! The handles preview: a spot light on a stage, edited by handles that
//! carry their own maths (`egui_hud::handle`): its centre (a point), its
//! radius around it, its feather outside the radius and its beam's angle.
//! Shift snaps (the centre to the axes, the angle to 15° steps), Ctrl
//! moves finely, the wheel scales or turns, a double click resets.
//!
//! [`Spot`] is the model, pure; [`paint`] draws it and drives the
//! handles with `render::handle`, which owns its drag here (over an
//! image, egui-pop's overlay owns it and `render::declare_handle` only
//! declares the area).

use std::collections::BTreeMap;

use egui_hud::handle::{AngleParam, Centre, Handle, HandleKind, Point, Reach};
use egui_hud::render;
use egui_pop::core::areas::AreaSpec;
use egui_pop::core::pointer::DragKind;
use egui_pop::egui::{self, Color32, Rect, Role, Sense, Stroke, StrokeKind, Ui, WidgetInfo, vec2};

/// The stage's size, in points.
pub const STAGE: egui::Vec2 = vec2(300.0, 180.0);

/// A handle's drag: which one, the values and the pointer at its start.
#[derive(Clone, Debug)]
struct Drag {
    handle: usize,
    start: BTreeMap<String, f64>,
    from: Point,
}

/// A spot light: its values and its handles.
#[derive(Clone, Debug)]
pub struct Spot {
    pub values: BTreeMap<String, f64>,
    pub handles: Vec<Handle>,
    drag: Option<Drag>,
}

impl Default for Spot {
    fn default() -> Self {
        Self::new()
    }
}

impl Spot {
    /// A spot in the middle of the stage, its beam pointing up-right.
    pub fn new() -> Self {
        let values = [
            ("x", 150.0),
            ("y", 90.0),
            ("radius", 40.0),
            ("feather", 15.0),
            ("beam", 45.0),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v))
        .collect();
        let c = Centre::default;
        let handles = vec![
            Handle::new("centre", HandleKind::point(c())).look(egui_hud::handle::Look::Move),
            Handle::new("radius", HandleKind::radius(c(), "radius")),
            Handle::new("feather", HandleKind::feather(c(), "radius", "feather")),
            Handle::new(
                "beam",
                HandleKind::angle(
                    c(),
                    AngleParam::new("beam").y_up(),
                    Reach::Param("radius".into(), 30.0),
                ),
            )
            .reset(Some(90.0)),
        ];
        Self {
            values,
            handles,
            drag: None,
        }
    }

    pub fn value(&self, name: &str) -> f64 {
        self.values.get(name).copied().unwrap_or(0.0)
    }

    /// The handle named `name`.
    pub fn handle(&self, name: &str) -> Option<&Handle> {
        self.handles.iter().find(|h| h.name == name)
    }

    /// Where the handle `i` sits on the stage.
    pub fn position(&self, i: usize) -> Point {
        self.handles[i].position(&self.values)
    }

    fn write(&mut self, w: Vec<(String, f64)>) {
        for (k, v) in w {
            // The stage keeps every size positive and the spot on it.
            let v = match k.as_str() {
                "x" => v.clamp(0.0, f64::from(STAGE.x)),
                "y" => v.clamp(0.0, f64::from(STAGE.y)),
                "radius" => v.max(4.0),
                _ => v,
            };
            self.values.insert(k, v);
        }
    }

    /// Begins dragging the handle `i` with the pointer at `from`.
    pub fn begin(&mut self, i: usize, from: Point) {
        self.drag = Some(Drag {
            handle: i,
            start: self.values.clone(),
            from,
        });
    }

    /// The pointer at `to`, with Shift (`shift`) and Ctrl (`fine`).
    pub fn update(&mut self, to: Point, shift: bool, fine: bool) {
        let Some(d) = self.drag.clone() else { return };
        let w = self.handles[d.handle].dragged(&d.start, d.from, to, shift, fine);
        self.write(w);
    }

    /// Ends the drag, kept.
    pub fn end(&mut self) {
        self.drag = None;
    }

    /// Gives the drag up: the values from its start come back.
    pub fn cancel(&mut self) {
        if let Some(d) = self.drag.take() {
            self.values = d.start;
        }
    }

    /// `delta` wheel units over the handle `i`.
    pub fn wheel(&mut self, i: usize, delta: f64) {
        if let Some(w) = self.handles[i].wheeled(&self.values, delta) {
            self.write(vec![w]);
        }
    }

    /// A double click on the handle `i`.
    pub fn reset(&mut self, i: usize) {
        if let Some(w) = self.handles[i].reset_value() {
            self.write(vec![w]);
        }
    }
}

fn on_stage(origin: egui::Pos2, p: Point) -> egui::Pos2 {
    origin + render::handle_pos(p).to_vec2()
}

/// Draws the stage and its handles, and drives them.
pub fn paint(ui: &mut Ui, spot: &mut Spot) {
    let (rect, r) = ui.allocate_exact_size(STAGE, Sense::hover());
    r.widget_info(|| WidgetInfo::labeled(Role::Image, true, "Handles preview: a spot light"));
    let origin = rect.min;
    let painter = ui.painter_at(rect);
    let v = ui.visuals().clone();
    painter.rect(
        rect,
        4.0,
        Color32::from_gray(18),
        v.widgets.noninteractive.bg_stroke,
        StrokeKind::Inside,
    );
    let c = on_stage(origin, [spot.value("x"), spot.value("y")]);
    let radius = spot.value("radius") as f32;
    let feather = spot.value("feather") as f32;
    let warm = Color32::from_rgb(255, 214, 140);
    for k in (1..=6).rev() {
        let share = k as f32 / 6.0;
        painter.circle_filled(c, radius + feather * share, warm.gamma_multiply(0.05));
    }
    painter.circle_filled(c, radius, warm.gamma_multiply(0.45));
    painter.circle_stroke(
        c,
        radius + feather,
        Stroke::new(1.0, warm.gamma_multiply(0.5)),
    );
    let beam = spot.handle("beam").map(|h| h.position(&spot.values));
    if let Some(b) = beam {
        painter.line_segment([c, on_stage(origin, b)], Stroke::new(1.5, warm));
    }
    let (shift, fine, escape) = ui.input(|i| {
        (
            i.modifiers.shift,
            i.modifiers.command || i.modifiers.ctrl,
            i.key_pressed(egui::Key::Escape),
        )
    });
    for i in 0..spot.handles.len() {
        let at = on_stage(origin, spot.position(i));
        let name = spot.handles[i].name.clone();
        let kind = spot.handles[i].kind.name();
        let writes = spot.handles[i].writes();
        let value = writes
            .first()
            .map(|w| format!("{w} {:.1}", spot.value(w)))
            .unwrap_or_default();
        let area = AreaSpec::edit(format!("demo.spot.{name}"), format!("Spot {name} ({kind})"))
            .value(value)
            .path(writes.join(", "))
            .cursor(DragKind::Move);
        let resp = render::handle(
            ui,
            Rect::from_center_size(at, egui::Vec2::splat(render::HANDLE_TARGET)),
            ui.make_persistent_id(("spot", i)),
            area,
        );
        let to = |p: egui::Pos2| -> Point {
            let d = p - origin;
            [f64::from(d.x), f64::from(d.y)]
        };
        if resp.drag_started()
            && let Some(p) = resp.interact_pointer_pos()
        {
            spot.begin(i, to(p));
        }
        if resp.dragged() {
            if escape {
                spot.cancel();
            } else if let Some(p) = resp.interact_pointer_pos() {
                spot.update(to(p), shift, fine);
            }
        }
        if resp.drag_stopped() {
            spot.end();
        }
        if resp.double_clicked() {
            spot.reset(i);
        }
        if resp.hovered() {
            let delta = ui.input(|i| i.smooth_scroll_delta.y);
            if delta != 0.0 {
                spot.wheel(i, f64::from(delta));
            }
        }
        let hot = resp.hovered() || resp.dragged();
        painter.circle(
            at,
            if hot { 6.0 } else { 5.0 },
            Color32::from_gray(240),
            Stroke::new(1.5, Color32::from_gray(40)),
        );
    }
}

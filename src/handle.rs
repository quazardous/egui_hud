//! Handles on a surface (an image), with the maths of their kind. No
//! egui.
//!
//! A [`Handle`] is a point a person drags to edit parameters: a shape's
//! centre, its radius, its feather, its angle, its band across a line.
//! Its [`HandleKind`] knows where the handle sits from the parameters
//! ([`Handle::position`]) and which values a drag to a point writes
//! ([`Handle::drag`]); the handle adds what Shift does to a drag
//! ([`Snap`], Ctrl moving it finely), what a double click resets and
//! what the wheel does ([`Wheel`]). An unusual shape (a path, a spline)
//! brings its own maths through [`HandleKind::Custom`] and the
//! [`HandleMaths`] trait.
//!
//! Points are in surface coordinates (image pixels), `x` to the right and
//! `y` down. Angles are in degrees, kept in (-180, 180] ([`wrap`]).
//! Parameters are read through [`Params`], so a host passes its own
//! parameter set; a missing parameter reads 0, a switch reads non-zero
//! when on.
//!
//! Nothing here draws or senses: the host's overlay (egui-pop's viewport
//! overlay) draws each handle with its [`Look`] and owns its drag, and
//! `render::declare_handle` registers the handle's edit area and hover
//! card without taking that drag.
//!
//! # As data
//!
//! A handle declaration is serde data, so a host can read one from a file
//! (a plugin's on-image tool, say). [`Handles`] is such a file: a
//! `format` ([`FORMAT`]) and its handles. Each handle is an object with
//! its `name`, its kind's fields tagged by `kind` (`point`, `radius`,
//! `feather`, `angle`, `line`), and the handle's own `look`, `snap`,
//! `reset` and `wheel`. Left out, those four take the kind's defaults
//! ([`Handle::new`]); `null` says "none". A kind's `centre` left out is
//! `x`, `y`; an angle's `offset` is 0 and `y-up` false; a line's
//! `both-sides` is true.
//!
//! ```json
//! {
//!   "format": "egui_hud-handles/1",
//!   "handles": [
//!     { "name": "centre", "kind": "point",
//!       "centre": { "x": "x", "y": "y" }, "look": "move" },
//!     { "name": "radius", "kind": "radius", "radius": "radius" },
//!     { "name": "feather", "kind": "feather",
//!       "radius": "radius", "feather": "feather", "reset": 25.0 },
//!     { "name": "beam", "kind": "angle",
//!       "angle": { "name": "beam", "y-up": true },
//!       "reach": { "param": "radius", "margin": 30.0 } }
//!   ]
//! }
//! ```
//!
//! A `reach` is `{ "fixed": 80.0 }` or `{ "param": "radius", "margin":
//! 30.0 }`. A `kind` this version does not know (a newer file's) reads as
//! [`HandleKind::Unknown`], kept as it was read and written back
//! unchanged, instead of failing the whole file: [`Handles::split`] sets
//! it apart with its name, so the host leaves it out and says so. A known
//! kind with a wrong field is still an error. [`HandleKind::Custom`] is
//! code: it never serialises (an error) and never reads back (`custom` is
//! an unknown kind like any other).

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

mod serial;

pub use serial::{FORMAT, Handles, Skipped, UnknownKind};

/// A point on the surface, `x` right and `y` down (image pixels).
pub type Point = [f64; 2];

/// With Shift, an angle snaps to steps of this many degrees.
pub const ANGLE_STEP: f64 = 15.0;
/// With Ctrl held, a handle moves this share of the pointer.
pub const FINE: f64 = 0.1;
/// A wheel unit scales a size or a feather by `exp(WHEEL_RATIO)`.
pub const WHEEL_RATIO: f64 = 0.002;
/// A wheel unit turns an angle by this many degrees.
pub const WHEEL_TURN: f64 = 0.1;

/// The parameters a handle reads, by name. A missing one reads `None`
/// (0 to the maths); a switch is non-zero when on.
pub trait Params {
    fn float(&self, name: &str) -> Option<f64>;
}

impl Params for BTreeMap<String, f64> {
    fn float(&self, name: &str) -> Option<f64> {
        self.get(name).copied()
    }
}

impl Params for BTreeMap<&str, f64> {
    fn float(&self, name: &str) -> Option<f64> {
        self.get(name).copied()
    }
}

impl<S: std::hash::BuildHasher> Params for HashMap<String, f64, S> {
    fn float(&self, name: &str) -> Option<f64> {
        self.get(name).copied()
    }
}

impl Params for [(&str, f64)] {
    fn float(&self, name: &str) -> Option<f64> {
        self.iter().find(|(k, _)| *k == name).map(|(_, v)| *v)
    }
}

impl<const N: usize> Params for [(&str, f64); N] {
    fn float(&self, name: &str) -> Option<f64> {
        self.as_slice().float(name)
    }
}

/// A host's parameter set read through a function.
pub struct ParamsFn<F>(pub F);

impl<F: Fn(&str) -> Option<f64>> Params for ParamsFn<F> {
    fn float(&self, name: &str) -> Option<f64> {
        (self.0)(name)
    }
}

fn get(p: &dyn Params, name: &str) -> f64 {
    p.float(name).unwrap_or(0.0)
}

fn sub(a: Point, b: Point) -> Point {
    [a[0] - b[0], a[1] - b[1]]
}

fn add(a: Point, b: Point) -> Point {
    [a[0] + b[0], a[1] + b[1]]
}

fn scale(a: Point, k: f64) -> Point {
    [a[0] * k, a[1] * k]
}

fn len(a: Point) -> f64 {
    a[0].hypot(a[1])
}

fn dot(a: Point, b: Point) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

/// A direction's angle on the surface (`y` down), in degrees.
fn degrees(d: Point) -> f64 {
    d[1].atan2(d[0]).to_degrees()
}

/// The unit vector at `deg` degrees on the surface (`y` down).
fn unit(deg: f64) -> Point {
    let r = deg.to_radians();
    [r.cos(), r.sin()]
}

/// An angle in degrees kept in (-180, 180].
pub fn wrap(deg: f64) -> f64 {
    let d = deg.rem_euclid(360.0);
    if d > 180.0 { d - 360.0 } else { d }
}

/// The two parameters of a point (a shape's centre or anchor).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Centre {
    pub x: String,
    pub y: String,
}

impl Centre {
    pub fn new(x: impl Into<String>, y: impl Into<String>) -> Self {
        Self {
            x: x.into(),
            y: y.into(),
        }
    }

    /// Where it is.
    pub fn at(&self, p: &dyn Params) -> Point {
        [get(p, &self.x), get(p, &self.y)]
    }
}

impl Default for Centre {
    /// `x` and `y`.
    fn default() -> Self {
        Self::new("x", "y")
    }
}

/// An angle parameter, as a direction on the surface.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct AngleParam {
    pub name: String,
    /// Degrees added to the parameter for the handle's direction (an
    /// ellipse's second radius sits 90° past its rotation).
    #[serde(default)]
    pub offset: f64,
    /// The parameter turns counter-clockwise on screen (mathematical
    /// angles, `y` up); otherwise clockwise, as `y` goes down.
    #[serde(default)]
    pub y_up: bool,
}

impl AngleParam {
    /// `name`, clockwise on screen, no offset.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            offset: 0.0,
            y_up: false,
        }
    }

    pub fn offset(mut self, degrees: f64) -> Self {
        self.offset = degrees;
        self
    }

    /// Counter-clockwise on screen (`y` up).
    pub fn y_up(mut self) -> Self {
        self.y_up = true;
        self
    }

    /// The handle's direction on the surface (`y` down), in degrees, for
    /// the parameter's value `v`.
    pub fn direction(&self, v: f64) -> f64 {
        let screen = v + self.offset;
        if self.y_up { -screen } else { screen }
    }

    /// The parameter's value for a direction `d` on the surface.
    pub fn value(&self, d: Point) -> f64 {
        let screen = if self.y_up {
            degrees([d[0], -d[1]])
        } else {
            degrees(d)
        };
        wrap(screen - self.offset)
    }

    fn direction_in(&self, p: &dyn Params) -> f64 {
        self.direction(get(p, &self.name))
    }
}

/// How far from the centre an angle handle sits.
#[derive(Clone, Debug, PartialEq)]
pub enum Reach {
    /// A fixed distance, in surface units.
    Fixed(f64),
    /// A parameter's value (a radius), plus a margin.
    Param(String, f64),
}

impl Reach {
    fn of(&self, p: &dyn Params) -> f64 {
        match self {
            Reach::Fixed(d) => *d,
            Reach::Param(n, margin) => get(p, n) + margin,
        }
    }
}

/// A feather in proportion to the larger of two radii, while a switch is
/// on: the distance past the radius, as a share of it, times the larger
/// radius.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Proportional {
    /// The switch (non-zero: on).
    pub switch: String,
    /// The other radius.
    pub other: String,
}

/// The maths of an unusual handle (a path's point, a spline's tangent),
/// a host's own.
pub trait HandleMaths: Send + Sync + fmt::Debug {
    /// The parameters it writes, the first the one it adjusts (the wheel
    /// and the reset).
    fn writes(&self) -> Vec<String>;
    /// Where it sits.
    fn position(&self, params: &dyn Params) -> Point;
    /// What a drag to `to` writes, from the parameters at the drag's
    /// start.
    fn drag(&self, start: &dyn Params, to: Point) -> Vec<(String, f64)>;
    /// What an angle snap turns around; its position by default.
    fn pivot(&self, params: &dyn Params) -> Point {
        self.position(params)
    }
}

/// What a handle edits, and so its maths.
#[derive(Clone, Debug)]
pub enum HandleKind {
    /// A point: the drag sets both its parameters.
    Point(Centre),
    /// A radius around a point: the drag sets the distance to the centre.
    /// With an `angle`, the handle sits in its direction; `turns`, the
    /// drag sets that angle too (an ellipse's radius turns the ellipse).
    Radius {
        centre: Centre,
        radius: String,
        angle: Option<AngleParam>,
        turns: bool,
    },
    /// A feather outside a radius: the drag sets the distance past the
    /// radius, never below 0, or in proportion to the larger radius.
    Feather {
        centre: Centre,
        radius: String,
        feather: String,
        angle: Option<AngleParam>,
        proportional: Option<Proportional>,
    },
    /// An angle around a point: the drag sets the direction to the
    /// centre.
    Angle {
        centre: Centre,
        angle: AngleParam,
        reach: Reach,
    },
    /// A distance across a line through a point at an angle (a
    /// gradient's band): the drag sets the distance to the line, twice it
    /// when `both_sides`.
    Line {
        centre: Centre,
        angle: AngleParam,
        width: String,
        both_sides: bool,
    },
    /// A host's own maths. Code: it never serialises, and a file's
    /// `custom` reads as [`HandleKind::Unknown`].
    Custom(Arc<dyn HandleMaths>),
    /// A kind this version does not know, read from a newer declaration
    /// and kept as it was read. Inert: it writes nothing and sits at the
    /// origin; a host leaves it out ([`Handles::split`]).
    Unknown(UnknownKind),
}

impl HandleKind {
    /// A point at `x`, `y`.
    pub fn point(centre: Centre) -> Self {
        HandleKind::Point(centre)
    }

    /// A radius `radius` around `centre`, the handle to its right.
    pub fn radius(centre: Centre, radius: impl Into<String>) -> Self {
        HandleKind::Radius {
            centre,
            radius: radius.into(),
            angle: None,
            turns: false,
        }
    }

    /// A feather `feather` past `radius` around `centre`.
    pub fn feather(centre: Centre, radius: impl Into<String>, feather: impl Into<String>) -> Self {
        HandleKind::Feather {
            centre,
            radius: radius.into(),
            feather: feather.into(),
            angle: None,
            proportional: None,
        }
    }

    /// An angle `angle` around `centre`, the handle `reach` away.
    pub fn angle(centre: Centre, angle: AngleParam, reach: Reach) -> Self {
        HandleKind::Angle {
            centre,
            angle,
            reach,
        }
    }

    /// A band `width` across the line through `centre` at `angle`, twice
    /// the handle's distance to the line.
    pub fn line(centre: Centre, angle: AngleParam, width: impl Into<String>) -> Self {
        HandleKind::Line {
            centre,
            angle,
            width: width.into(),
            both_sides: true,
        }
    }

    /// A host's own maths.
    pub fn custom(maths: impl HandleMaths + 'static) -> Self {
        HandleKind::Custom(Arc::new(maths))
    }

    /// The kind's name (`point`, `radius`, `feather`, `angle`, `line`,
    /// `custom`, `unknown`).
    pub fn name(&self) -> &'static str {
        match self {
            HandleKind::Point(_) => "point",
            HandleKind::Radius { .. } => "radius",
            HandleKind::Feather { .. } => "feather",
            HandleKind::Angle { .. } => "angle",
            HandleKind::Line { .. } => "line",
            HandleKind::Custom(_) => "custom",
            HandleKind::Unknown(_) => "unknown",
        }
    }

    /// The parameters it writes, the first the one it adjusts.
    pub fn writes(&self) -> Vec<String> {
        match self {
            HandleKind::Point(c) => vec![c.x.clone(), c.y.clone()],
            HandleKind::Radius {
                radius,
                angle,
                turns,
                ..
            } => {
                let mut w = vec![radius.clone()];
                if let (true, Some(a)) = (turns, angle) {
                    w.push(a.name.clone());
                }
                w
            }
            HandleKind::Feather { feather, .. } => vec![feather.clone()],
            HandleKind::Angle { angle, .. } => vec![angle.name.clone()],
            HandleKind::Line { width, .. } => vec![width.clone()],
            HandleKind::Custom(m) => m.writes(),
            HandleKind::Unknown(_) => Vec::new(),
        }
    }

    /// The point it turns around (an angle snap's pivot).
    pub fn pivot(&self, p: &dyn Params) -> Point {
        match self {
            HandleKind::Point(c)
            | HandleKind::Radius { centre: c, .. }
            | HandleKind::Feather { centre: c, .. }
            | HandleKind::Angle { centre: c, .. }
            | HandleKind::Line { centre: c, .. } => c.at(p),
            HandleKind::Custom(m) => m.pivot(p),
            HandleKind::Unknown(_) => [0.0, 0.0],
        }
    }

    /// Where the handle sits.
    pub fn position(&self, p: &dyn Params) -> Point {
        let dir = |a: Option<&AngleParam>| a.map_or(0.0, |a| a.direction_in(p));
        match self {
            HandleKind::Point(c) => c.at(p),
            HandleKind::Radius {
                centre,
                radius,
                angle,
                ..
            } => add(
                centre.at(p),
                scale(unit(dir(angle.as_ref())), get(p, radius)),
            ),
            HandleKind::Feather {
                centre,
                radius,
                feather,
                angle,
                proportional,
            } => {
                let a = get(p, radius);
                let f = get(p, feather);
                let d = match proportional {
                    Some(pr) if get(p, &pr.switch) != 0.0 && a > 0.0 => {
                        let larger = a.max(get(p, &pr.other));
                        if larger > 0.0 {
                            a * (1.0 + f / larger)
                        } else {
                            a
                        }
                    }
                    _ => a + f,
                };
                add(centre.at(p), scale(unit(dir(angle.as_ref())), d))
            }
            HandleKind::Angle {
                centre,
                angle,
                reach,
            } => add(
                centre.at(p),
                scale(unit(angle.direction_in(p)), reach.of(p)),
            ),
            HandleKind::Line {
                centre,
                angle,
                width,
                both_sides,
            } => {
                let w = get(p, width);
                let d = if *both_sides { w / 2.0 } else { w };
                add(centre.at(p), scale(normal(angle.direction_in(p)), d))
            }
            HandleKind::Custom(m) => m.position(p),
            HandleKind::Unknown(_) => [0.0, 0.0],
        }
    }

    /// What a drag to `to` writes, from the parameters at its start.
    pub fn drag(&self, start: &dyn Params, to: Point) -> Vec<(String, f64)> {
        match self {
            HandleKind::Point(c) => vec![(c.x.clone(), to[0]), (c.y.clone(), to[1])],
            HandleKind::Radius {
                centre,
                radius,
                angle,
                turns,
            } => {
                let d = sub(to, centre.at(start));
                let mut w = vec![(radius.clone(), len(d))];
                if let (true, Some(a)) = (turns, angle) {
                    w.push((a.name.clone(), a.value(d)));
                }
                w
            }
            HandleKind::Feather {
                centre,
                radius,
                feather,
                proportional,
                ..
            } => {
                let d = len(sub(to, centre.at(start)));
                let a = get(start, radius);
                let f = match proportional {
                    Some(pr) if get(start, &pr.switch) != 0.0 && a > 0.0 => {
                        (d / a - 1.0) * a.max(get(start, &pr.other))
                    }
                    _ => d - a,
                };
                vec![(feather.clone(), f.max(0.0))]
            }
            HandleKind::Angle { centre, angle, .. } => {
                vec![(angle.name.clone(), angle.value(sub(to, centre.at(start))))]
            }
            HandleKind::Line {
                centre,
                angle,
                width,
                both_sides,
            } => {
                let n = normal(angle.direction_in(start));
                let d = dot(sub(to, centre.at(start)), n).abs();
                vec![(width.clone(), if *both_sides { 2.0 * d } else { d })]
            }
            HandleKind::Custom(m) => m.drag(start, to),
            HandleKind::Unknown(_) => Vec::new(),
        }
    }
}

/// The normal of a line at `deg` on the surface: the line's direction
/// turned a quarter counter-clockwise on screen.
fn normal(deg: f64) -> Point {
    let u = unit(deg);
    [u[1], -u[0]]
}

/// What Shift does to a handle's drag.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Snap {
    /// Turns it around the handle's pivot to [`ANGLE_STEP`] steps.
    Angle,
    /// Holds its move to the axes and diagonals.
    Axes,
}

/// What the wheel over a handle does to its first parameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Wheel {
    /// Scales it by a ratio, never below 0 (a size, a feather).
    Scale,
    /// Turns it by degrees, kept in (-180, 180].
    Turn,
}

/// How a handle looks and is hit, as egui-pop's overlay draws it
/// (`render::overlay_kind` maps it).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Look {
    /// The whole shape (its body).
    Move,
    /// One point of a line or a path.
    Point,
    /// A corner of a box.
    Corner,
    /// An edge of a box.
    Edge,
    /// A size (a radius).
    Size,
    /// A rotation.
    Rotate,
    /// A feather (a soft edge).
    Feather,
    /// A Bézier control point.
    Control,
}

/// A handle: a name, a kind with its maths, how it looks, and what
/// Shift, a double click and the wheel do.
#[derive(Clone, Debug)]
pub struct Handle {
    /// As the host names it (`centre`, `radius`).
    pub name: String,
    pub kind: HandleKind,
    pub look: Look,
    pub snap: Option<Snap>,
    /// A double click puts its first parameter back to this.
    pub reset: Option<f64>,
    pub wheel: Option<Wheel>,
}

impl Handle {
    /// A handle of `kind`, with its kind's defaults: a point moves along
    /// the axes with Shift; a radius scales with the wheel, and snaps its
    /// angle with Shift when it turns; a feather and a band reset to 0
    /// and scale with the wheel; an angle snaps to steps and turns with
    /// the wheel.
    pub fn new(name: impl Into<String>, kind: HandleKind) -> Self {
        let (look, snap, reset, wheel) = match &kind {
            HandleKind::Point(_) => (Look::Point, Some(Snap::Axes), None, None),
            HandleKind::Radius { turns, angle, .. } => (
                Look::Size,
                (*turns && angle.is_some()).then_some(Snap::Angle),
                None,
                Some(Wheel::Scale),
            ),
            HandleKind::Feather { .. } | HandleKind::Line { .. } => {
                (Look::Feather, None, Some(0.0), Some(Wheel::Scale))
            }
            HandleKind::Angle { .. } => (Look::Rotate, Some(Snap::Angle), None, Some(Wheel::Turn)),
            HandleKind::Custom(_) | HandleKind::Unknown(_) => (Look::Point, None, None, None),
        };
        Self {
            name: name.into(),
            kind,
            look,
            snap,
            reset,
            wheel,
        }
    }

    pub fn look(mut self, look: Look) -> Self {
        self.look = look;
        self
    }

    pub fn snap(mut self, snap: Option<Snap>) -> Self {
        self.snap = snap;
        self
    }

    pub fn reset(mut self, to: Option<f64>) -> Self {
        self.reset = to;
        self
    }

    pub fn wheel(mut self, wheel: Option<Wheel>) -> Self {
        self.wheel = wheel;
        self
    }

    /// The parameters it writes, the first the one it adjusts.
    pub fn writes(&self) -> Vec<String> {
        self.kind.writes()
    }

    /// Where it sits.
    pub fn position(&self, params: &dyn Params) -> Point {
        self.kind.position(params)
    }

    /// What a drag to `to` writes, from the parameters at its start.
    pub fn drag(&self, start: &dyn Params, to: Point) -> Vec<(String, f64)> {
        self.kind.drag(start, to)
    }

    /// What a drag from `from` to `to` writes with the modifiers held:
    /// Ctrl (`fine`) moves a tenth of the pointer, Shift (`shift`) holds
    /// the move as the handle's [`Snap`] says.
    pub fn dragged(
        &self,
        start: &dyn Params,
        from: Point,
        to: Point,
        shift: bool,
        fine: bool,
    ) -> Vec<(String, f64)> {
        let to = snapped(self.snap, self.kind.pivot(start), from, to, shift, fine);
        self.drag(start, to)
    }

    /// What `delta` wheel units over it write: its first parameter,
    /// scaled or turned. `None` without a wheel.
    pub fn wheeled(&self, params: &dyn Params, delta: f64) -> Option<(String, f64)> {
        let wheel = self.wheel?;
        let p = self.writes().into_iter().next()?;
        let v = get(params, &p);
        Some(match wheel {
            Wheel::Scale => (p, (v * (delta * WHEEL_RATIO).exp()).max(0.0)),
            Wheel::Turn => (p, wrap(v + delta * WHEEL_TURN)),
        })
    }

    /// What a double click writes: its first parameter back to its reset
    /// value. `None` without one.
    pub fn reset_value(&self) -> Option<(String, f64)> {
        let to = self.reset?;
        Some((self.writes().into_iter().next()?, to))
    }
}

/// Where a handle dragged from `start` to `to` goes: Ctrl (`fine`) moves
/// it a tenth of the pointer, then Shift (`shift`) holds it as `rule`
/// says (an angle turns around `pivot`).
pub fn snapped(
    rule: Option<Snap>,
    pivot: Point,
    start: Point,
    to: Point,
    shift: bool,
    fine: bool,
) -> Point {
    let mut to = to;
    if fine {
        to = add(start, scale(sub(to, start), FINE));
    }
    if shift {
        to = match rule {
            Some(Snap::Angle) => {
                let d = sub(to, pivot);
                let a = (degrees(d) / ANGLE_STEP).round() * ANGLE_STEP;
                add(pivot, scale(unit(a), len(d)))
            }
            Some(Snap::Axes) => {
                let d = sub(to, start);
                let u = unit((degrees(d) / 45.0).round() * 45.0);
                add(start, scale(u, dot(d, u)))
            }
            None => to,
        };
    }
    to
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    fn near_point(a: Point, b: Point) -> bool {
        near(a[0], b[0]) && near(a[1], b[1])
    }

    /// The written values applied over `params`.
    fn applied(params: &BTreeMap<String, f64>, w: &[(String, f64)]) -> BTreeMap<String, f64> {
        let mut p = params.clone();
        for (k, v) in w {
            p.insert(k.clone(), *v);
        }
        p
    }

    fn params(pairs: &[(&str, f64)]) -> BTreeMap<String, f64> {
        pairs.iter().map(|(k, v)| ((*k).to_owned(), *v)).collect()
    }

    fn pairs(w: &[(String, f64)]) -> Vec<(&str, f64)> {
        w.iter().map(|(k, v)| (k.as_str(), *v)).collect()
    }

    #[test]
    fn a_point_writes_where_it_is_dragged_and_sits_there() {
        let h = Handle::new("centre", HandleKind::point(Centre::default()));
        let p = params(&[("x", 100.0), ("y", 50.0)]);
        assert_eq!(h.position(&p), [100.0, 50.0]);
        let w = h.drag(&p, [3.0, 4.0]);
        assert_eq!(pairs(&w), [("x", 3.0), ("y", 4.0)]);
        assert_eq!(h.position(&applied(&p, &w)), [3.0, 4.0]);
        assert_eq!(h.snap, Some(Snap::Axes));
        assert!(h.wheeled(&p, 1.0).is_none());
    }

    #[test]
    fn a_radius_is_the_distance_to_its_centre() {
        let h = Handle::new("radius", HandleKind::radius(Centre::default(), "radius"));
        let p = params(&[("x", 100.0), ("y", 50.0), ("radius", 20.0)]);
        assert_eq!(h.position(&p), [120.0, 50.0]);
        let w = h.drag(&p, [130.0, 90.0]);
        assert_eq!(pairs(&w), [("radius", 50.0)]);
        // Round trip: dragged to its own position, it writes what it had.
        assert_eq!(pairs(&h.drag(&p, h.position(&p))), [("radius", 20.0)]);
    }

    #[test]
    fn a_turning_radius_sets_its_angle_and_sits_in_its_direction() {
        let rx = Handle::new(
            "radius_x",
            HandleKind::Radius {
                centre: Centre::default(),
                radius: "radius_x".into(),
                angle: Some(AngleParam::new("rotation")),
                turns: true,
            },
        );
        let ry = Handle::new(
            "radius_y",
            HandleKind::Radius {
                centre: Centre::default(),
                radius: "radius_y".into(),
                angle: Some(AngleParam::new("rotation").offset(90.0)),
                turns: true,
            },
        );
        let p = params(&[
            ("x", 0.0),
            ("y", 0.0),
            ("radius_x", 40.0),
            ("radius_y", 20.0),
        ]);
        assert_eq!(
            pairs(&rx.drag(&p, [0.0, 30.0])),
            [("radius_x", 30.0), ("rotation", 90.0)]
        );
        assert_eq!(
            pairs(&ry.drag(&p, [0.0, 30.0])),
            [("radius_y", 30.0), ("rotation", 0.0)]
        );
        assert_eq!(rx.snap, Some(Snap::Angle));
        // Round trip at a turned ellipse.
        let p = params(&[
            ("x", 5.0),
            ("y", 7.0),
            ("radius_x", 40.0),
            ("radius_y", 20.0),
            ("rotation", 30.0),
        ]);
        for h in [&rx, &ry] {
            let w = h.drag(&p, h.position(&p));
            let back = applied(&p, &w);
            for (k, v) in &p {
                assert!(near(back[k], *v), "{}: {k} {} != {v}", h.name, back[k]);
            }
        }
    }

    #[test]
    fn a_feather_is_the_distance_past_its_radius() {
        let h = Handle::new(
            "feather",
            HandleKind::feather(Centre::default(), "radius", "feather"),
        );
        let p = params(&[
            ("x", 100.0),
            ("y", 50.0),
            ("radius", 20.0),
            ("feather", 5.0),
        ]);
        assert_eq!(h.position(&p), [125.0, 50.0]);
        assert_eq!(pairs(&h.drag(&p, [70.0, 10.0])), [("feather", 30.0)]);
        // Inside the radius: no feather.
        assert_eq!(pairs(&h.drag(&p, [101.0, 50.0])), [("feather", 0.0)]);
        assert_eq!(h.reset_value(), Some(("feather".into(), 0.0)));
    }

    #[test]
    fn a_proportional_feather_follows_the_larger_radius() {
        let h = Handle::new(
            "feather",
            HandleKind::Feather {
                centre: Centre::default(),
                radius: "radius_x".into(),
                feather: "feather".into(),
                angle: None,
                proportional: Some(Proportional {
                    switch: "proportional".into(),
                    other: "radius_y".into(),
                }),
            },
        );
        let mut p = params(&[
            ("x", 0.0),
            ("y", 0.0),
            ("radius_x", 40.0),
            ("radius_y", 20.0),
        ]);
        assert_eq!(pairs(&h.drag(&p, [-50.0, 0.0])), [("feather", 10.0)]);
        p.insert("proportional".into(), 1.0);
        p.insert("radius_y".into(), 80.0);
        // 60 from the centre: half the radius past it, half of 80.
        assert_eq!(pairs(&h.drag(&p, [-60.0, 0.0])), [("feather", 40.0)]);
        let p = applied(&p, &[("feather".into(), 40.0)]);
        assert!(near_point(h.position(&p), [60.0, 0.0]));
    }

    #[test]
    fn an_angle_turns_around_its_centre() {
        let h = Handle::new(
            "rotation",
            HandleKind::angle(
                Centre::default(),
                AngleParam::new("rotation").y_up(),
                Reach::Fixed(50.0),
            ),
        )
        .reset(Some(0.0));
        let p = params(&[("x", 0.0), ("y", 0.0), ("rotation", 0.0)]);
        // Straight up on screen (y down) is 90° counter-clockwise.
        assert_eq!(pairs(&h.drag(&p, [0.0, -100.0])), [("rotation", 90.0)]);
        let p = params(&[("x", 10.0), ("y", 10.0), ("rotation", 30.0)]);
        let w = h.drag(&p, h.position(&p));
        assert!(near(w[0].1, 30.0), "{w:?}");
        assert_eq!(h.snap, Some(Snap::Angle));
        assert_eq!(h.wheel, Some(Wheel::Turn));
    }

    #[test]
    fn a_line_s_band_is_twice_the_distance_across_it() {
        let h = Handle::new(
            "feather",
            HandleKind::line(
                Centre::default(),
                AngleParam::new("rotation").y_up(),
                "feather",
            ),
        );
        let p = params(&[("x", 0.0), ("y", 0.0), ("rotation", 0.0), ("feather", 10.0)]);
        // At 0° the band is across: twice the distance from the line.
        assert_eq!(pairs(&h.drag(&p, [40.0, -15.0])), [("feather", 30.0)]);
        // The handle sits half the band away, up the screen.
        assert!(near_point(h.position(&p), [0.0, -5.0]));
        let p = params(&[
            ("x", 3.0),
            ("y", 4.0),
            ("rotation", 60.0),
            ("feather", 24.0),
        ]);
        let w = h.drag(&p, h.position(&p));
        assert!(near(w[0].1, 24.0), "{w:?}");
    }

    #[derive(Debug)]
    struct PathPoint(usize);

    impl HandleMaths for PathPoint {
        fn writes(&self) -> Vec<String> {
            vec![format!("p{}.x", self.0), format!("p{}.y", self.0)]
        }
        fn position(&self, p: &dyn Params) -> Point {
            let w = self.writes();
            [get(p, &w[0]), get(p, &w[1])]
        }
        fn drag(&self, _: &dyn Params, to: Point) -> Vec<(String, f64)> {
            self.writes().into_iter().zip(to).collect()
        }
    }

    #[test]
    fn an_unusual_shape_brings_its_own_maths() {
        let h = Handle::new("p1", HandleKind::custom(PathPoint(1)));
        assert_eq!(h.kind.name(), "custom");
        let p = [("p1.x", 4.0), ("p1.y", 2.0)];
        assert_eq!(h.position(&p), [4.0, 2.0]);
        assert_eq!(h.writes(), ["p1.x", "p1.y"]);
        assert_eq!(
            pairs(&h.drag(&p, [1.0, 2.0])),
            [("p1.x", 1.0), ("p1.y", 2.0)]
        );
        let f = ParamsFn(|n: &str| (n == "p1.x").then_some(9.0));
        assert_eq!(h.position(&f), [9.0, 0.0]);
    }

    #[test]
    fn shift_snaps_and_ctrl_moves_finely() {
        let a = snapped(
            Some(Snap::Angle),
            [0.0, 0.0],
            [10.0, 0.0],
            [10.0, 3.0],
            true,
            false,
        );
        let deg = degrees(a);
        assert!(near(deg / 15.0, (deg / 15.0).round()), "{deg}");
        let ax = snapped(
            Some(Snap::Axes),
            [0.0, 0.0],
            [0.0, 0.0],
            [60.0, 8.0],
            true,
            false,
        );
        assert!(near_point(ax, [60.0, 0.0]), "{ax:?}");
        let fine = snapped(None, [0.0, 0.0], [0.0, 0.0], [100.0, 0.0], false, true);
        assert_eq!(fine, [10.0, 0.0]);
        // Through a handle: the radius' angle snaps around its centre.
        let h = Handle::new(
            "radius_x",
            HandleKind::Radius {
                centre: Centre::default(),
                radius: "r".into(),
                angle: Some(AngleParam::new("a")),
                turns: true,
            },
        );
        let p = params(&[("x", 0.0), ("y", 0.0)]);
        let w = h.dragged(&p, [10.0, 0.0], [10.0, 3.0], true, false);
        assert!(near(w[1].1, 15.0), "{w:?}");
    }

    #[test]
    fn the_wheel_scales_sizes_and_turns_angles() {
        let angle = Handle::new(
            "rotation",
            HandleKind::angle(
                Centre::default(),
                AngleParam::new("rotation"),
                Reach::Fixed(1.0),
            ),
        );
        let band = Handle::new(
            "feather",
            HandleKind::line(Centre::default(), AngleParam::new("rotation"), "feather"),
        );
        let p = params(&[("rotation", 179.0), ("feather", 10.0)]);
        let (_, r) = angle.wheeled(&p, 20.0).unwrap();
        assert!(near(r, -179.0), "{r}");
        let (_, f) = band.wheeled(&p, 100.0).unwrap();
        assert!(near(f, 10.0 * 0.2_f64.exp()));
        assert!(angle.reset_value().is_none());
    }
}

//! Where HUD cards sit around and over a surface (an image, a canvas):
//! egui-pop-fit's slot regions. A card says where it goes ([`Place`]:
//! an anchor, over or around the surface, its priority); [`Hud::place`]
//! solves every card at once and keeps the last layout, so a card does
//! not jump between frames. Neither `egui_hud` nor its host computes a
//! rectangle: the regions do, and the host draws each card at its
//! [`HudLayout::rect`] (`render::hud_rect` turns it into egui's).
//!
//! When the room runs out, the lowest-priority cards fold: they are
//! listed by [`HudLayout::folded`], for a host to show under a "More"
//! chip or elsewhere. Cards inside stay on the card layout
//! ([`crate::card`]).
//!
//! No egui: a surface is a [`PointRect`] in points.

use std::collections::BTreeMap;

use egui_pop_fit as fit;
use serde::{Deserialize, Serialize};

pub use fit::{Anchor, PointRect, Side};

/// The gap between a region's cards, and from the surface's edges, in
/// points, unless a place sets its own.
pub const GAP: f32 = 6.0;

/// The slot engine's unit for HUD regions, in points: cards take whole
/// twelfths of it (their rectangles keep their exact size).
pub const UNIT: f32 = 8.0;

/// Where a HUD card goes on its surface.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Place {
    /// Its corner, edge, the centre, or the whole surface (`overlay`).
    pub anchor: Anchor,
    /// Over the surface (default) or around it, in the margin up to the
    /// bounds.
    #[serde(default)]
    pub side: Side,
    /// When room runs out, cards with a lower priority fold first.
    #[serde(default)]
    pub priority: i32,
}

impl Place {
    /// Over the surface at `anchor`, priority 0.
    pub fn at(anchor: Anchor) -> Self {
        Self {
            anchor,
            side: Side::Over,
            priority: 0,
        }
    }

    /// The same place, around the surface instead of over it.
    pub fn around(mut self) -> Self {
        self.side = Side::Around;
        self
    }

    /// The same place with a priority.
    pub fn priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }
}

/// A card to place: its key, its place and its size in points (a
/// resolved card's `layout.rack.width` and `layout.height()`).
#[derive(Clone, Debug, PartialEq)]
pub struct HudCard {
    pub key: String,
    pub place: Place,
    pub width: f32,
    pub height: f32,
}

impl HudCard {
    pub fn new(key: impl Into<String>, place: Place, width: f32, height: f32) -> Self {
        Self {
            key: key.into(),
            place,
            width,
            height,
        }
    }
}

/// The surface the cards sit on and around.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HudSurface {
    /// The surface (an image), in points.
    pub rect: PointRect,
    /// The room around it the `around` places may use; the surface itself
    /// when `None`.
    pub bounds: Option<PointRect>,
    /// Whether start is on the right (a right-to-left interface).
    pub rtl: bool,
    /// The gap, in points ([`GAP`] by default).
    pub gap: f32,
    /// The slot engine's unit, in points ([`UNIT`] by default).
    pub unit: f32,
}

impl HudSurface {
    pub fn new(rect: PointRect) -> Self {
        Self {
            rect,
            bounds: None,
            rtl: false,
            gap: GAP,
            unit: UNIT,
        }
    }

    /// The same surface within `bounds`.
    pub fn within(mut self, bounds: PointRect) -> Self {
        self.bounds = Some(bounds);
        self
    }

    /// The same surface, start on the right when `rtl`.
    pub fn right_to_left(mut self, rtl: bool) -> Self {
        self.rtl = rtl;
        self
    }
}

/// The cards placed: each one's rectangle, or folded.
#[derive(Clone, Debug, PartialEq)]
pub struct HudLayout {
    /// Each card's rectangle when placed, `None` when folded, by key.
    pub rects: BTreeMap<String, Option<PointRect>>,
    /// The folded cards' keys, in the order given.
    pub folded: Vec<String>,
    /// The regions' layout, as egui-pop-fit gave it.
    pub regions: fit::RegionLayout,
}

impl HudLayout {
    /// Where card `key` goes; `None` when it is folded or unknown.
    pub fn rect(&self, key: &str) -> Option<PointRect> {
        self.rects.get(key).copied().flatten()
    }

    /// The folded cards' keys, in the order given.
    pub fn folded(&self) -> &[String] {
        &self.folded
    }
}

/// The HUD of one surface: places its cards each frame, keeping the
/// last layout for hysteresis.
#[derive(Clone, Debug, Default)]
pub struct Hud {
    previous: Option<fit::RegionLayout>,
}

impl Hud {
    pub fn new() -> Self {
        Self::default()
    }

    /// Places `cards` on `surface`: one region per anchor and side, solved
    /// by egui-pop-fit's slot regions. Cards keep their order within a
    /// region; a region's priority is its highest card's.
    pub fn place(&mut self, surface: &HudSurface, cards: &[HudCard]) -> HudLayout {
        let layout = place(surface, cards, self.previous.as_ref());
        self.previous = Some(layout.regions.clone());
        layout
    }
}

/// [`Hud::place`] without memory: `previous` is the last frame's regions,
/// if any.
pub fn place(
    surface: &HudSurface,
    cards: &[HudCard],
    previous: Option<&fit::RegionLayout>,
) -> HudLayout {
    let mut regions: Vec<fit::Region> = Vec::new();
    let mut fit_cards = Vec::with_capacity(cards.len());
    for (i, c) in cards.iter().enumerate() {
        let key = region_key(c.place.anchor, c.place.side);
        match regions.iter_mut().find(|r| r.key == key) {
            Some(r) => r.priority = r.priority.max(c.place.priority),
            None => {
                let mut r = fit::Region::new(key, c.place.anchor)
                    .with_gap(surface.gap)
                    .with_priority(c.place.priority);
                if c.place.side == Side::Around {
                    r = r.around();
                }
                regions.push(r);
            }
        }
        fit_cards.push(
            fit::Card::sized(i as u64, key, c.width.max(0.0), c.height.max(0.0))
                .with_priority(c.place.priority),
        );
    }
    let mut s = fit::Surface::new(surface.rect, surface.unit).right_to_left(surface.rtl);
    if let Some(b) = surface.bounds {
        s = s.within(b);
    }
    let solved = fit::layout_regions(&s, &regions, &fit_cards, previous);
    let mut rects = BTreeMap::new();
    let mut folded = Vec::new();
    for (i, c) in cards.iter().enumerate() {
        let rect = solved.card(i as u64).and_then(|p| p.rect);
        if rect.is_none() {
            folded.push(c.key.clone());
        }
        rects.insert(c.key.clone(), rect);
    }
    HudLayout {
        rects,
        folded,
        regions: solved,
    }
}

/// A region's key: its anchor and side.
fn region_key(anchor: Anchor, side: Side) -> u64 {
    let a = Anchor::ALL.iter().position(|x| *x == anchor).unwrap_or(0) as u64;
    a * 2 + u64::from(side == Side::Around)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f32, y: f32, w: f32, h: f32) -> PointRect {
        PointRect { x, y, w, h }
    }

    fn inside(r: PointRect, of: PointRect) -> bool {
        r.x >= of.x - 0.01
            && r.y >= of.y - 0.01
            && r.x + r.w <= of.x + of.w + 0.01
            && r.y + r.h <= of.y + of.h + 0.01
    }

    fn overlap(a: PointRect, b: PointRect) -> bool {
        a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
    }

    #[test]
    fn cards_go_to_their_corners_inside_the_surface_without_overlapping() {
        let surface = HudSurface::new(rect(0.0, 0.0, 800.0, 600.0));
        let cards = [
            HudCard::new("exposure", Place::at(Anchor::TopStart), 200.0, 40.0),
            HudCard::new("levels", Place::at(Anchor::TopStart), 200.0, 80.0),
            HudCard::new("histogram", Place::at(Anchor::TopEnd), 240.0, 120.0),
        ];
        let l = Hud::new().place(&surface, &cards);
        assert!(l.folded().is_empty(), "{:?}", l.folded());
        let e = l.rect("exposure").unwrap();
        let h = l.rect("histogram").unwrap();
        assert!(e.x < 400.0 && e.y < 300.0, "top start: {e:?}");
        assert!(h.x > 400.0 && h.y < 300.0, "top end: {h:?}");
        let all: Vec<_> = cards.iter().map(|c| l.rect(&c.key).unwrap()).collect();
        for (i, a) in all.iter().enumerate() {
            assert!(inside(*a, surface.rect));
            assert_eq!((a.w, a.h), (cards[i].width, cards[i].height));
            for b in &all[i + 1..] {
                assert!(!overlap(*a, *b), "{a:?} over {b:?}");
            }
        }
    }

    #[test]
    fn right_to_left_mirrors_start_and_end() {
        let card = [HudCard::new("a", Place::at(Anchor::TopStart), 100.0, 30.0)];
        let ltr = place(&HudSurface::new(rect(0.0, 0.0, 600.0, 400.0)), &card, None);
        let rtl = place(
            &HudSurface::new(rect(0.0, 0.0, 600.0, 400.0)).right_to_left(true),
            &card,
            None,
        );
        assert!(ltr.rect("a").unwrap().x < 300.0);
        assert!(rtl.rect("a").unwrap().x > 300.0);
    }

    #[test]
    fn the_lowest_priority_card_folds_when_the_surface_is_too_small() {
        let surface = HudSurface::new(rect(0.0, 0.0, 300.0, 100.0));
        let cards = [
            HudCard::new("main", Place::at(Anchor::TopStart).priority(2), 180.0, 60.0),
            HudCard::new("extra", Place::at(Anchor::TopStart), 180.0, 60.0),
        ];
        let l = Hud::new().place(&surface, &cards);
        assert!(l.rect("main").is_some());
        assert_eq!(l.folded(), ["extra".to_owned()]);
        assert_eq!(l.rect("extra"), None);
    }

    #[test]
    fn around_places_sit_outside_the_surface_within_the_bounds() {
        let image = rect(100.0, 0.0, 600.0, 400.0);
        let surface = HudSurface::new(image).within(rect(0.0, 0.0, 800.0, 400.0));
        let cards = [HudCard::new(
            "tools",
            Place::at(Anchor::Start).around(),
            80.0,
            200.0,
        )];
        let l = place(&surface, &cards, None);
        let r = l.rect("tools").unwrap();
        assert!(r.x + r.w <= image.x + 0.01, "left of the image: {r:?}");
    }

    #[test]
    fn a_place_reads_from_json() {
        let p: Place = serde_json::from_str(r#"{"anchor": "TopEnd"}"#).unwrap();
        assert_eq!(p, Place::at(Anchor::TopEnd));
    }
}

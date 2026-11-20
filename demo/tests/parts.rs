//! The managed parts a host no longer draws itself, headless: a curve
//! and a colour wheel on egui-pop's editors, a long list folded into a
//! summary that opens its values under the card, read-out lines under
//! the card, a choice whose entries show a preview, and the error line
//! of a card that could not be resolved.

use std::collections::BTreeMap;

use egui_hud::card::{Card, CardItem};
use egui_hud::context::CardBox;
use egui_hud::model::{Control, Surface, ValueSpec, ValueType};
use egui_hud::render::{self, Drawings, Drawn, Edit, Line, NoDrawings, Preview};
use egui_hud::resolve::{Layouts, Moment, Values, resolve};
use egui_hud::{ContextKeys, Registry, Value};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop::core::icon::Icon;
use egui_pop::egui::{self, Ui};

fn other(id: &str, label: &str, stored: ValueType, face: &str) -> Control {
    Control::new(id, label, ValueSpec::Other { stored })
        .purpose("A part under test")
        .face(face)
        .with_faces()
}

fn registry() -> Registry {
    let mut beam = Control::new(
        "beam",
        "Beam",
        ValueSpec::OneOf {
            choices: vec!["Spot".into(), "Flood".into()],
            default: "Spot".into(),
        },
    )
    .face("preview-menu")
    .with_faces();
    beam.purpose = "The beam's shape".into();
    Registry::with(vec![
        other("curve", "Tone curve", ValueType::Points, "curve"),
        other("wheel", "Shadows", ValueType::Colour, "colour-wheel"),
        other("list", "Points", ValueType::Points, "folded-list"),
        other("ports", "Opacities", ValueType::Points, "readout-lines").indicator(),
        beam,
    ])
    .unwrap()
}

/// The host: its words for the lines and the previews, the edits it
/// was handed.
#[derive(Default)]
struct Host {
    edits: Vec<Edit>,
    drawn: BTreeMap<String, Drawn>,
}

impl Drawings for Host {
    fn draw(
        &mut self,
        _: &mut Ui,
        _: &str,
        _: &Control,
        _: egui::Rect,
        _: &mut Value,
    ) -> Option<Vec<Edit>> {
        None
    }

    fn lines(&mut self, _: &egui::Context, key: &str, _: &Control, _: &Value) -> Option<Vec<Line>> {
        (key == "ports").then(|| {
            vec![
                Line::new("Circle: 80 %").hint("Port 1 of the group"),
                Line::new("Brush: 100 %").hint("Port 2 of the group"),
            ]
        })
    }

    fn preview(
        &mut self,
        _: &egui::Context,
        _: &str,
        _: &Control,
        choice: &str,
    ) -> Option<Preview> {
        Some(Preview::icon(Icon::Circle).hint(format!("{choice} preview")))
    }
}

struct State {
    registry: Registry,
    values: BTreeMap<String, Value>,
    host: Host,
}

fn harness() -> Harness<'static, State> {
    let registry = registry();
    let mut values = BTreeMap::new();
    values.insert(
        "curve".into(),
        Value::Floats(vec![0.0, 0.0, 0.5, 0.6, 1.0, 1.0]),
    );
    values.insert("wheel".into(), Value::Floats(vec![0.25, 0.3, 0.0]));
    values.insert("list".into(), Value::Floats(vec![0.1, 0.2, 0.3]));
    values.insert("ports".into(), Value::Floats(vec![0.8, 1.0]));
    let state = State {
        registry,
        values,
        host: Host::default(),
    };
    Harness::builder()
        .with_size(egui::vec2(480.0, 900.0))
        .build_ui_state(
            |ui, s: &mut State| {
                let card = Card::new(vec![
                    CardItem::control("curve", "curve"),
                    CardItem::control("wheel", "wheel"),
                    CardItem::control("list", "list"),
                    CardItem::control("ports", "ports"),
                    CardItem::control("beam", "beam"),
                ]);
                let placement = resolve(
                    &s.registry,
                    &card,
                    &ContextKeys::new(CardBox::new(Surface::Panel, 48, 480)),
                    &Layouts::default(),
                    &Values::new(),
                    &Moment::default(),
                    None,
                )
                .unwrap();
                let out = render::card(ui, &s.registry, &placement, &mut s.values, &mut s.host);
                s.host.edits.extend(out.edits);
                s.host.drawn = out.drawn;
                render::error_line(ui, "No room for the card");
            },
            state,
        )
}

#[test]
fn the_curve_and_the_wheel_are_egui_pops_editors_managed() {
    let mut h = harness();
    h.run_steps(3);
    let drawn = &h.state().host.drawn;
    for key in ["curve", "wheel", "list", "ports", "beam"] {
        assert_eq!(drawn.get(key), Some(&Drawn::Managed), "{key}: {drawn:?}");
    }
    // Each editor answers by its label, its label shown above it.
    assert!(h.get_all_by_label("Tone curve").count() >= 2);
    assert!(h.get_all_by_label("Shadows").count() >= 2);
    // No numbered rows of points, no swatch and channels.
    assert!(h.query_by_label("Tone curve 1 x").is_none());
    assert!(h.query_by_label("Shadows Hue").is_none());
}

#[test]
fn a_folded_list_opens_its_values_under_the_card() {
    let mut h = harness();
    h.run_steps(3);
    assert!(h.query_by_label("1: 0.100  0.200  0.300").is_none());
    h.get_by_label("Points: 3 values").click_accesskit();
    h.run_steps(3);
    let line = h.get_by_label("1: 0.100  0.200  0.300").rect();
    let summary = h.get_by_label("Points: 3 values").rect();
    assert!(line.top() > summary.bottom(), "{line:?} under {summary:?}");
    h.get_by_label("Points: 3 values").click_accesskit();
    h.run_steps(3);
    assert!(h.query_by_label("1: 0.100  0.200  0.300").is_none());
}

#[test]
fn read_out_lines_show_under_the_card_in_the_hosts_words() {
    let mut h = harness();
    h.run_steps(3);
    let first = h.get_by_label("Circle: 80 %").rect();
    let second = h.get_by_label("Brush: 100 %").rect();
    assert!(second.top() >= first.bottom(), "one line each");
    assert!(h.state().host.edits.is_empty(), "read only");
}

#[test]
fn a_choice_shows_its_preview_before_its_menu_and_in_it() {
    let mut h = harness();
    h.run_steps(3);
    let _ = h.get_by_label("Spot preview");
    assert!(h.query_by_label("Flood preview").is_none());
    h.get_by_role(egui::accesskit::Role::ComboBox)
        .click_accesskit();
    h.run_steps(3);
    assert!(h.get_all_by_label("Spot preview").count() >= 2);
    let _ = h.get_by_label("Flood preview");
}

#[test]
fn the_error_line_says_what_went_wrong() {
    let mut h = harness();
    h.run_steps(2);
    let _ = h.get_by_label("No room for the card");
}

#[test]
fn no_drawings_still_draws_every_part() {
    let registry = registry();
    let mut h = Harness::builder()
        .with_size(egui::vec2(480.0, 900.0))
        .build_ui(move |ui| {
            let card = Card::new(vec![
                CardItem::control("list", "list"),
                CardItem::control("ports", "ports"),
            ]);
            let placement = resolve(
                &registry,
                &card,
                &ContextKeys::new(CardBox::new(Surface::Panel, 48, 480)),
                &Layouts::default(),
                &Values::new(),
                &Moment::default(),
                None,
            )
            .unwrap();
            let mut values = BTreeMap::new();
            values.insert("ports".to_owned(), Value::Floats(vec![0.5]));
            render::card(ui, &registry, &placement, &mut values, &mut NoDrawings);
        });
    h.run_steps(2);
    // The list's own words, the lines numbered.
    let _ = h.get_by_label("Points: 0 values");
    let _ = h.get_by_label("1: 0.500");
}

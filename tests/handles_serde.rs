//! Handle declarations as data: every built-in kind round-trips, an
//! unknown kind degrades to a handle left out, a custom kind never
//! crosses, and a radial exposure tool's handles read from a file match
//! the ones built in Rust.

use std::collections::BTreeMap;

use egui_hud::handle::{
    AngleParam, Centre, FORMAT, Handle, HandleKind, HandleMaths, Handles, Look, Params, Point,
    Proportional, Reach, Skipped, Snap, Wheel,
};
use serde_json::{Value as Json, json};

fn params(pairs: &[(&str, f64)]) -> BTreeMap<String, f64> {
    pairs.iter().map(|(k, v)| ((*k).to_owned(), *v)).collect()
}

fn as_json(h: &Handle) -> Json {
    serde_json::to_value(h).unwrap()
}

/// One handle of every built-in kind, with their options set away from
/// the defaults where a kind has any.
fn every_kind() -> Vec<Handle> {
    let c = || Centre::new("cx", "cy");
    vec![
        Handle::new("centre", HandleKind::point(c())).look(Look::Move),
        Handle::new("radius", HandleKind::radius(Centre::default(), "radius")),
        Handle::new(
            "radius_y",
            HandleKind::Radius {
                centre: c(),
                radius: "radius_y".into(),
                angle: Some(AngleParam::new("rotation").offset(90.0).y_up()),
                turns: true,
            },
        ),
        Handle::new("feather", HandleKind::feather(c(), "radius", "feather")).reset(Some(25.0)),
        Handle::new(
            "feather_p",
            HandleKind::Feather {
                centre: c(),
                radius: "radius_x".into(),
                feather: "feather".into(),
                angle: Some(AngleParam::new("rotation")),
                proportional: Some(Proportional {
                    switch: "proportional".into(),
                    other: "radius_y".into(),
                }),
            },
        )
        .wheel(None),
        Handle::new(
            "beam",
            HandleKind::angle(
                c(),
                AngleParam::new("beam").y_up(),
                Reach::Param("radius".into(), 30.0),
            ),
        ),
        Handle::new(
            "rotation",
            HandleKind::angle(c(), AngleParam::new("rotation"), Reach::Fixed(80.0)),
        )
        .snap(None),
        Handle::new(
            "band",
            HandleKind::line(c(), AngleParam::new("angle"), "width"),
        ),
        Handle::new(
            "edge",
            HandleKind::Line {
                centre: c(),
                angle: AngleParam::new("angle").offset(-45.0),
                width: "width".into(),
                both_sides: false,
            },
        )
        .look(Look::Edge)
        .snap(Some(Snap::Axes)),
    ]
}

#[test]
fn every_built_in_kind_round_trips() {
    let p = params(&[
        ("cx", 10.0),
        ("cy", -5.0),
        ("x", 3.0),
        ("y", 4.0),
        ("radius", 20.0),
        ("radius_x", 40.0),
        ("radius_y", 15.0),
        ("feather", 6.0),
        ("proportional", 1.0),
        ("rotation", 30.0),
        ("beam", -60.0),
        ("angle", 12.0),
        ("width", 18.0),
    ]);
    let built = every_kind();
    let kinds: Vec<_> = built.iter().map(|h| h.kind.name()).collect();
    for k in HandleKind::KINDS {
        assert!(kinds.contains(&k), "no `{k}` handle in the round trip");
    }
    let text = Handles::new(built.clone()).to_json().unwrap();
    let (read, skipped) = Handles::from_json(&text).unwrap().split();
    assert!(skipped.is_empty());
    assert_eq!(read.len(), built.len());
    for (a, b) in built.iter().zip(&read) {
        assert_eq!(as_json(a), as_json(b), "{}", a.name);
        assert_eq!(a.kind.name(), b.kind.name());
        assert_eq!(
            (a.look, a.snap, a.reset, a.wheel),
            (b.look, b.snap, b.reset, b.wheel)
        );
        assert_eq!(a.writes(), b.writes());
        assert_eq!(a.position(&p), b.position(&p), "{}", a.name);
        assert_eq!(a.drag(&p, [50.0, 70.0]), b.drag(&p, [50.0, 70.0]));
    }
}

#[test]
fn the_shape_is_tagged_by_kind_with_readable_fields() {
    let h = Handle::new(
        "beam",
        HandleKind::angle(
            Centre::default(),
            AngleParam::new("beam").y_up(),
            Reach::Param("radius".into(), 30.0),
        ),
    );
    assert_eq!(
        as_json(&h),
        json!({
            "name": "beam",
            "kind": "angle",
            "centre": { "x": "x", "y": "y" },
            "angle": { "name": "beam", "offset": 0.0, "y-up": true },
            "reach": { "param": "radius", "margin": 30.0 },
            "look": "rotate",
            "snap": "angle",
            "reset": null,
            "wheel": "turn"
        })
    );
    let fixed = Handle::new(
        "edge",
        HandleKind::Line {
            centre: Centre::default(),
            angle: AngleParam::new("a"),
            width: "w".into(),
            both_sides: false,
        },
    );
    let j = as_json(&fixed);
    assert_eq!(j["kind"], "line");
    assert_eq!(j["both-sides"], false);
    let fixed = Handle::new(
        "r",
        HandleKind::angle(Centre::default(), AngleParam::new("a"), Reach::Fixed(80.0)),
    );
    assert_eq!(as_json(&fixed)["reach"], json!({ "fixed": 80.0 }));
}

#[test]
fn left_out_fields_take_the_kind_defaults_and_null_says_none() {
    let h: Handle = serde_json::from_value(json!({
        "name": "band", "kind": "line", "angle": { "name": "a" }, "width": "w"
    }))
    .unwrap();
    let HandleKind::Line {
        centre, both_sides, ..
    } = &h.kind
    else {
        panic!("not a line: {:?}", h.kind);
    };
    assert_eq!(centre, &Centre::default());
    assert!(both_sides);
    assert_eq!(
        (h.look, h.snap, h.reset, h.wheel),
        (Look::Feather, None, Some(0.0), Some(Wheel::Scale))
    );

    let h: Handle = serde_json::from_value(json!({
        "name": "band", "kind": "line", "angle": { "name": "a" }, "width": "w",
        "reset": null, "wheel": null, "snap": "axes", "look": "edge"
    }))
    .unwrap();
    assert_eq!(
        (h.look, h.snap, h.reset, h.wheel),
        (Look::Edge, Some(Snap::Axes), None, None)
    );
}

#[test]
fn an_unknown_kind_is_left_out_and_the_others_load() {
    let file = json!({
        "format": FORMAT,
        "handles": [
            { "name": "centre", "kind": "point" },
            { "name": "tangent", "kind": "spline-tangent", "node": 3, "look": "control" },
            { "name": "radius", "kind": "radius", "radius": "radius" }
        ]
    })
    .to_string();
    let handles = Handles::from_json(&file).unwrap();

    // Kept as it was read, written back unchanged.
    let tangent = &handles.handles[1];
    assert_eq!(tangent.kind.name(), "unknown");
    assert!(tangent.writes().is_empty());
    assert!(tangent.drag(&params(&[]), [1.0, 2.0]).is_empty());
    assert_eq!(tangent.look, Look::Control);
    let again = as_json(tangent);
    assert_eq!(again["kind"], "spline-tangent");
    assert_eq!(again["node"], 3);

    let (known, skipped) = handles.split();
    let names: Vec<_> = known.iter().map(|h| h.name.as_str()).collect();
    assert_eq!(names, ["centre", "radius"]);
    assert_eq!(
        skipped,
        [Skipped {
            name: "tangent".into(),
            kind: "spline-tangent".into()
        }]
    );
    assert_eq!(
        skipped[0].to_string(),
        "handle `tangent`: unknown kind `spline-tangent`, from a newer format: left out"
    );
}

#[test]
fn a_known_kind_declared_wrong_is_still_an_error() {
    let file = json!({
        "format": FORMAT,
        "handles": [{ "name": "radius", "kind": "radius" }]
    })
    .to_string();
    let e = Handles::from_json(&file).unwrap_err();
    assert!(e.contains("radius"), "{e}");
    let missing = json!({ "format": FORMAT, "handles": [{ "name": "x" }] }).to_string();
    assert!(Handles::from_json(&missing).unwrap_err().contains("kind"));
}

#[test]
fn another_format_is_refused() {
    let file = json!({ "format": "egui_hud-handles/2", "handles": [] }).to_string();
    let e = Handles::from_json(&file).unwrap_err();
    assert!(e.contains("egui_hud-handles/2"), "{e}");
}

#[derive(Debug)]
struct Pin;

impl HandleMaths for Pin {
    fn writes(&self) -> Vec<String> {
        vec!["pin".into()]
    }
    fn position(&self, _: &dyn Params) -> Point {
        [1.0, 1.0]
    }
    fn drag(&self, _: &dyn Params, to: Point) -> Vec<(String, f64)> {
        vec![("pin".into(), to[0])]
    }
}

#[test]
fn a_custom_kind_never_serialises_nor_reads_back() {
    let h = Handle::new("pin", HandleKind::custom(Pin));
    let e = serde_json::to_string(&h).unwrap_err().to_string();
    assert!(e.contains("custom"), "{e}");
    assert!(Handles::new(vec![h]).to_json().is_err());

    // A file naming `custom` gets no maths: an unknown kind, left out.
    let file = json!({
        "format": FORMAT,
        "handles": [{ "name": "pin", "kind": "custom", "maths": "anything" }]
    })
    .to_string();
    let read = Handles::from_json(&file).unwrap();
    assert!(matches!(read.handles[0].kind, HandleKind::Unknown(_)));
    let (known, skipped) = read.split();
    assert!(known.is_empty());
    assert_eq!(skipped[0].kind, "custom");
}

/// The handles of Helios's radial exposure tool (its
/// `crates/helios-rack/tests/radial_exposure.rs`): a centre to move, a
/// radius around it, a feather past the radius reset to 25.
fn radial_exposure() -> Vec<Handle> {
    let centre = Centre::default;
    vec![
        Handle::new("centre", HandleKind::point(centre())).look(Look::Move),
        Handle::new("radius", HandleKind::radius(centre(), "radius")),
        Handle::new(
            "feather",
            HandleKind::feather(centre(), "radius", "feather"),
        )
        .reset(Some(25.0)),
    ]
}

#[test]
fn the_radial_exposure_fixture_reads_as_the_tool_builds_it() {
    let file = include_str!("fixtures/radial_exposure.json");
    let (read, skipped) = Handles::from_json(file).unwrap().split();
    assert!(skipped.is_empty());
    let built = radial_exposure();
    assert_eq!(
        read.iter().map(as_json).collect::<Vec<_>>(),
        built.iter().map(as_json).collect::<Vec<_>>()
    );
    let writes: Vec<_> = read.iter().map(Handle::writes).collect();
    assert_eq!(writes, [vec!["x", "y"], vec!["radius"], vec!["feather"]]);

    // Driven as the tool drives them: 150 from the centre, 30 past the
    // radius, a double click puts the feather back to 25.
    let p = params(&[
        ("x", 40.0),
        ("y", 30.0),
        ("radius", 150.0),
        ("feather", 25.0),
    ]);
    assert_eq!(read[1].drag(&p, [40.0, 180.0]), [("radius".into(), 150.0)]);
    assert_eq!(read[2].drag(&p, [220.0, 30.0]), [("feather".into(), 30.0)]);
    assert_eq!(read[2].reset_value(), Some(("feather".into(), 25.0)));
}

#[test]
fn the_crate_docs_example_reads() {
    let file = r#"{
      "format": "egui_hud-handles/1",
      "handles": [
        { "name": "centre", "kind": "point",
          "centre": { "x": "x", "y": "y" }, "look": "move" },
        { "name": "radius", "kind": "radius", "radius": "radius" },
        { "name": "feather", "kind": "feather",
          "radius": "radius", "feather": "feather", "reset": 25.0 },
        { "name": "beam", "kind": "angle",
          "angle": { "name": "beam", "y-up": true },
          "reach": { "param": "radius", "margin": 30.0 } }
      ]
    }"#;
    let (known, skipped) = Handles::from_json(file).unwrap().split();
    assert!(skipped.is_empty());
    assert_eq!(known.len(), 4);
    assert_eq!(known[3].wheel, Some(Wheel::Turn));
}

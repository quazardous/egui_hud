//! A generic control named and set up on the bench, kept as a configured
//! control, saved with the bench and loaded back; the same through the
//! `controls.bench.*` commands.

use egui_hud::Value;
use egui_hud::bench::{Bench, Generic, KEPT_GROUP, Settings, setup_id};
use egui_hud_demo::kit;
use serde_json::json;

#[test]
fn a_generic_toggle_is_named_on_the_bench_and_kept_as_a_preset() {
    let mut b = Bench::new(kit().unwrap());
    let key = b.add("toggle", None).unwrap();
    assert_eq!(key, "toggle");
    let mut s = Settings::named("Phase invert");
    s.line = "Flips the signal's polarity, to fix two microphones cancelling out".into();
    s.default = Some(json!(true));
    b.configure(&key, s.clone()).unwrap();
    let c = b.registry.get(&setup_id(&key)).unwrap();
    assert_eq!(c.label, "Phase invert");
    assert!(c.purpose.starts_with("Flips"));
    // Its value stays as it was: still a toggle, set up again.
    assert_eq!(b.value(&key), Some(Value::Bool(false)));
    // A number: a name, a range and a unit.
    let n = b
        .add_configured(
            "number",
            Settings {
                range: Some((20.0, 200.0)),
                unit: Some("Hz".into()),
                ..Settings::named("Cutoff")
            },
            None,
        )
        .unwrap();
    assert_eq!(n, "cutoff");
    assert_eq!(b.readout(&n), "20.0 Hz");
    // A choice with no choices is refused, the bench unchanged.
    assert!(
        b.add_configured("choice", Settings::named("Mode"), None)
            .is_err()
    );
    // A registered control has its own settings.
    b.add("gain", None).unwrap();
    assert!(b.configure("gain", Settings::named("Volume")).is_err());

    // Kept: on the shelf under its group, saved and loaded back.
    let name = b.keep(&key, Some("Phase"), None).unwrap();
    let e = b.shelf.get(&name).unwrap();
    assert_eq!(
        (e.group.as_str(), e.label.as_str()),
        (KEPT_GROUP, "Phase invert")
    );
    assert_eq!(e.hint, s.line);
    assert!(b.keep(&key, Some("mute"), None).is_err(), "a kit's name");
    b.add(&name, None).unwrap();
    let saved = serde_json::to_string(&b.save()).unwrap();

    let mut fresh = Bench::new(kit().unwrap());
    assert!(fresh.shelf.get(&name).is_none());
    fresh.load(serde_json::from_str(&saved).unwrap()).unwrap();
    assert_eq!(fresh.kept.len(), 1);
    assert!(fresh.shelf.get(&name).is_some());
    assert_eq!(
        fresh.registry.get(&setup_id("cutoff")).unwrap().label,
        "Cutoff"
    );
    assert_eq!(fresh.value("phase"), Some(Value::Bool(true)));
    assert_eq!(fresh.composition.keys(), b.composition.keys());
}

#[test]
fn the_bench_commands_add_set_up_and_keep() {
    let mut b = Bench::new(kit().unwrap());
    let out = b
        .run(
            "controls.bench.add",
            &json!({"control": "choice", "settings": {
                "label": "Room", "line": "Picks the room the reverb sounds like",
                "choices": ["Small", "Hall", "Church"], "default": "Hall"}}),
        )
        .unwrap();
    assert_eq!(out["key"], "room");
    assert_eq!(b.readout("room"), "Hall");
    let out = b
        .run(
            "controls.bench.configure",
            &json!({"key": "room", "settings": {"label": "Room", "choices": ["Small", "Hall"]}}),
        )
        .unwrap();
    assert_eq!(out["value"], "Hall");
    let out = b
        .run(
            "controls.bench.keep",
            &json!({"key": "room", "group": "Reverb"}),
        )
        .unwrap();
    assert_eq!(out["name"], "room");
    assert_eq!(b.shelf.get("room").unwrap().group, "Reverb");
    let list = b.run("controls.bench.list", &json!({})).unwrap();
    assert_eq!(list["controls"][0]["setup"]["generic"], "choice");
    let saved = b.run("controls.bench.save", &json!({})).unwrap();
    assert_eq!(saved["bench"]["configured"][0]["name"], "room");
    let mut other = Bench::new(kit().unwrap());
    other
        .run("controls.bench.load", &json!({"bench": saved["bench"]}))
        .unwrap();
    assert_eq!(
        other.shelf.get("room").unwrap().descriptor.choices,
        ["Small", "Hall"]
    );
    // Every generic control can be set up.
    for g in Generic::ALL {
        assert!(b.shelf.get(g.name()).is_some_and(|e| e.generic), "{g:?}");
    }
}

#[test]
fn a_generic_control_clicked_on_the_shelf_opens_its_set_up_headless() {
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable as _;

    let mut h = Harness::builder()
        .with_size(egui_pop::egui::vec2(1400.0, 1000.0))
        .build_ui_state(
            |ui, d: &mut egui_hud_demo::Demo| d.ui(ui),
            egui_hud_demo::Demo::new(),
        );
    h.run_steps(2);
    // The shelf scrolls: search it first.
    h.get_by_label("Search the shelf").type_text("toggle");
    h.run_steps(2);
    h.get_by_label("Shelf: Toggle").click_accesskit();
    h.run_steps(2);
    let setup = h.state().page.setup.clone().expect("its set-up opens");
    assert_eq!(setup.key, "toggle");
    let text = egui_pop::egui::accesskit::Role::TextInput;
    h.get_by_role_and_label(text, "Name").focus();
    h.run_steps(1);
    h.get_by_role_and_label(text, "Name").type_text(" switch");
    h.run_steps(2);
    let c = h.state().page.bench.settings("toggle").unwrap();
    assert_eq!(c.settings.label, "Toggle switch");
}

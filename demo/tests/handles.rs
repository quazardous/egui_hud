//! The handles preview: each handle of the spot light writes what its
//! kind's maths says, Escape puts a drag back, the wheel and a double
//! click adjust, and the page draws the stage.

use egui_hud_demo::Demo;
use egui_hud_demo::handles::Spot;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;

fn index(s: &Spot, name: &str) -> usize {
    s.handles.iter().position(|h| h.name == name).unwrap()
}

#[test]
fn each_handle_writes_its_kind_s_values() {
    let mut s = Spot::new();
    // The radius: the distance to the centre (150, 90).
    let r = index(&s, "radius");
    s.begin(r, s.position(r));
    s.update([150.0, 150.0], false, false);
    s.end();
    assert_eq!(s.value("radius"), 60.0);
    // The feather: the distance past the radius.
    let f = index(&s, "feather");
    s.begin(f, s.position(f));
    s.update([230.0, 90.0], false, false);
    s.end();
    assert_eq!(s.value("feather"), 20.0);
    // The beam: straight up on screen is 90°.
    let b = index(&s, "beam");
    s.begin(b, s.position(b));
    s.update([150.0, 10.0], false, false);
    s.end();
    assert!((s.value("beam") - 90.0).abs() < 1e-9);
    // The centre moves the whole spot; its handles follow.
    let c = index(&s, "centre");
    s.begin(c, s.position(c));
    s.update([100.0, 60.0], false, false);
    s.end();
    assert_eq!(s.position(r), [160.0, 60.0]);
}

#[test]
fn escape_puts_the_drag_back_and_the_wheel_and_reset_adjust() {
    let mut s = Spot::new();
    let r = index(&s, "radius");
    s.begin(r, s.position(r));
    s.update([150.0, 170.0], false, false);
    assert_eq!(s.value("radius"), 80.0);
    s.cancel();
    assert_eq!(s.value("radius"), 40.0);
    s.wheel(r, 100.0);
    assert!(s.value("radius") > 40.0);
    let b = index(&s, "beam");
    s.reset(b);
    assert_eq!(s.value("beam"), 90.0);
}

#[test]
fn the_page_draws_the_stage() {
    let mut h = Harness::builder()
        .with_size(egui_pop::egui::vec2(1400.0, 1200.0))
        .build_ui_state(|ui, d: &mut Demo| d.ui(ui), Demo::new());
    h.run_steps(3);
    let _ = h.get_by_label("Handles preview: a spot light");
}

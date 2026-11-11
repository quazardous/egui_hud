//! The page, headless: it draws its box of controls and its previews, a
//! control clicked in the box changes its preview, and a sweep of the
//! box's width keeps every frame within a time budget.

use std::time::{Duration, Instant};

use egui_hud::bench::WIDTH_RANGE;
use egui_hud::render::Drawn;
use egui_hud_demo::{Demo, GAIN, METER, SILENCE, preview};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;

fn harness() -> Harness<'static, Demo> {
    Harness::builder()
        .with_size(egui_pop::egui::vec2(1400.0, 900.0))
        .build_ui_state(|ui, d: &mut Demo| d.ui(ui), Demo::new())
}

#[test]
fn the_page_shows_its_title_box_and_previews() {
    let mut h = harness();
    h.run_steps(3);
    let _ = h.get_by_label(egui_hud_demo::TITLE);
    let zone = h.state().zone.expect("the box drawn");
    assert!(zone.width() >= h.state().width - 1.0, "{zone:?}");
    let drawn = &h.state().drawn;
    assert_eq!(drawn.get(GAIN), Some(&Drawn::Integrated), "{drawn:?}");
    assert_eq!(drawn.get(METER), Some(&Drawn::Integrated));
    assert!(
        h.get_all_by(|n| n
            .label()
            .is_some_and(|l| l.starts_with("Channel strip preview")))
            .next()
            .is_some()
    );
    assert!(
        h.get_all_by(|n| n.label().is_some_and(|l| l.starts_with("Lamp preview")))
            .next()
            .is_some()
    );
}

#[test]
fn a_toggle_clicked_in_the_box_changes_its_preview() {
    let mut h = harness();
    h.run_steps(3);
    assert!(!h.state().channel().muted);
    // The toggle in the box: not the Add button, nor the list's mark.
    let mute = h.get_by_role_and_label(egui_pop::egui::accesskit::Role::CheckBox, "Mute");
    mute.click_accesskit();
    h.run_steps(3);
    let d = h.state();
    assert!(d.channel().muted);
    let s = preview::strip(&d.channel(), 0.4);
    assert_eq!((s.left, s.right), (SILENCE, SILENCE));
    assert_eq!(d.level_at(0.4), SILENCE);
}

#[test]
fn a_face_picked_for_a_control_is_drawn() {
    let mut h = harness();
    h.state_mut().load_preset("Wide: fader").unwrap();
    h.run_steps(3);
    assert_eq!(h.state().drawn.get(GAIN), Some(&Drawn::Integrated));
    h.state_mut().pick_face(GAIN, "knob");
    h.run_steps(3);
    assert_eq!(h.state().drawn.get(GAIN), Some(&Drawn::Managed));
}

#[test]
fn a_width_sweep_stays_within_its_frame_budget() {
    // Generous for a debug build on a loaded machine: a freeze takes
    // seconds, a frame takes milliseconds.
    const BUDGET: Duration = Duration::from_millis(400);
    for preset in ["Mixer channel", "Light"] {
        let mut h = harness();
        h.state_mut().load_preset(preset).unwrap();
        h.run_steps(2);
        let mut w = WIDTH_RANGE.0;
        let mut worst = Duration::ZERO;
        while w <= WIDTH_RANGE.1 {
            h.state_mut().set_width(w);
            let start = Instant::now();
            h.run_steps(1);
            worst = worst.max(start.elapsed());
            let zone = h.state().zone.expect("the box drawn");
            assert!(
                (zone.width() - w).abs() < 1.0 || zone.width() > w,
                "{w}: {zone:?}"
            );
            w += 13.0;
        }
        assert!(worst < BUDGET, "{preset}: worst frame {worst:?}");
    }
}

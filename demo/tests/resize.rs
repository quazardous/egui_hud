//! The box resized by its right edge: every width resolves in time,
//! and the handle stays under the pointer, past either end of the range
//! too, so the pointer never lands on the bench mid-drag and the bench
//! never waits for it to leave (anti-jump) while it is being resized.

use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use egui_hud::bench::WIDTH_RANGE;
use egui_hud_demo::{Demo, bench, kit};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use egui_pop::egui::{Pos2, vec2};

/// Runs `f` on a thread; fails when it has not returned within `budget`,
/// so a frame that never returns fails the test instead of blocking it.
fn within<T: Send + 'static>(
    budget: Duration,
    what: String,
    f: impl FnOnce() -> T + Send + 'static,
) -> T {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(f());
    });
    match rx.recv_timeout(budget) {
        Ok(t) => t,
        Err(mpsc::RecvTimeoutError::Timeout) => panic!("{what}: no answer within {budget:?}"),
        Err(mpsc::RecvTimeoutError::Disconnected) => panic!("{what}: failed"),
    }
}

fn presets() -> Vec<String> {
    kit()
        .unwrap()
        .presets
        .iter()
        .map(|p| p.name.clone())
        .collect()
}

#[test]
fn every_width_resolves_in_time_for_every_preset_there_and_back() {
    for preset in presets() {
        for pointer in [false, true] {
            let p = preset.clone();
            within(
                Duration::from_mins(1),
                format!("{preset}, pointer on the bench: {pointer}"),
                move || {
                    let mut b = bench().unwrap();
                    b.load_preset(&p).unwrap();
                    b.pointer_in = pointer;
                    let mut widths = Vec::new();
                    let mut w = WIDTH_RANGE.0;
                    while w <= WIDTH_RANGE.1 {
                        widths.push(w);
                        w += 4.0;
                    }
                    let back: Vec<f32> = widths.iter().rev().copied().collect();
                    widths.extend(back);
                    for w in widths {
                        b.set_width(w);
                        let t = Instant::now();
                        b.resolve().unwrap();
                        let e = t.elapsed();
                        assert!(e < Duration::from_millis(500), "{p}, width {w}: {e:?}");
                    }
                },
            );
        }
    }
}

#[test]
fn the_width_handle_stays_under_the_pointer_past_either_end() {
    for preset in ["Mixer channel", "Narrow: knob"] {
        within(Duration::from_mins(2), preset.to_owned(), move || {
            let mut h = Harness::builder()
                .with_size(vec2(1280.0, 900.0))
                .build_ui_state(|ui, d: &mut Demo| d.ui(ui), Demo::new());
            h.state_mut().load_preset(preset).unwrap();
            h.run_steps(3);
            let start = h.get_by_label_contains("Box width").rect().center();
            let width = h.state().bench.width;
            h.hover_at(start);
            h.step();
            h.drag_at(start);
            h.step();
            // Far past the narrowest width, then back to where it started.
            let mut at = start;
            for i in 0..240 {
                let dx = if i < 120 { -4.0 } else { 4.0 };
                at = Pos2::new(at.x + dx, at.y);
                h.hover_at(at);
                let t = Instant::now();
                h.step();
                let e = t.elapsed();
                let s = h.state();
                let b = &s.bench;
                assert!(e < Duration::from_secs(2), "{preset}, step {i}: {e:?}");
                assert!(
                    !b.pointer_in,
                    "{preset}, step {i}: the hand counts as on the bench"
                );
                let last = s.last.as_ref().unwrap();
                assert!(
                    !last.pending,
                    "{preset}, step {i}: the bench waits for the pointer"
                );
                let follows = (width + at.x - start.x).clamp(WIDTH_RANGE.0, WIDTH_RANGE.1);
                assert!(
                    (b.width - follows).abs() < 0.5,
                    "{preset}, step {i}: width {} for the pointer at {}",
                    b.width,
                    at.x
                );
            }
            assert!((h.state().bench.width - width).abs() < 0.5);
            h.drop_at(at);
            h.step();
        });
    }
}

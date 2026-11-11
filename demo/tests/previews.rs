//! The previews: every mixer control changes the channel strip, every
//! light control changes the lamp, through pure functions of the values,
//! and through the controls in the box.

use egui_hud::{Value, ValueStore as _};
use egui_hud_demo::Demo;
use egui_hud_demo::preview::{BANDS, Channel, LOW_CUT_EDGE, Light, eq_gain, lamp, strip};

const T: f64 = 0.4;

#[test]
fn each_mixer_control_changes_the_channel_strip() {
    let base = Channel {
        eq: vec![0.0, 0.5, 1.0, 0.5],
        name: "Vocals".into(),
        ..Channel::default()
    };
    let at = |c: &Channel| strip(c, T);
    let before = at(&base);
    // Low cut: the left of the spectrum drops, the rest stays.
    let cut = at(&Channel {
        low_cut: true,
        ..base.clone()
    });
    assert!(cut.spectrum[0] < before.spectrum[0] - 20.0);
    let edge = (LOW_CUT_EDGE * (BANDS - 1) as f64).ceil() as usize + 1;
    assert_eq!(cut.spectrum[edge..], before.spectrum[edge..]);
    // The EQ curve: treble raised, the right of the spectrum rises.
    let eq = at(&Channel {
        eq: vec![0.0, 0.5, 1.0, 1.0],
        ..base.clone()
    });
    assert!(eq.spectrum[BANDS - 1] > before.spectrum[BANDS - 1] + 5.0);
    assert_eq!(eq_gain(&[0.0, 0.5, 1.0, 1.0], 0.5), 6.0);
    // Gain and mute drive the level.
    let loud = at(&Channel {
        gain: 6.0,
        ..base.clone()
    });
    assert!(loud.left > before.left && loud.spectrum[10] > before.spectrum[10]);
    let muted = at(&Channel {
        muted: true,
        ..base.clone()
    });
    assert_eq!(muted.left, egui_hud_demo::SILENCE);
    assert_eq!(muted.right, egui_hud_demo::SILENCE);
    // Pan splits it: centred, even; to the left, the left meter wins.
    assert!((before.left - before.right).abs() < 1e-9);
    let left = at(&Channel {
        pan: -0.8,
        ..base.clone()
    });
    assert!(left.left > left.right + 6.0);
    // Name and tag label the strip.
    assert_eq!(before.name, "Vocals");
    assert_eq!(before.tag, None);
    assert!(
        at(&Channel {
            tag: "Red".into(),
            ..base.clone()
        })
        .tag
        .is_some()
    );
}

#[test]
fn each_light_control_changes_the_lamp() {
    let base = Light::default();
    let before = lamp(&base, T);
    assert!(before.brightness > 0.5);
    let off = lamp(
        &Light {
            power: false,
            ..base.clone()
        },
        T,
    );
    assert_eq!(off.brightness, 0.0);
    let dim = lamp(
        &Light {
            intensity: 0.2,
            ..base.clone()
        },
        T,
    );
    assert!(dim.brightness < before.brightness);
    // Warm white is redder and less blue than cool white.
    let warm = lamp(
        &Light {
            temperature: 2700.0,
            ..base.clone()
        },
        T,
    )
    .rgb;
    let cool = lamp(
        &Light {
            temperature: 9000.0,
            ..base.clone()
        },
        T,
    )
    .rgb;
    assert!(warm[2] < cool[2] && warm[0] >= cool[0]);
    // A tint from the colour wheel, turned by the hue.
    let tinted = Light {
        colour: [0.0, 1.0, 0.5],
        ..base.clone()
    };
    let red = lamp(&tinted, T).rgb;
    assert!(red[0] > 0.9 && red[1] < 0.1, "{red:?}");
    let green = lamp(
        &Light {
            hue: 120.0,
            ..tinted.clone()
        },
        T,
    )
    .rgb;
    assert!(green[1] > 0.9 && green[0] < 0.1, "{green:?}");
    let brighter = lamp(
        &Light {
            colour: [0.0, 0.0, 0.9],
            intensity: 0.5,
            ..base.clone()
        },
        T,
    );
    assert!(
        brighter.brightness
            > lamp(
                &Light {
                    intensity: 0.5,
                    ..base.clone()
                },
                T
            )
            .brightness
    );
    // The effect animates it: every choice but steady changes over time.
    for effect in ["Fade", "Pulse", "Strobe"] {
        let l = Light {
            effect: effect.into(),
            ..base.clone()
        };
        let seen: Vec<f64> = (0..40)
            .map(|i| lamp(&l, f64::from(i) * 0.05).brightness)
            .collect();
        let (lo, hi) = seen
            .iter()
            .fold((1.0_f64, 0.0_f64), |(a, b), x| (a.min(*x), b.max(*x)));
        assert!(hi - lo > 0.2, "{effect}: {lo}..{hi}");
    }
}

#[test]
fn the_previews_read_the_controls_on_the_bench() {
    let mut d = Demo::new();
    d.load_preset("Mixer channel").unwrap();
    let before = d.channel();
    assert_eq!(before.name, "Vocals");
    let path = |k: &str| egui_hud::bench::path_of(k);
    let s = &mut d.bench.store;
    for (key, to) in [
        ("low-cut", Value::Bool(true)),
        ("mute", Value::Bool(true)),
        ("tag", Value::Choice("Blue".into())),
        ("name", Value::Text("Drums".into())),
        ("pan", Value::Float(0.5)),
        ("gain", Value::Float(-6.0)),
    ] {
        let g = s.begin(&"test", key);
        s.preview(g, &path(key), to, &"test", false).unwrap();
        s.commit(g);
    }
    let after = d.channel();
    assert!(after.low_cut && after.muted);
    assert_eq!((after.tag.as_str(), after.name.as_str()), ("Blue", "Drums"));
    assert_eq!((after.pan, after.gain), (0.5, -6.0));
    // The light's controls are not on the bench: the lamp reads their
    // defaults, then the bench once its preset is loaded.
    assert_eq!(d.light().effect, "Steady");
    d.load_preset("Light").unwrap();
    assert_eq!(d.light().intensity, 0.75);
}

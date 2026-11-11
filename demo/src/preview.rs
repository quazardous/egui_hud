//! The previews: what the controls on the bench drive, so every control
//! shows what it does.
//!
//! - [`strip`]: a mixer channel strip from the mixer controls: a
//!   simulated signal as a small spectrum (the low cut drops its left
//!   end, the EQ curve reshapes it), left and right meters (the gain and
//!   mute drive the level, pan splits it), the channel's name and colour
//!   tag.
//! - [`lamp`]: a lamp from the light controls: power, intensity
//!   (brightness), temperature (warm to cool white), the colour wheel and
//!   hue (a tint), the effect (steady, fade, pulse, strobe).
//!
//! Both are pure functions of the values and the time; [`paint_strip`]
//! and [`paint_lamp`] draw them.

use std::f64::consts::FRAC_PI_4;

use egui_pop::egui::{
    self, Color32, Rect, Role, Sense, Stroke, StrokeKind, Ui, WidgetInfo, pos2, vec2,
};

use crate::{PEAK, SILENCE, signal};

/// The spectrum's bands, from the bass (left) to the treble (right).
pub const BANDS: usize = 24;

/// Below this share of the spectrum (about 100 Hz), the low cut removes
/// the signal.
pub const LOW_CUT_EDGE: f64 = 0.2;

/// What the mixer controls say.
#[derive(Clone, Debug, PartialEq)]
pub struct Channel {
    /// The gain, in decibels.
    pub gain: f64,
    pub muted: bool,
    /// From -1 (left) to 1 (right).
    pub pan: f64,
    pub low_cut: bool,
    /// The EQ curve's points, `x, y` pairs in 0..1; `y` 0.5 is flat.
    pub eq: Vec<f64>,
    pub name: String,
    /// The colour tag's choice (`None`, `Red`…).
    pub tag: String,
}

impl Default for Channel {
    fn default() -> Self {
        Self {
            gain: 0.0,
            muted: false,
            pan: 0.0,
            low_cut: false,
            eq: Vec::new(),
            name: String::new(),
            tag: "None".into(),
        }
    }
}

/// The channel strip as the preview shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct Strip {
    /// Each band's level, in decibels, bass first.
    pub spectrum: Vec<f64>,
    /// The left and right meters, in decibels.
    pub left: f64,
    pub right: f64,
    pub name: String,
    /// The tag's colour, none for `None`.
    pub tag: Option<Color32>,
}

/// The EQ curve's gain at `x` (0..1), in decibels: its points joined by
/// straight lines, `y` 0.5 flat, 0 and 1 twelve decibels down and up.
pub fn eq_gain(points: &[f64], x: f64) -> f64 {
    let mut pts: Vec<(f64, f64)> = points.chunks_exact(2).map(|p| (p[0], p[1])).collect();
    if pts.is_empty() {
        return 0.0;
    }
    pts.sort_by(|a, b| a.0.total_cmp(&b.0));
    let y = if x <= pts[0].0 {
        pts[0].1
    } else if x >= pts[pts.len() - 1].0 {
        pts[pts.len() - 1].1
    } else {
        pts.windows(2)
            .find(|w| x >= w[0].0 && x <= w[1].0)
            .map_or(0.5, |w| {
                let span = (w[1].0 - w[0].0).max(1e-9);
                w[0].1 + (w[1].1 - w[0].1) * (x - w[0].0) / span
            })
    };
    (y - 0.5) * 24.0
}

/// A colour tag's colour.
pub fn tag_colour(tag: &str) -> Option<Color32> {
    Some(match tag {
        "Red" => Color32::from_rgb(220, 60, 60),
        "Orange" => Color32::from_rgb(235, 140, 40),
        "Yellow" => Color32::from_rgb(230, 210, 60),
        "Green" => Color32::from_rgb(70, 180, 90),
        "Blue" => Color32::from_rgb(70, 130, 230),
        "Purple" => Color32::from_rgb(150, 90, 210),
        _ => return None,
    })
}

/// The strip at time `t` (seconds).
pub fn strip(c: &Channel, t: f64) -> Strip {
    let spectrum = (0..BANDS)
        .map(|i| {
            if c.muted {
                return SILENCE;
            }
            let x = i as f64 / (BANDS - 1) as f64;
            // A signal like music: louder in the bass, moving.
            let base = -8.0 - 14.0 * x + 3.0 * (t * 2.3 + i as f64 * 0.7).sin();
            let cut = if c.low_cut && x < LOW_CUT_EDGE {
                -48.0 * (1.0 - x / LOW_CUT_EDGE)
            } else {
                0.0
            };
            (base + c.gain + eq_gain(&c.eq, x) + cut).clamp(SILENCE, PEAK)
        })
        .collect();
    let level = signal(c.gain, c.muted, t);
    // Equal power: in the middle each side reads the level; panned hard,
    // one side reads 3 dB more and the other falls silent.
    let angle = (c.pan.clamp(-1.0, 1.0) + 1.0) * FRAC_PI_4;
    let side = |share: f64| {
        if level <= SILENCE {
            SILENCE
        } else {
            (level + 20.0 * share.max(1e-6).log10() + 3.0103).clamp(SILENCE, PEAK)
        }
    };
    Strip {
        spectrum,
        left: side(angle.cos()),
        right: side(angle.sin()),
        name: c.name.clone(),
        tag: tag_colour(&c.tag),
    }
}

/// What the light controls say.
#[derive(Clone, Debug, PartialEq)]
pub struct Light {
    pub power: bool,
    /// 0..1.
    pub intensity: f64,
    /// In kelvin.
    pub temperature: f64,
    /// The colour wheel: hue (0..1), strength of the tint (0..1),
    /// brightness (0.5 even).
    pub colour: [f64; 3],
    /// The hue control, in degrees: turns the tint.
    pub hue: f64,
    /// `Steady`, `Fade`, `Pulse` or `Strobe`.
    pub effect: String,
}

impl Default for Light {
    fn default() -> Self {
        Self {
            power: true,
            intensity: 0.75,
            temperature: 5600.0,
            colour: [0.0, 0.0, 0.5],
            hue: 0.0,
            effect: "Steady".into(),
        }
    }
}

/// The lamp as the preview shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct Lamp {
    /// 0 (dark) to 1.
    pub brightness: f64,
    /// The light's colour at full brightness, each channel 0..1.
    pub rgb: [f64; 3],
}

/// The effect's share of the brightness at time `t`.
pub fn effect_share(effect: &str, t: f64) -> f64 {
    match effect {
        "Fade" => 0.55 + 0.45 * (t * 1.2).sin(),
        "Pulse" => 0.3 + 0.7 * (t * 6.0).sin().max(0.0),
        "Strobe" => {
            if (t * 6.0).fract() < 0.3 {
                1.0
            } else {
                0.0
            }
        }
        _ => 1.0,
    }
}

/// A white's colour from its temperature in kelvin, each channel 0..1.
pub fn white(kelvin: f64) -> [f64; 3] {
    let t = kelvin.clamp(1000.0, 40000.0) / 100.0;
    let r = if t <= 66.0 {
        255.0
    } else {
        329.698_727_446 * (t - 60.0).powf(-0.133_204_759_2)
    };
    let g = if t <= 66.0 {
        99.470_802_586_1 * t.ln() - 161.119_568_166_1
    } else {
        288.122_169_528_3 * (t - 60.0).powf(-0.075_514_849_2)
    };
    let b = if t >= 66.0 {
        255.0
    } else if t <= 19.0 {
        0.0
    } else {
        138.517_731_223_1 * (t - 10.0).ln() - 305.044_792_730_7
    };
    [r, g, b].map(|c| (c / 255.0).clamp(0.0, 1.0))
}

/// A hue (0..1) at full strength.
fn pure(hue: f64) -> [f64; 3] {
    let h = hue.rem_euclid(1.0) * 6.0;
    let x = 1.0 - (h % 2.0 - 1.0).abs();
    match h as u32 {
        0 => [1.0, x, 0.0],
        1 => [x, 1.0, 0.0],
        2 => [0.0, 1.0, x],
        3 => [0.0, x, 1.0],
        4 => [x, 0.0, 1.0],
        _ => [1.0, 0.0, x],
    }
}

/// The lamp at time `t` (seconds).
pub fn lamp(l: &Light, t: f64) -> Lamp {
    let [h, s, v] = l.colour;
    let brightness = if l.power {
        (l.intensity * effect_share(&l.effect, t) * (0.5 + v)).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let w = white(l.temperature);
    let tint = pure(h + l.hue / 360.0);
    let s = s.clamp(0.0, 1.0);
    let rgb = [0, 1, 2].map(|i| w[i] * (1.0 - s) + tint[i] * s);
    Lamp { brightness, rgb }
}

fn colour_of(rgb: [f64; 3], share: f64) -> Color32 {
    let [r, g, b] = rgb.map(|c| (c * share * 255.0).round().clamp(0.0, 255.0) as u8);
    Color32::from_rgb(r, g, b)
}

/// The preview's size, in points.
pub const SIZE: egui::Vec2 = egui::Vec2::new(300.0, 130.0);

/// The channel strip: its name and tag, the spectrum, left and right
/// meters.
pub fn paint_strip(ui: &mut Ui, s: &Strip) {
    let (rect, r) = ui.allocate_exact_size(SIZE, Sense::hover());
    let reading = |db: f64| {
        if db <= SILENCE {
            "silent".to_owned()
        } else {
            format!("{db:+.0} dB")
        }
    };
    let name = format!(
        "Channel strip preview: {}, left {}, right {}",
        if s.name.is_empty() {
            "unnamed"
        } else {
            &s.name
        },
        reading(s.left),
        reading(s.right)
    );
    r.widget_info(|| WidgetInfo::labeled(Role::Image, true, &name));
    let _ = r.on_hover_text("Driven by the mixer controls on the bench: low cut and EQ shape the spectrum, gain and mute the level, pan splits it left and right; name and colour tag label the strip");
    let v = ui.visuals().clone();
    let painter = ui.painter();
    painter.rect(
        rect,
        4.0,
        v.extreme_bg_color,
        v.widgets.noninteractive.bg_stroke,
        StrokeKind::Inside,
    );
    let inner = rect.shrink(6.0);
    // The name, on its tag's colour.
    let head = Rect::from_min_size(inner.min, vec2(inner.width(), 18.0));
    if let Some(c) = s.tag {
        painter.rect_filled(head, 3.0, c);
    }
    let text = if s.tag.is_some() {
        Color32::BLACK
    } else {
        v.text_color()
    };
    painter.text(
        head.left_center() + vec2(4.0, 0.0),
        egui::Align2::LEFT_CENTER,
        if s.name.is_empty() {
            "(no name)"
        } else {
            &s.name
        },
        egui::TextStyle::Body.resolve(ui.style()),
        text,
    );
    let body = Rect::from_min_max(pos2(inner.left(), head.bottom() + 4.0), inner.max);
    let meters = 30.0;
    let spec = Rect::from_min_max(body.min, pos2(body.right() - meters - 6.0, body.bottom()));
    let share = |db: f64| ((db - SILENCE) / (PEAK - SILENCE)).clamp(0.0, 1.0) as f32;
    let w = spec.width() / BANDS as f32;
    for (i, db) in s.spectrum.iter().enumerate() {
        let h = spec.height() * share(*db);
        let x = spec.left() + w * i as f32;
        let bar = Rect::from_min_max(
            pos2(x + 1.0, spec.bottom() - h),
            pos2(x + w - 1.0, spec.bottom()),
        );
        painter.rect_filled(bar, 1.0, Color32::from_rgb(90, 160, 220));
    }
    painter.hline(
        spec.x_range(),
        spec.bottom(),
        Stroke::new(1.0, v.weak_text_color()),
    );
    for (j, db) in [s.left, s.right].into_iter().enumerate() {
        let x = body.right() - meters + j as f32 * (meters / 2.0);
        let m = Rect::from_min_max(
            pos2(x + 1.0, body.top()),
            pos2(x + meters / 2.0 - 1.0, body.bottom()),
        );
        painter.rect_filled(m, 1.0, v.faint_bg_color);
        let h = m.height() * share(db);
        let colour = if db > 0.0 {
            Color32::from_rgb(220, 70, 60)
        } else if db > -12.0 {
            Color32::from_rgb(220, 190, 70)
        } else {
            Color32::from_rgb(90, 190, 110)
        };
        painter.rect_filled(
            Rect::from_min_max(pos2(m.left(), m.bottom() - h), m.max),
            1.0,
            colour,
        );
        painter.text(
            pos2(m.center().x, m.bottom() - 2.0),
            egui::Align2::CENTER_BOTTOM,
            if j == 0 { "L" } else { "R" },
            egui::TextStyle::Small.resolve(ui.style()),
            v.strong_text_color(),
        );
    }
}

/// The lamp: a bulb glowing in its colour and brightness.
pub fn paint_lamp(ui: &mut Ui, l: &Lamp) {
    let (rect, r) = ui.allocate_exact_size(vec2(SIZE.y + 60.0, SIZE.y), Sense::hover());
    let name = format!("Lamp preview: brightness {:.0}%", l.brightness * 100.0);
    r.widget_info(|| WidgetInfo::labeled(Role::Image, true, &name));
    let _ = r.on_hover_text("Driven by the light controls on the bench: power, intensity, temperature, colour, hue and effect");
    let v = ui.visuals().clone();
    let painter = ui.painter();
    painter.rect(
        rect,
        4.0,
        Color32::from_gray(18),
        v.widgets.noninteractive.bg_stroke,
        StrokeKind::Inside,
    );
    let c = rect.center();
    let radius = rect.height() * 0.3;
    // The glow, then the bulb.
    for k in (1..=4).rev() {
        let share = l.brightness * 0.12 * f64::from(5 - k) / 4.0;
        painter.circle_filled(c, radius * (1.0 + 0.25 * k as f32), colour_of(l.rgb, share));
    }
    let bulb = colour_of(l.rgb, 0.08 + 0.92 * l.brightness);
    painter.circle(c, radius, bulb, Stroke::new(1.0, Color32::from_gray(90)));
    if l.brightness <= 0.0 {
        painter.text(
            c,
            egui::Align2::CENTER_CENTER,
            "off",
            egui::TextStyle::Small.resolve(ui.style()),
            Color32::from_gray(160),
        );
    }
}

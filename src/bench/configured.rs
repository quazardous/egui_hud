//! Configured controls: a generic control (one per value type) and its
//! settings.
//!
//! A checkbox stays a checkbox whatever its domain: "Mute", "Low cut" and
//! "Power" are the same generic toggle, each with its own name, line and
//! default. A [`Configured`] control says exactly that: which
//! [`Generic`] control it is, and its [`Settings`] (a name, one line
//! saying what it does, and per value type its range, step, unit,
//! choices and default). A kit's configured controls are presets of the
//! generic controls, listed on the shelf under their domain group; the
//! generic controls themselves are configured controls with plain
//! settings, under [`MISC_GROUP`](super::MISC_GROUP).

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

use crate::value::{ParamDescriptor, ParamType, Value, Widget};

/// A generic control: one per value type, drawn by the faces of its type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Generic {
    /// A number in a range, with a step and a unit.
    Number,
    /// A whole number in a range.
    Whole,
    /// On or off.
    Toggle,
    /// One of a list of choices.
    Choice,
    /// A curve: points from left to right.
    Curve,
    /// A colour: hue, saturation, brightness.
    Colour,
    /// A line of text.
    Text,
}

impl Generic {
    pub const ALL: [Generic; 7] = [
        Generic::Number,
        Generic::Whole,
        Generic::Toggle,
        Generic::Choice,
        Generic::Curve,
        Generic::Colour,
        Generic::Text,
    ];

    /// Its name on the shelf and in a kit (`toggle`).
    pub fn name(self) -> &'static str {
        match self {
            Generic::Number => "number",
            Generic::Whole => "whole",
            Generic::Toggle => "toggle",
            Generic::Choice => "choice",
            Generic::Curve => "curve",
            Generic::Colour => "colour",
            Generic::Text => "text",
        }
    }

    /// What it holds, in words (`a toggle`).
    pub fn what(self) -> &'static str {
        match self {
            Generic::Number => "a number",
            Generic::Whole => "a whole number",
            Generic::Toggle => "a toggle",
            Generic::Choice => "a choice",
            Generic::Curve => "a curve (points)",
            Generic::Colour => "a colour",
            Generic::Text => "a text",
        }
    }

    /// The generic control as the shelf offers it, before anyone names
    /// it: plain settings, a name that says what it is.
    pub fn plain(self) -> Configured {
        let mut s = Settings::named(match self {
            Generic::Number => "Amount",
            Generic::Whole => "Count",
            Generic::Toggle => "Toggle",
            Generic::Choice => "Choice",
            Generic::Curve => "Curve",
            Generic::Colour => "Colour",
            Generic::Text => "Text",
        });
        s.line = format!(
            "Generic control of {}: add it, then name it and set it up",
            self.what()
        );
        match self {
            Generic::Number => {
                s.range = Some((0.0, 1.0));
                s.unit = Some("%".into());
                s.scale = Some(100.0);
                s.precision = Some(0);
                s.default = Some(Json::from(0.5));
            }
            Generic::Whole => {
                s.range = Some((0.0, 8.0));
                s.default = Some(Json::from(2));
            }
            Generic::Choice => {
                s.choices = vec!["First".into(), "Second".into(), "Third".into()];
            }
            Generic::Curve => {
                s.default = Some(Json::from(vec![0.0, 0.0, 0.25, 0.2, 0.75, 0.85, 1.0, 1.0]));
            }
            Generic::Colour => {
                s.default = Some(Json::from(vec![0.08, 0.3, 0.1]));
            }
            Generic::Toggle | Generic::Text => {}
        }
        Configured {
            name: self.name().into(),
            group: super::MISC_GROUP.into(),
            generic: self,
            settings: s,
        }
    }
}

/// A generic control's settings: its name, its line, and per value type
/// what applies (a range, step, unit and scale for a number; choices for
/// a choice; a default for each). What a value type does not use is
/// ignored.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    /// Its name, as it reads on the card (`Low cut`).
    pub label: String,
    /// One line, in plain words, saying what it does: shown on the shelf
    /// and when the pointer rests on the control.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub line: String,
    /// A number's lowest and highest value, as stored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<(f64, f64)>,
    /// A number's step, as stored; a hundredth of its range by default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<f64>,
    /// A number's unit, as it reads (`dB`, `%`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    /// What a number is multiplied by to read (100 for a share read in
    /// percent); 1 by default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<f64>,
    /// A number's decimals as it reads; from its step by default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub precision: Option<u32>,
    /// A choice's choices, in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<String>,
    /// Its default, as its value type writes it: a number, `true`, a
    /// choice, a list of numbers, a text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Json>,
}

impl Settings {
    /// Settings with only a name.
    pub fn named(label: &str) -> Self {
        Self {
            label: label.into(),
            ..Self::default()
        }
    }

    /// The parameter a generic control with these settings declares;
    /// refused when they do not hold (a range upside down, a choice with
    /// no choices, a default of another type).
    pub fn descriptor(&self, generic: Generic) -> Result<ParamDescriptor, String> {
        let label = self.label.trim();
        if label.is_empty() {
            return Err("a configured control needs a name".into());
        }
        let wrong = |what: &str| format!("`{label}`: the default must be {what}");
        let mut d = match generic {
            Generic::Number | Generic::Whole => {
                let whole = generic == Generic::Whole;
                let (lo, hi) = self
                    .range
                    .unwrap_or(if whole { (0.0, 10.0) } else { (0.0, 1.0) });
                if !(lo.is_finite() && hi.is_finite() && lo < hi) {
                    return Err(format!("`{label}`: the range must go up ({lo} to {hi})"));
                }
                let default = match &self.default {
                    None => lo.max(0.0).min(hi),
                    Some(j) => j.as_f64().ok_or_else(|| wrong("a number"))?.clamp(lo, hi),
                };
                let scale = self.scale.filter(|s| *s > 0.0).unwrap_or(1.0);
                let mut d = ParamDescriptor::float(label, lo, hi, default);
                d.step = if whole {
                    self.step.unwrap_or(1.0).round().max(1.0)
                } else {
                    self.step.filter(|s| *s > 0.0).unwrap_or((hi - lo) / 100.0)
                };
                d.coarse = d.step * 10.0;
                d.unit.clone_from(&self.unit);
                d.display_scale = scale;
                d.precision = if whole {
                    0
                } else {
                    self.precision.unwrap_or_else(|| decimals(d.step * scale))
                };
                if whole {
                    d.ty = ParamType::Int;
                    d.default = Value::Int(default.round() as i64);
                }
                d
            }
            Generic::Toggle => {
                let on = match &self.default {
                    None => false,
                    Some(j) => j.as_bool().ok_or_else(|| wrong("true or false"))?,
                };
                ParamDescriptor::new(ParamType::Bool, label, Value::Bool(on))
            }
            Generic::Choice => {
                let choices: Vec<String> = self
                    .choices
                    .iter()
                    .map(|c| c.trim().to_owned())
                    .filter(|c| !c.is_empty())
                    .collect();
                let Some(first) = choices.first().cloned() else {
                    return Err(format!("`{label}`: a choice needs its choices"));
                };
                let default = match &self.default {
                    None => first,
                    Some(j) => {
                        let c = j.as_str().ok_or_else(|| wrong("one of its choices"))?;
                        if !choices.iter().any(|x| x == c) {
                            return Err(wrong("one of its choices"));
                        }
                        c.to_owned()
                    }
                };
                let mut d = ParamDescriptor::new(ParamType::Choice, label, Value::Choice(default));
                d.choices = choices;
                d
            }
            Generic::Curve | Generic::Colour => {
                let numbers = match &self.default {
                    None if generic == Generic::Curve => vec![0.0, 0.0, 1.0, 1.0],
                    None => vec![0.0, 0.0, 0.5],
                    Some(j) => j
                        .as_array()
                        .and_then(|a| a.iter().map(Json::as_f64).collect::<Option<Vec<_>>>())
                        .ok_or_else(|| wrong("a list of numbers"))?,
                };
                let mut d = ParamDescriptor::new(ParamType::Floats, label, Value::Floats(numbers));
                d.widget = Some(if generic == Generic::Curve {
                    Widget::Curve
                } else {
                    Widget::Wheel
                });
                d
            }
            Generic::Text => {
                let text = match &self.default {
                    None => String::new(),
                    Some(j) => j.as_str().ok_or_else(|| wrong("a text"))?.to_owned(),
                };
                ParamDescriptor::new(ParamType::Text, label, Value::Text(text))
            }
        };
        self.line.trim().clone_into(&mut d.description);
        Ok(d)
    }
}

/// The decimals a step needs to read (0.01 needs 2).
fn decimals(step: f64) -> u32 {
    let mut n = 0;
    let mut s = step.abs();
    while n < 6 && s > 0.0 && (s - s.round()).abs() > 1e-9 {
        s *= 10.0;
        n += 1;
    }
    n
}

/// A configured control: a generic control and its settings. A kit's
/// configured controls are presets of the generic controls, on the shelf
/// under their group; one set up on the bench can be kept as one.
///
/// In a kit it reads flat: `{"name": "low-cut", "group": "Mixer
/// channel", "generic": "toggle", "label": "Low cut", "line": "Removes
/// the lowest frequencies, rumble and hum"}`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Configured {
    /// Its shelf name, what its key on the bench starts with.
    pub name: String,
    /// Its shelf group: a domain (`Mixer channel`).
    #[serde(default = "misc")]
    pub group: String,
    pub generic: Generic,
    #[serde(flatten)]
    pub settings: Settings,
}

fn misc() -> String {
    super::MISC_GROUP.into()
}

impl Configured {
    /// A generic control with settings, under a group.
    pub fn new(name: &str, group: &str, generic: Generic, settings: Settings) -> Self {
        Self {
            name: name.into(),
            group: group.into(),
            generic,
            settings,
        }
    }

    /// The parameter it declares (see [`Settings::descriptor`]).
    pub fn descriptor(&self) -> Result<ParamDescriptor, String> {
        self.settings.descriptor(self.generic)
    }
}

/// A key or a shelf name from a control's name: lower case, words joined
/// by dashes (`Low cut` gives `low-cut`).
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.trim().chars() {
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_make_the_parameter_of_their_generic_control() {
        let mut s = Settings::named("Cutoff");
        s.range = Some((20.0, 200.0));
        s.unit = Some("Hz".into());
        s.default = Some(Json::from(80.0));
        let d = s.descriptor(Generic::Number).unwrap();
        assert_eq!(d.hard, Some((20.0, 200.0)));
        assert_eq!(d.default, Value::Float(80.0));
        assert_eq!(d.step, 1.8);
        assert_eq!(d.precision, 1);
        let mut c = Settings::named("Mode");
        assert!(c.descriptor(Generic::Choice).is_err());
        c.choices = vec!["A".into(), "B".into()];
        c.default = Some(Json::from("B"));
        assert_eq!(
            c.descriptor(Generic::Choice).unwrap().default,
            Value::Choice("B".into())
        );
        assert!(Settings::named(" ").descriptor(Generic::Toggle).is_err());
        for g in Generic::ALL {
            g.plain().descriptor().unwrap();
        }
        assert_eq!(slug(" Low  cut!"), "low-cut");
    }

    #[test]
    fn a_configured_control_reads_flat_in_a_kit() {
        let c: Configured = serde_json::from_str(
            r#"{"name": "mute", "group": "Mixer", "generic": "toggle", "label": "Mute", "line": "Silences it"}"#,
        )
        .unwrap();
        assert_eq!(c.generic, Generic::Toggle);
        assert_eq!(c.descriptor().unwrap().description, "Silences it");
        let back = serde_json::to_value(&c).unwrap();
        assert_eq!(back["label"], "Mute");
    }
}

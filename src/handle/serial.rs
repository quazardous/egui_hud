//! Handles as serde data: the kinds tagged by `kind`, an unknown kind
//! kept as it was read, `Custom` refused both ways, and the [`Handles`]
//! file with its format.

use serde::de::Error as _;
use serde::ser::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value as Json;

use super::{AngleParam, Centre, Handle, HandleKind, Look, Proportional, Reach, Snap, Wheel};

/// What a handles file says it is.
pub const FORMAT: &str = "egui_hud-handles/1";

// ---- Reach: `{ "fixed": d }` or `{ "param": name, "margin": m }`. ----

#[derive(Serialize, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
enum ReachDecl {
    Fixed {
        fixed: f64,
    },
    Param {
        param: String,
        #[serde(default)]
        margin: f64,
    },
}

impl Serialize for Reach {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Reach::Fixed(d) => ReachDecl::Fixed { fixed: *d },
            Reach::Param(n, m) => ReachDecl::Param {
                param: n.clone(),
                margin: *m,
            },
        }
        .serialize(s)
    }
}

impl<'de> Deserialize<'de> for Reach {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Reach, D::Error> {
        ReachDecl::deserialize(d)
            .map(|r| match r {
                ReachDecl::Fixed { fixed } => Reach::Fixed(fixed),
                ReachDecl::Param { param, margin } => Reach::Param(param, margin),
            })
            .map_err(|_| {
                D::Error::custom(
                    "a reach is { \"fixed\": distance } or { \"param\": name, \"margin\": distance }",
                )
            })
    }
}

// ---- HandleKind: internally tagged by `kind`. ----

fn yes() -> bool {
    true
}

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde's skip_serializing_if passes a reference"
)]
fn is_true(b: &bool) -> bool {
    *b
}

/// A known kind, borrowed, as it is written.
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
enum KindOut<'a> {
    Point {
        centre: &'a Centre,
    },
    Radius {
        centre: &'a Centre,
        radius: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        angle: Option<&'a AngleParam>,
        #[serde(skip_serializing_if = "std::ops::Not::not")]
        turns: bool,
    },
    Feather {
        centre: &'a Centre,
        radius: &'a str,
        feather: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        angle: Option<&'a AngleParam>,
        #[serde(skip_serializing_if = "Option::is_none")]
        proportional: Option<&'a Proportional>,
    },
    Angle {
        centre: &'a Centre,
        angle: &'a AngleParam,
        reach: &'a Reach,
    },
    #[serde(rename_all = "kebab-case")]
    Line {
        centre: &'a Centre,
        angle: &'a AngleParam,
        width: &'a str,
        #[serde(skip_serializing_if = "is_true")]
        both_sides: bool,
    },
}

/// The kinds this version reads, as they are declared (an error in one
/// of them is still an error: only an unknown `kind` degrades).
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
enum KindIn {
    Point {
        #[serde(default)]
        centre: Centre,
    },
    Radius {
        #[serde(default)]
        centre: Centre,
        radius: String,
        #[serde(default)]
        angle: Option<AngleParam>,
        #[serde(default)]
        turns: bool,
    },
    Feather {
        #[serde(default)]
        centre: Centre,
        radius: String,
        feather: String,
        #[serde(default)]
        angle: Option<AngleParam>,
        #[serde(default)]
        proportional: Option<Proportional>,
    },
    Angle {
        #[serde(default)]
        centre: Centre,
        angle: AngleParam,
        reach: Reach,
    },
    #[serde(rename_all = "kebab-case")]
    Line {
        #[serde(default)]
        centre: Centre,
        angle: AngleParam,
        width: String,
        #[serde(default = "yes")]
        both_sides: bool,
    },
}

impl HandleKind {
    /// The `kind` names this version reads.
    pub const KINDS: [&'static str; 5] = ["point", "radius", "feather", "angle", "line"];
}

impl From<KindIn> for HandleKind {
    fn from(k: KindIn) -> HandleKind {
        match k {
            KindIn::Point { centre } => HandleKind::Point(centre),
            KindIn::Radius {
                centre,
                radius,
                angle,
                turns,
            } => HandleKind::Radius {
                centre,
                radius,
                angle,
                turns,
            },
            KindIn::Feather {
                centre,
                radius,
                feather,
                angle,
                proportional,
            } => HandleKind::Feather {
                centre,
                radius,
                feather,
                angle,
                proportional,
            },
            KindIn::Angle {
                centre,
                angle,
                reach,
            } => HandleKind::Angle {
                centre,
                angle,
                reach,
            },
            KindIn::Line {
                centre,
                angle,
                width,
                both_sides,
            } => HandleKind::Line {
                centre,
                angle,
                width,
                both_sides,
            },
        }
    }
}

impl Serialize for HandleKind {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let out = match self {
            HandleKind::Point(centre) => KindOut::Point { centre },
            HandleKind::Radius {
                centre,
                radius,
                angle,
                turns,
            } => KindOut::Radius {
                centre,
                radius,
                angle: angle.as_ref(),
                turns: *turns,
            },
            HandleKind::Feather {
                centre,
                radius,
                feather,
                angle,
                proportional,
            } => KindOut::Feather {
                centre,
                radius,
                feather,
                angle: angle.as_ref(),
                proportional: proportional.as_ref(),
            },
            HandleKind::Angle {
                centre,
                angle,
                reach,
            } => KindOut::Angle {
                centre,
                angle,
                reach,
            },
            HandleKind::Line {
                centre,
                angle,
                width,
                both_sides,
            } => KindOut::Line {
                centre,
                angle,
                width,
                both_sides: *both_sides,
            },
            HandleKind::Custom(_) => {
                return Err(S::Error::custom(
                    "a custom handle kind is code: it cannot be serialised",
                ));
            }
            HandleKind::Unknown(u) => return u.raw.serialize(s),
        };
        out.serialize(s)
    }
}

impl<'de> Deserialize<'de> for HandleKind {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<HandleKind, D::Error> {
        let raw = Json::deserialize(d)?;
        let kind = raw
            .get("kind")
            .and_then(Json::as_str)
            .ok_or_else(|| D::Error::missing_field("kind"))?
            .to_owned();
        if HandleKind::KINDS.contains(&kind.as_str()) {
            return KindIn::deserialize(raw)
                .map(HandleKind::from)
                .map_err(D::Error::custom);
        }
        Ok(HandleKind::Unknown(UnknownKind { kind, raw }))
    }
}

/// A handle kind a newer declaration names and this version does not
/// know: its `kind`, and the kind's fields as they were read, written
/// back unchanged.
#[derive(Clone, Debug, PartialEq)]
pub struct UnknownKind {
    pub kind: String,
    pub raw: Json,
}

// ---- Handle: its name, its kind's fields, look, snap, reset, wheel. ----

#[derive(Serialize)]
struct HandleOut<'a> {
    name: &'a str,
    #[serde(flatten)]
    kind: &'a HandleKind,
    look: Look,
    snap: Option<Snap>,
    reset: Option<f64>,
    wheel: Option<Wheel>,
}

/// Left out: `None`; `null`: `Some(None)`.
#[allow(
    clippy::option_option,
    reason = "a field left out takes the kind's default, a null says none"
)]
fn given<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<Option<T>>, D::Error> {
    Option::<T>::deserialize(d).map(Some)
}

#[derive(Deserialize)]
#[allow(
    clippy::option_option,
    reason = "a field left out takes the kind's default, a null says none"
)]
struct HandleIn {
    name: String,
    #[serde(flatten)]
    kind: HandleKind,
    #[serde(default)]
    look: Option<Look>,
    #[serde(default, deserialize_with = "given")]
    snap: Option<Option<Snap>>,
    #[serde(default, deserialize_with = "given")]
    reset: Option<Option<f64>>,
    #[serde(default, deserialize_with = "given")]
    wheel: Option<Option<Wheel>>,
}

impl Serialize for Handle {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        HandleOut {
            name: &self.name,
            kind: &self.kind,
            look: self.look,
            snap: self.snap,
            reset: self.reset,
            wheel: self.wheel,
        }
        .serialize(s)
    }
}

impl<'de> Deserialize<'de> for Handle {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Handle, D::Error> {
        let h = HandleIn::deserialize(d)?;
        let mut out = Handle::new(h.name, h.kind);
        if let Some(look) = h.look {
            out.look = look;
        }
        if let Some(snap) = h.snap {
            out.snap = snap;
        }
        if let Some(reset) = h.reset {
            out.reset = reset;
        }
        if let Some(wheel) = h.wheel {
            out.wheel = wheel;
        }
        Ok(out)
    }
}

// ---- The file. ----

/// A handles file: its format ([`FORMAT`]) and its handles, in order.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Handles {
    pub format: String,
    pub handles: Vec<Handle>,
}

/// A handle left out of a declaration, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Skipped {
    /// The handle's name.
    pub name: String,
    /// Its `kind`, which this version does not know.
    pub kind: String,
}

impl std::fmt::Display for Skipped {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "handle `{}`: unknown kind `{}`, from a newer format: left out",
            self.name, self.kind
        )
    }
}

impl Handles {
    /// These handles, in this version's format.
    pub fn new(handles: Vec<Handle>) -> Self {
        Self {
            format: FORMAT.into(),
            handles,
        }
    }

    /// Reads a handles file. Refused when it is not JSON, its format is
    /// not [`FORMAT`], or a known kind is declared wrong; a kind this
    /// version does not know is kept ([`HandleKind::Unknown`]).
    pub fn from_json(json: &str) -> Result<Self, String> {
        let h: Handles = serde_json::from_str(json).map_err(|e| e.to_string())?;
        if h.format != FORMAT {
            return Err(format!(
                "not a handles file this version reads: format `{}`, not `{FORMAT}`",
                h.format
            ));
        }
        Ok(h)
    }

    /// The file as JSON. Refused when it holds a [`HandleKind::Custom`].
    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self).map_err(|e| e.to_string())
    }

    /// The handles this version knows, in order, and the ones it leaves
    /// out (an unknown kind), for the host to report.
    pub fn split(self) -> (Vec<Handle>, Vec<Skipped>) {
        let mut known = Vec::new();
        let mut skipped = Vec::new();
        for h in self.handles {
            match h.kind {
                HandleKind::Unknown(u) => skipped.push(Skipped {
                    name: h.name,
                    kind: u.kind,
                }),
                _ => known.push(h),
            }
        }
        (known, skipped)
    }
}

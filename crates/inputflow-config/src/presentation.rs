use std::collections::BTreeSet;

use inputflow_engine::{Action, Key, PresentationAction, Rule};
use serde::{Deserialize, Serialize};

use crate::config::{ConfigError, KeyConfig, TriggerConfig, resolve_mode_rule};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MovementMode {
    #[default]
    Relative,
    Absolute,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Style {
    Ring {
        color: String,
        diameter: u32,
        thickness: u32,
    },
    Dot {
        color: String,
        diameter: u32,
    },
    Ink {
        color: String,
        width: u32,
        #[serde(default)]
        retain_seconds: Option<u32>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preset {
    pub id: String,
    pub name: String,
    pub binding: Option<TriggerConfig>,
    pub style: Style,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PresentationConfig {
    pub enabled: bool,
    pub start: TriggerConfig,
    pub stop: TriggerConfig,
    pub temporary_mouse: Option<TriggerConfig>,
    pub clear: Option<TriggerConfig>,
    pub presets: Vec<Preset>,
    pub default_preset: String,
    pub display: Option<String>,
    pub movement: MovementMode,
}

impl Default for PresentationConfig {
    fn default() -> Self {
        let double = |key| TriggerConfig::DoubleTap {
            key: KeyConfig::logical(key),
        };
        let alt = |key| {
            Some(TriggerConfig::KeyChord {
                first: KeyConfig::logical(Key::LeftAlt),
                second: KeyConfig::logical(key),
            })
        };
        Self {
            enabled: false,
            start: double(Key::LeftCtrl),
            stop: double(Key::RightCtrl),
            temporary_mouse: alt(Key::Digit4),
            clear: alt(Key::Digit5),
            presets: vec![
                Preset {
                    id: "red-ring".into(),
                    name: "Red ring".into(),
                    binding: alt(Key::Digit1),
                    style: Style::Ring {
                        color: "#FF3333".into(),
                        diameter: 28,
                        thickness: 3,
                    },
                },
                Preset {
                    id: "yellow-dot".into(),
                    name: "Yellow dot".into(),
                    binding: alt(Key::Digit2),
                    style: Style::Dot {
                        color: "#FFE500".into(),
                        diameter: 18,
                    },
                },
                Preset {
                    id: "blue-ink".into(),
                    name: "Blue ink".into(),
                    binding: alt(Key::Digit3),
                    style: Style::Ink {
                        color: "#2684FF".into(),
                        width: 5,
                        retain_seconds: None,
                    },
                },
            ],
            default_preset: "red-ring".into(),
            display: None,
            movement: MovementMode::Relative,
        }
    }
}

pub fn parse_color(value: &str) -> Option<u32> {
    let digits = value.strip_prefix('#')?;
    (digits.len() == 6 && digits.bytes().all(|ch| ch.is_ascii_hexdigit()))
        .then(|| u32::from_str_radix(digits, 16).ok())
        .flatten()
}

impl PresentationConfig {
    pub(crate) fn resolved_rules(
        &self,
        emergency: Option<Key>,
        errors: &mut Vec<ConfigError>,
    ) -> Vec<Rule> {
        let mut ids = BTreeSet::new();
        if self.presets.len() > 32 || (self.enabled && self.presets.is_empty()) {
            errors.push(ConfigError(
                "presentation.presets requires 1–32 presets when enabled".into(),
            ));
        }
        if !self.presets.is_empty()
            && !self
                .presets
                .iter()
                .any(|preset| preset.id == self.default_preset)
        {
            errors.push(ConfigError(
                "presentation.default_preset must name an existing preset ID".into(),
            ));
        }
        if self
            .display
            .as_ref()
            .is_some_and(|name| name.trim().is_empty() || name.len() > 128 || name.contains('\0'))
        {
            errors.push(ConfigError(
                "presentation.display must be a nonempty display ID of at most 128 bytes".into(),
            ));
        }
        let mut bindings = vec![
            (
                "presentation.start".to_string(),
                Some(&self.start),
                PresentationAction::Start,
            ),
            (
                "presentation.stop".to_string(),
                Some(&self.stop),
                PresentationAction::Stop,
            ),
            (
                "presentation.temporary_mouse".to_string(),
                self.temporary_mouse.as_ref(),
                PresentationAction::ToggleMouse,
            ),
            (
                "presentation.clear".to_string(),
                self.clear.as_ref(),
                PresentationAction::Clear,
            ),
        ];
        for (index, preset) in self.presets.iter().enumerate() {
            let field = format!("presentation.presets[{index}]");
            if preset.id.trim().is_empty() || preset.id.len() > 64 || !ids.insert(&preset.id) {
                errors.push(ConfigError(format!(
                    "{field}.id must be nonempty, unique and at most 64 bytes"
                )));
            }
            if preset.name.trim().is_empty() || preset.name.len() > 128 {
                errors.push(ConfigError(format!(
                    "{field}.name must be nonempty and at most 128 bytes"
                )));
            }
            let (color, valid) = match &preset.style {
                Style::Ring {
                    color,
                    diameter,
                    thickness,
                } => (
                    color,
                    (4..=256).contains(diameter) && *thickness > 0 && *thickness < *diameter / 2,
                ),
                Style::Dot { color, diameter } => (color, (2..=256).contains(diameter)),
                Style::Ink {
                    color,
                    width,
                    retain_seconds,
                } => (
                    color,
                    (1..=64).contains(width)
                        && retain_seconds.is_none_or(|seconds| (1..=600).contains(&seconds)),
                ),
            };
            if parse_color(color).is_none() {
                errors.push(ConfigError(format!("{field}.style.color must use #RRGGBB")));
            }
            if !valid {
                errors.push(ConfigError(format!(
                    "{field}.style dimensions or retention are out of range"
                )));
            }
            if self.enabled && self.presets.len() > 1 && preset.binding.is_none() {
                errors.push(ConfigError(format!(
                    "{field}.binding is required when multiple presets are enabled"
                )));
            }
            bindings.push((
                format!("{field}.binding"),
                preset.binding.as_ref(),
                PresentationAction::Select(preset.id.clone()),
            ));
        }
        bindings
            .into_iter()
            .filter_map(|(field, binding, action)| {
                resolve_mode_rule(
                    field,
                    binding?,
                    Action::Presentation(action),
                    emergency,
                    errors,
                )
            })
            .collect()
    }
}

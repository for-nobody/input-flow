use std::collections::BTreeSet;

use inputflow_engine::{Action, Key, NumpadAction, Rule};
use inputflow_touchpad::panel::{Geometry, Region, rectangle, valid_label};
use serde::{Deserialize, Serialize};

use crate::config::{ConfigError, KeyConfig, TriggerConfig, parse_key, resolve_mode_rule};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NumpadGuide {
    Always,
    TwoSeconds,
    #[default]
    UntilFirstInput,
    Never,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NumpadConfig {
    pub enabled: bool,
    pub start: TriggerConfig,
    pub stop: TriggerConfig,
    pub guide: NumpadGuide,
    pub default_layout: Option<String>,
    pub layouts: Vec<PanelLayout>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PanelLayout {
    pub id: String,
    pub name: String,
    pub select: Option<TriggerConfig>,
    pub geometry: Geometry<PanelAction>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PanelOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum PanelAction {
    Digit { digit: u8 },
    Operator { operator: PanelOperator },
    KeyChord { keys: Vec<KeyConfig> },
}

pub enum PanelOutput {
    Digit(u8),
    Keys(Vec<Key>),
}

impl PanelAction {
    pub fn resolve(&self) -> Result<PanelOutput, String> {
        match self {
            Self::Digit { digit } if *digit <= 9 => Ok(PanelOutput::Digit(*digit)),
            Self::Digit { .. } => Err("digit must be between 0 and 9".into()),
            Self::Operator { operator } => Ok(PanelOutput::Keys(vec![match operator {
                PanelOperator::Add => Key::NumpadAdd,
                PanelOperator::Subtract => Key::NumpadSubtract,
                PanelOperator::Multiply => Key::NumpadMultiply,
                PanelOperator::Divide => Key::NumpadDivide,
            }])),
            Self::KeyChord { keys } => {
                if !(1..=8).contains(&keys.len()) {
                    return Err("a panel action needs one to eight keys".into());
                }
                let keys = keys.iter().map(parse_key).collect::<Result<Vec<_>, _>>()?;
                let mut seen = BTreeSet::new();
                if keys.iter().any(|&key| {
                    matches!(
                        key,
                        Key::NumLock
                            | Key::Physical {
                                scan_code: 0x45,
                                ..
                            }
                    ) || !seen.insert(key)
                }) {
                    return Err("panel actions cannot contain Num Lock or repeated keys".into());
                }
                Ok(PanelOutput::Keys(keys))
            }
        }
    }
}

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct LegacyNumpad {
    enabled: bool,
    start: TriggerConfig,
    stop: TriggerConfig,
    guide: NumpadGuide,
}

impl Default for LegacyNumpad {
    fn default() -> Self {
        let double_tap = |key| TriggerConfig::DoubleTap {
            key: KeyConfig::logical(key),
        };
        Self {
            enabled: false,
            start: double_tap(Key::F9),
            stop: double_tap(Key::F10),
            guide: NumpadGuide::default(),
        }
    }
}

impl From<LegacyNumpad> for NumpadConfig {
    fn from(legacy: LegacyNumpad) -> Self {
        let classic = PanelLayout::classic();
        Self {
            enabled: legacy.enabled,
            start: legacy.start,
            stop: legacy.stop,
            guide: legacy.guide,
            default_layout: Some(classic.id.clone()),
            layouts: vec![classic],
        }
    }
}

impl Default for NumpadConfig {
    fn default() -> Self {
        let mut config: Self = LegacyNumpad::default().into();
        config.layouts = PanelLayout::presets();
        config.default_layout = Some(config.layouts[0].id.clone());
        config
    }
}

fn chord(key: Key) -> PanelAction {
    PanelAction::KeyChord {
        keys: vec![KeyConfig::logical(key)],
    }
}

fn select(key: Key) -> TriggerConfig {
    TriggerConfig::KeyChord {
        first: KeyConfig::logical(Key::LeftAlt),
        second: KeyConfig::logical(key),
    }
}

fn cell(
    name: &str,
    action: Option<PanelAction>,
    rect: [f64; 4],
    index: usize,
) -> Region<PanelAction> {
    Region {
        id: format!("cell-{index}"),
        name: name.into(),
        action,
        faces: vec![rectangle(rect[0], rect[1], rect[2], rect[3])],
    }
}

impl PanelLayout {
    pub fn classic() -> Self {
        use inputflow_touchpad::numpad::DIGITS;
        let regions = DIGITS
            .into_iter()
            .flatten()
            .enumerate()
            .map(|(i, digit)| {
                let x = (i % 3) as f64 / 3.0;
                let y = (i / 3) as f64 / 4.0;
                cell(
                    &digit.map_or_else(|| "Empty".into(), |digit| digit.to_string()),
                    digit.map(|digit| PanelAction::Digit { digit }),
                    [x, y, (i % 3 + 1) as f64 / 3.0, y + 0.25],
                    i,
                )
            })
            .collect();
        Self {
            id: "classic-numpad".into(),
            name: "Classic numpad".into(),
            select: None,
            geometry: Geometry {
                gap: [0.04 / 3.0, 0.04 / 4.0],
                regions,
            },
        }
    }

    pub fn presets() -> Vec<Self> {
        use PanelAction::{Digit, Operator};
        use PanelOperator::{Add, Divide, Multiply, Subtract};
        let mut digits = [
            ("7", Digit { digit: 7 }),
            ("8", Digit { digit: 8 }),
            ("9", Digit { digit: 9 }),
            ("÷", Operator { operator: Divide }),
            ("4", Digit { digit: 4 }),
            ("5", Digit { digit: 5 }),
            ("6", Digit { digit: 6 }),
            ("×", Operator { operator: Multiply }),
            ("1", Digit { digit: 1 }),
            ("2", Digit { digit: 2 }),
            ("3", Digit { digit: 3 }),
            ("−", Operator { operator: Subtract }),
        ]
        .into_iter()
        .enumerate()
        .map(|(i, (name, action))| {
            let x = (i % 4) as f64 * 0.25;
            let y = (i / 4) as f64 * 0.25;
            cell(name, Some(action), [x, y, x + 0.25, y + 0.25], i)
        })
        .collect::<Vec<_>>();
        digits.extend([
            cell("0", Some(Digit { digit: 0 }), [0.0, 0.75, 0.5, 1.0], 12),
            cell("Enter", Some(chord(Key::Enter)), [0.5, 0.75, 0.75, 1.0], 13),
            cell(
                "+",
                Some(Operator { operator: Add }),
                [0.75, 0.75, 1.0, 1.0],
                14,
            ),
        ]);
        let media = [
            ("Previous", Key::MediaPreviousTrack),
            ("Play/Pause", Key::MediaPlayPause),
            ("Next", Key::MediaNextTrack),
            ("Volume −", Key::VolumeDown),
            ("Mute", Key::VolumeMute),
            ("Volume +", Key::VolumeUp),
        ]
        .into_iter()
        .enumerate()
        .map(|(i, (name, key))| {
            let y = (i / 3) as f64 * 0.5;
            cell(
                name,
                Some(chord(key)),
                [(i % 3) as f64 / 3.0, y, (i % 3 + 1) as f64 / 3.0, y + 0.5],
                i,
            )
        })
        .collect();
        let slides = vec![
            cell(
                "Previous slide",
                Some(chord(Key::PageUp)),
                [0.0, 0.0, 0.5, 1.0],
                0,
            ),
            cell(
                "Next slide",
                Some(chord(Key::PageDown)),
                [0.5, 0.0, 1.0, 1.0],
                1,
            ),
        ];
        [
            ("numpad", "Numpad", Key::Digit1, digits),
            ("media", "Media", Key::Digit2, media),
            ("slides", "Presentation controls", Key::Digit3, slides),
        ]
        .into_iter()
        .map(|(id, name, key, regions)| Self {
            id: id.into(),
            name: name.into(),
            select: Some(select(key)),
            geometry: Geometry {
                gap: [0.005, 0.005],
                regions,
            },
        })
        .collect()
    }
}

impl NumpadConfig {
    pub(crate) fn resolved_rules(
        &self,
        emergency: Option<Key>,
        errors: &mut Vec<ConfigError>,
    ) -> Vec<Rule> {
        let mut rules: Vec<_> = [
            ("numpad.start", &self.start, NumpadAction::Start),
            ("numpad.stop", &self.stop, NumpadAction::Stop),
        ]
        .into_iter()
        .filter_map(|(field, trigger, action)| {
            resolve_mode_rule(
                field.into(),
                trigger,
                Action::Numpad(action),
                emergency,
                errors,
            )
        })
        .collect();
        if self.layouts.len() > 24 {
            errors.push(ConfigError(
                "panel: at most 24 layouts are supported".into(),
            ));
        }
        let mut ids = BTreeSet::new();
        for layout in &self.layouts {
            let field = format!("panel.layouts[{}]", layout.id);
            if !valid_label(&layout.id, 64)
                || !ids.insert(&layout.id)
                || !valid_label(&layout.name, 80)
            {
                errors.push(ConfigError(format!(
                    "{field}: invalid or duplicate ID or empty name"
                )));
            }
            if let Err(error) = layout.geometry.validate() {
                errors.push(ConfigError(format!("{field}: {error}")));
            }
            for region in &layout.geometry.regions {
                if let Some(action) = &region.action
                    && let Err(error) = action.resolve()
                {
                    errors.push(ConfigError(format!(
                        "{field}.regions[{}]: {error}",
                        region.id
                    )));
                }
            }
            if let Some(binding) = &layout.select {
                rules.extend(resolve_mode_rule(
                    format!("{field}.select"),
                    binding,
                    Action::Numpad(NumpadAction::Select(layout.id.clone())),
                    emergency,
                    errors,
                ));
            }
        }
        if self
            .default_layout
            .as_ref()
            .map_or(!self.layouts.is_empty(), |id| !ids.contains(id))
        {
            errors.push(ConfigError(
                "panel.default_layout must identify a saved layout or be null for an empty panel"
                    .into(),
            ));
        }
        rules
    }
}

use inputflow_engine::{Action, Key, NumpadAction, Rule};
use serde::{Deserialize, Serialize};

use crate::config::{ConfigError, KeyConfig, TriggerConfig, resolve_mode_rule};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NumpadGuide {
    Always,
    TwoSeconds,
    #[default]
    UntilFirstInput,
    Never,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct NumpadConfig {
    pub enabled: bool,
    pub start: TriggerConfig,
    pub stop: TriggerConfig,
    pub guide: NumpadGuide,
}

impl Default for NumpadConfig {
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

impl NumpadConfig {
    pub(crate) fn resolved_rules(
        &self,
        emergency: Option<Key>,
        errors: &mut Vec<ConfigError>,
    ) -> Vec<Rule> {
        [
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
        .collect()
    }
}

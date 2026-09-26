//! Configuration schema, validation, loading, and atomic saving.
//!
//! The on-disk format is a single JSON object with a `schema_version`, an
//! `emergency_bypass_key`, and a list of `rules`. Key and mouse-button names use
//! the canonical strings from `inputflow_engine::{Key, MouseButton}` (e.g.
//! `"LeftCtrl"`, `"Right"`).

use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use inputflow_engine::{Action, Key, MouseButton, Rule, RuleIndex, Trigger};
use serde::{Deserialize, Serialize};

/// Current on-disk schema version.
pub const SCHEMA_VERSION: u32 = 1;
/// Default emergency bypass key used when no config is present.
pub const DEFAULT_EMERGENCY_KEY: Key = Key::F12;
/// Lower bound (inclusive) for a rule's `timeout_ms`.
pub const MIN_TIMEOUT_MS: u64 = 1;
/// Upper bound (inclusive) for a rule's `timeout_ms`.
pub const MAX_TIMEOUT_MS: u64 = 60_000;

/// An on-disk configuration document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[serde(default = "default_emergency_key_name")]
    pub emergency_bypass_key: String,
    #[serde(default)]
    pub rules: Vec<RuleConfig>,
}

/// A single rule in its on-disk (string-key) form.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleConfig {
    pub id: String,
    pub trigger: TriggerConfig,
    pub action: ActionConfig,
}

/// Trigger in its on-disk form, tagged by `type`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, tag = "type", rename_all = "snake_case")]
pub enum TriggerConfig {
    KeyChord { first: String, second: String },
    KeyMouseButton { key: String, button: String },
    Hold { key: String, timeout_ms: u64 },
    HoldMouseButton {
        key: String,
        timeout_ms: u64,
        button: String,
    },
}

/// Action in its on-disk form, tagged by `type`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, tag = "type", rename_all = "snake_case")]
pub enum ActionConfig {
    KeyChord { keys: Vec<String> },
}

/// A single validation problem (non-fatal; loading falls back to defaults).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError(pub String);

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// The result of loading and validating a config: a usable rule set plus any
/// non-fatal problems (a missing/invalid config yields empty rules).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedConfig {
    pub emergency_key: Key,
    pub rules: Vec<Rule>,
    pub problems: Vec<String>,
}

impl Default for LoadedConfig {
    fn default() -> Self {
        Self {
            emergency_key: DEFAULT_EMERGENCY_KEY,
            rules: Vec::new(),
            problems: Vec::new(),
        }
    }
}

fn default_schema_version() -> u32 {
    SCHEMA_VERSION
}

fn default_emergency_key_name() -> String {
    DEFAULT_EMERGENCY_KEY.to_string()
}
/// Load and validate the config at `path`. Never fails: on any error it returns
/// a default (empty rules) config and records the problems.
pub fn load(path: &Path) -> LoadedConfig {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return LoadedConfig {
                problems: vec![format!(
                    "config file not found at `{}`; starting with no rules (bypass)",
                    path.display()
                )],
                ..LoadedConfig::default()
            };
        }
        Err(err) => {
            return LoadedConfig {
                problems: vec![format!(
                    "failed to read config `{}`: {err}; starting with no rules (bypass)",
                    path.display()
                )],
                ..LoadedConfig::default()
            };
        }
    };

    let config: Config = match serde_json::from_str(&text) {
        Ok(config) => config,
        Err(err) => {
            return LoadedConfig {
                problems: vec![format!(
                    "config `{}` is not valid JSON: {err}; starting with no rules (bypass)",
                    path.display()
                )],
                ..LoadedConfig::default()
            };
        }
    };

    match validate(&config) {
        Ok(resolved) => LoadedConfig {
            emergency_key: resolved.emergency_key,
            rules: resolved.rules,
            problems: Vec::new(),
        },
        Err(errors) => LoadedConfig {
            problems: std::iter::once(format!(
                "config `{}` is invalid ({n} problem(s)); starting with no rules (bypass)",
                path.display(),
                n = errors.len()
            ))
            .chain(errors.into_iter().map(|e| e.0))
            .collect(),
            ..LoadedConfig::default()
        },
    }
}
/// The validated, resolved form of a config.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Resolved {
    emergency_key: Key,
    rules: Vec<Rule>,
}

/// Validate a parsed config and resolve it into engine rules. Returns all
/// problems at once.
fn validate(config: &Config) -> Result<Resolved, Vec<ConfigError>> {
    let mut errors: Vec<ConfigError> = Vec::new();

    if config.schema_version != SCHEMA_VERSION {
        errors.push(ConfigError(format!(
            "unsupported schema_version {} (expected {SCHEMA_VERSION})",
            config.schema_version
        )));
    }

    let emergency_key = match Key::from_name(&config.emergency_bypass_key) {
        Some(key) => key,
        None => {
            errors.push(ConfigError(format!(
                "unknown emergency_bypass_key `{}`",
                config.emergency_bypass_key
            )));
            DEFAULT_EMERGENCY_KEY
        }
    };

    let mut rules = Vec::with_capacity(config.rules.len());
    for (i, rule) in config.rules.iter().enumerate() {
        match resolve_rule(rule) {
            Ok(resolved) => rules.push(resolved),
            Err(err) => errors.push(ConfigError(format!("rules[{i}]: {err}"))),
        }
    }

    // The emergency key must not be a candidate prefix of any rule, otherwise
    // pressing it would toggle bypass instead of starting (or completing) a rule.
    let mut prefix_keys: BTreeSet<Key> = BTreeSet::new();
    for rule in &rules {
        let prefix = match rule.trigger {
            Trigger::KeyChord { first, .. } => Some(first),
            Trigger::KeyMouseButton { key, .. } => Some(key),
            Trigger::Hold { key, .. } => Some(key),
            Trigger::HoldMouseButton { key, .. } => Some(key),
        };
        if let Some(key) = prefix {
            prefix_keys.insert(key);
        }
    }
    if prefix_keys.contains(&emergency_key) {
        errors.push(ConfigError(format!(
            "emergency_bypass_key `{emergency_key}` collides with a rule prefix key"
        )));
    }

    // Reuse the engine's rule-index validation (duplicate ids/triggers and
    // cross-kind prefix conflicts).
    if let Err(rule_errors) = RuleIndex::compile(rules.clone()) {
        for rule_error in rule_errors {
            errors.push(ConfigError(rule_error.to_string()));
        }
    }

    if errors.is_empty() {
        Ok(Resolved {
            emergency_key,
            rules,
        })
    } else {
        Err(errors)
    }
}

/// Resolve one on-disk rule into an engine [`Rule`].
fn resolve_rule(rule: &RuleConfig) -> Result<Rule, String> {
    if rule.id.trim().is_empty() {
        return Err("rule has an empty id".to_string());
    }

    let trigger = match &rule.trigger {
        TriggerConfig::KeyChord { first, second } => Trigger::KeyChord {
            first: parse_key(first)?,
            second: parse_key(second)?,
        },
        TriggerConfig::KeyMouseButton { key, button } => Trigger::KeyMouseButton {
            key: parse_key(key)?,
            button: parse_button(button)?,
        },
        TriggerConfig::Hold { key, timeout_ms } => Trigger::Hold {
            key: parse_key(key)?,
            timeout_ms: check_timeout(*timeout_ms)?,
        },
        TriggerConfig::HoldMouseButton {
            key,
            timeout_ms,
            button,
        } => Trigger::HoldMouseButton {
            key: parse_key(key)?,
            timeout_ms: check_timeout(*timeout_ms)?,
            button: parse_button(button)?,
        },
    };

    let action = match &rule.action {
        ActionConfig::KeyChord { keys } => {
            if keys.is_empty() {
                return Err("action has no keys".to_string());
            }
            let mut parsed = Vec::with_capacity(keys.len());
            for key in keys {
                parsed.push(parse_key(key)?);
            }
            Action::KeyChord(parsed)
        }
    };

    Ok(Rule {
        id: rule.id.clone(),
        trigger,
        action,
    })
}

fn parse_key(name: &str) -> Result<Key, String> {
    Key::from_name(name).ok_or_else(|| format!("unknown key `{name}`"))
}

fn parse_button(name: &str) -> Result<MouseButton, String> {
    MouseButton::from_name(name).ok_or_else(|| format!("unknown mouse button `{name}`"))
}

fn check_timeout(timeout_ms: u64) -> Result<u64, String> {
    if (MIN_TIMEOUT_MS..=MAX_TIMEOUT_MS).contains(&timeout_ms) {
        Ok(timeout_ms)
    } else {
        Err(format!(
            "timeout_ms {timeout_ms} out of range [{MIN_TIMEOUT_MS}, {MAX_TIMEOUT_MS}]"
        ))
    }
}
/// Atomically (best-effort) write `config` to `path`: write a temp file, flush
/// and sync it, then rename it over the target. Creates parent directories.
pub fn save(path: &Path, config: &Config) -> Result<(), String> {
    let text = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;

    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let tmp = PathBuf::from(format!("{}.tmp", path.display()));
    let mut file = fs::File::create(&tmp).map_err(|e| e.to_string())?;
    file.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
    file.flush().map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    drop(file);

    match fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(rename_err) => {
            // On Windows `rename` may not overwrite an existing target; fall back
            // to remove-then-rename (best-effort atomicity).
            if path.exists() {
                fs::remove_file(path).map_err(|e| e.to_string())?;
                fs::rename(&tmp, path).map_err(|e| e.to_string())?;
                Ok(())
            } else {
                Err(format!("failed to move config into place: {rename_err}"))
            }
        }
    }
}

/// A default config: schema version 1, default emergency key, no rules.
pub fn default_config() -> Config {
    Config {
        schema_version: SCHEMA_VERSION,
        emergency_bypass_key: DEFAULT_EMERGENCY_KEY.to_string(),
        rules: Vec::new(),
    }
}

/// Serialize a config to pretty-printed JSON (for printing a template).
pub fn to_json_pretty(config: &Config) -> String {
    serde_json::to_string_pretty(config).unwrap_or_else(|e| format!("serialization error: {e}"))
}
#[cfg(test)]
mod tests {
    use super::*;

    fn sample_config() -> Config {
        Config {
            schema_version: SCHEMA_VERSION,
            emergency_bypass_key: "F12".to_string(),
            rules: vec![RuleConfig {
                id: "hold-ctrl-right-click-copy".to_string(),
                trigger: TriggerConfig::HoldMouseButton {
                    key: "LeftCtrl".to_string(),
                    timeout_ms: 250,
                    button: "Right".to_string(),
                },
                action: ActionConfig::KeyChord {
                    keys: vec!["LeftCtrl".to_string(), "C".to_string()],
                },
            }],
        }
    }

    fn load_from(text: &str) -> LoadedConfig {
        let dir = std::env::temp_dir().join(format!("inputflow-config-str-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        fs::write(&path, text).unwrap();
        let loaded = load(&path);
        let _ = fs::remove_dir_all(&dir);
        loaded
    }

    #[test]
    fn valid_config_resolves_to_engine_rules() {
        let loaded = load_from(&serde_json::to_string(&sample_config()).unwrap());
        assert!(loaded.problems.is_empty());
        assert_eq!(loaded.emergency_key, Key::F12);
        assert_eq!(loaded.rules.len(), 1);
        assert_eq!(
            loaded.rules[0].trigger,
            Trigger::HoldMouseButton {
                key: Key::LeftCtrl,
                timeout_ms: 250,
                button: MouseButton::Right,
            }
        );
    }

    #[test]
    fn unknown_key_is_rejected() {
        let mut config = sample_config();
        config.rules[0].trigger = TriggerConfig::Hold {
            key: "NotAKey".to_string(),
            timeout_ms: 250,
        };
        let problems = validate(&config).unwrap_err();
        assert!(problems.iter().any(|e| e.0.contains("unknown key `NotAKey`")));
    }

    #[test]
    fn timeout_out_of_range_is_rejected() {
        let mut config = sample_config();
        config.rules[0].trigger = TriggerConfig::Hold {
            key: "LeftCtrl".to_string(),
            timeout_ms: 0,
        };
        let problems = validate(&config).unwrap_err();
        assert!(problems.iter().any(|e| e.0.contains("out of range")));
    }

    #[test]
    fn duplicate_ids_are_rejected() {
        let mut config = sample_config();
        config.rules.push(RuleConfig {
            id: "hold-ctrl-right-click-copy".to_string(),
            trigger: TriggerConfig::Hold {
                key: "LeftCtrl".to_string(),
                timeout_ms: 250,
            },
            action: ActionConfig::KeyChord {
                keys: vec!["LeftCtrl".to_string(), "C".to_string()],
            },
        });
        let problems = validate(&config).unwrap_err();
        assert!(problems.iter().any(|e| e.0.contains("duplicate rule id")));
    }

    #[test]
    fn emergency_key_collision_is_rejected() {
        let mut config = sample_config();
        config.emergency_bypass_key = "LeftCtrl".to_string();
        let problems = validate(&config).unwrap_err();
        assert!(problems
            .iter()
            .any(|e| e.0.contains("collides with a rule prefix key")));
    }
    #[test]
    fn rule_conflict_is_rejected() {
        let mut config = sample_config();
        config.rules.push(RuleConfig {
            id: "hold-ctrl-copy".to_string(),
            trigger: TriggerConfig::Hold {
                key: "LeftCtrl".to_string(),
                timeout_ms: 250,
            },
            action: ActionConfig::KeyChord {
                keys: vec!["LeftCtrl".to_string(), "C".to_string()],
            },
        });
        let problems = validate(&config).unwrap_err();
        assert!(problems
            .iter()
            .any(|e| e.0.contains("different kinds share prefix key")));
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = std::env::temp_dir().join(format!("inputflow-config-test-{}", std::process::id()));
        let path = dir.join("config.json");
        let config = sample_config();

        save(&path, &config).expect("save should succeed");
        let loaded = load(&path);
        assert!(loaded.problems.is_empty());
        assert_eq!(loaded.emergency_key, Key::F12);
        assert_eq!(loaded.rules, validate(&config).unwrap().rules);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_missing_file_returns_default() {
        let path = std::env::temp_dir().join(format!(
            "inputflow-config-missing-{}-{}.json",
            std::process::id(),
            line!()
        ));
        let loaded = load(&path);
        assert!(loaded.rules.is_empty());
        assert_eq!(loaded.emergency_key, DEFAULT_EMERGENCY_KEY);
        assert!(!loaded.problems.is_empty());
    }

    #[test]
    fn load_invalid_json_returns_default() {
        let dir = std::env::temp_dir().join(format!("inputflow-config-bad-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        fs::write(&path, b"{ not valid json ").unwrap();

        let loaded = load(&path);
        assert!(loaded.rules.is_empty());
        assert!(!loaded.problems.is_empty());

        let _ = fs::remove_dir_all(&dir);
    }
}






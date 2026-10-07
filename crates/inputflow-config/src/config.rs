//! Configuration schema, validation, loading, and crash-recoverable saving.
//!
//! The on-disk format is a single JSON object with a `schema_version`, an
//! `emergency_bypass_key`, and a list of `rules`. Schema v2 makes every key
//! identity explicit: logical keys use canonical names and physical keys use a
//! scan code plus extended flag. Schema v3 adds persistent per-rule enablement;
//! schema v4 adds keyboard-activated mouse direction triggers. Schema v1/v2/v3
//! documents remain readable and are migrated in memory; all writes use v4.

use std::collections::BTreeSet;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use inputflow_engine::{Action, Key, MouseButton, MouseDirection, Rule, RuleIndex, Trigger};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Current on-disk schema version.
pub const SCHEMA_VERSION: u32 = 4;
/// Old string-key schema accepted for compatibility and explicit migration.
pub const LEGACY_SCHEMA_VERSION: u32 = 1;
/// Explicit-key schema accepted for compatibility.
pub const KEY_IDENTITY_SCHEMA_VERSION: u32 = 2;
/// Previous enablement schema accepted for compatibility.
pub const PREVIOUS_SCHEMA_VERSION: u32 = 3;
/// Default emergency bypass key used when no config is present.
pub const DEFAULT_EMERGENCY_KEY: Key = Key::F12;
/// Lower bound (inclusive) for a rule's `timeout_ms`.
pub const MIN_TIMEOUT_MS: u64 = 1;
/// Upper bound (inclusive) for a rule's `timeout_ms`.
pub const MAX_TIMEOUT_MS: u64 = 60_000;
/// Default values offered by the settings UI for mouse-direction rules.
pub const DEFAULT_DIRECTION_MIN_DISTANCE_PX: u32 = 80;
pub const DEFAULT_DIRECTION_MAX_DURATION_MS: u64 = 500;
pub const DEFAULT_DIRECTION_OFF_AXIS_TOLERANCE_PX: u32 = 40;
pub const MIN_DIRECTION_DISTANCE_PX: u32 = 10;
pub const MAX_DIRECTION_DISTANCE_PX: u32 = 2_000;
pub const MIN_DIRECTION_DURATION_MS: u64 = 100;
pub const MAX_DIRECTION_DURATION_MS: u64 = 5_000;
pub const MAX_DIRECTION_OFF_AXIS_TOLERANCE_PX: u32 = 2_000;
/// Number of previously committed configurations retained after successful
/// replacements. Five generations cost little for the current small JSON file
/// while bounding autosave growth.
const RECOVERY_BACKUP_LIMIT: usize = 5;
/// Number of uncommitted save attempts retained. These are lower-priority than
/// committed backups during recovery.
const RECOVERY_TEMP_LIMIT: usize = 3;

/// Serialize commits within this process. Unique artifact names prevent file
/// clobbering, while this lock makes the intended last-writer-wins order clear.
static SAVE_LOCK: Mutex<()> = Mutex::new(());
static SAVE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// An on-disk configuration document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema_version: u32,
    pub emergency_bypass_key: KeyConfig,
    #[serde(default)]
    pub rules: Vec<RuleConfig>,
}

/// Explicit key match/output identity used by schema v2.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, tag = "match", rename_all = "snake_case")]
pub enum KeyConfig {
    /// Match/output the logical Windows virtual-key identity.
    Logical { key: String },
    /// Match/output a layout-independent physical scan position.
    Physical { scan_code: u16, extended: bool },
}

impl KeyConfig {
    pub fn logical(key: Key) -> Self {
        Self::Logical {
            key: key.to_string(),
        }
    }

    pub const fn physical(scan_code: u16, extended: bool) -> Self {
        Self::Physical {
            scan_code,
            extended,
        }
    }
}

/// A single rule in its current-schema form.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleConfig {
    pub id: String,
    pub enabled: bool,
    pub trigger: TriggerConfig,
    pub action: ActionConfig,
}

/// Trigger in its on-disk form, tagged by `type`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, tag = "type", rename_all = "snake_case")]
pub enum TriggerConfig {
    KeyChord {
        first: KeyConfig,
        second: KeyConfig,
    },
    KeyMouseButton {
        key: KeyConfig,
        button: String,
    },
    Hold {
        key: KeyConfig,
        timeout_ms: u64,
    },
    HoldMouseButton {
        key: KeyConfig,
        timeout_ms: u64,
        button: String,
    },
    MouseDirection {
        key: KeyConfig,
        direction: DirectionConfig,
        min_distance_px: u32,
        max_duration_ms: u64,
        off_axis_tolerance_px: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DirectionConfig {
    Left,
    Right,
    Up,
    Down,
}

/// Action in its on-disk form, tagged by `type`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, tag = "type", rename_all = "snake_case")]
pub enum ActionConfig {
    KeyChord { keys: Vec<KeyConfig> },
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigV1 {
    #[serde(default = "legacy_schema_version")]
    schema_version: u32,
    #[serde(default = "legacy_emergency_key_name")]
    emergency_bypass_key: String,
    #[serde(default)]
    rules: Vec<RuleConfigV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuleConfigV1 {
    id: String,
    trigger: TriggerConfigV1,
    action: ActionConfigV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigV2 {
    schema_version: u32,
    emergency_bypass_key: KeyConfig,
    #[serde(default)]
    rules: Vec<RuleConfigV2>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigV3 {
    schema_version: u32,
    emergency_bypass_key: KeyConfig,
    #[serde(default)]
    rules: Vec<RuleConfigV3>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuleConfigV3 {
    id: String,
    enabled: bool,
    trigger: TriggerConfigV3,
    action: ActionConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, tag = "type", rename_all = "snake_case")]
enum TriggerConfigV3 {
    KeyChord {
        first: KeyConfig,
        second: KeyConfig,
    },
    KeyMouseButton {
        key: KeyConfig,
        button: String,
    },
    Hold {
        key: KeyConfig,
        timeout_ms: u64,
    },
    HoldMouseButton {
        key: KeyConfig,
        timeout_ms: u64,
        button: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuleConfigV2 {
    id: String,
    trigger: TriggerConfigV3,
    action: ActionConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, tag = "type", rename_all = "snake_case")]
enum TriggerConfigV1 {
    KeyChord {
        first: String,
        second: String,
    },
    KeyMouseButton {
        key: String,
        button: String,
    },
    Hold {
        key: String,
        timeout_ms: u64,
    },
    HoldMouseButton {
        key: String,
        timeout_ms: u64,
        button: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, tag = "type", rename_all = "snake_case")]
enum ActionConfigV1 {
    KeyChord { keys: Vec<String> },
}

const fn legacy_schema_version() -> u32 {
    LEGACY_SCHEMA_VERSION
}

fn legacy_emergency_key_name() -> String {
    DEFAULT_EMERGENCY_KEY.to_string()
}

/// Result of parsing any supported schema into the current v4 document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationReport {
    pub source_schema_version: u32,
    pub config: Config,
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
    /// Current-schema document, migrated in memory when the source was v1.
    pub config: Config,
    pub source_schema_version: u32,
    pub emergency_key: Key,
    pub rules: Vec<Rule>,
    pub problems: Vec<String>,
}

/// A validated current-schema document resolved into runtime identities.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedConfig {
    pub config: Config,
    pub emergency_key: Key,
    pub rules: Vec<Rule>,
}

/// Structured result of an atomic configuration commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveReport {
    pub path: PathBuf,
    pub backup_path: Option<PathBuf>,
    pub cleanup_warnings: Vec<String>,
}

impl Default for LoadedConfig {
    fn default() -> Self {
        Self {
            config: default_config(),
            source_schema_version: SCHEMA_VERSION,
            emergency_key: DEFAULT_EMERGENCY_KEY,
            rules: Vec::new(),
            problems: Vec::new(),
        }
    }
}

/// Load and validate the config at `path`. Never fails: on any error it returns
/// a default (empty rules) config and records the problems.
pub fn load(path: &Path) -> LoadedConfig {
    let primary_errors = match load_resolved(path) {
        Ok(resolved) => {
            let problems = migration_notice(resolved.source_schema_version)
                .into_iter()
                .collect();
            return LoadedConfig {
                config: resolved.config,
                source_schema_version: resolved.source_schema_version,
                emergency_key: resolved.emergency_key,
                rules: resolved.rules,
                problems,
            };
        }
        Err(errors) => errors,
    };
    for candidate in recovery_candidates(path) {
        if let Ok(resolved) = load_resolved(&candidate) {
            let artifact_kind = if recovery_kind(path, &candidate) == Some(RecoveryKind::Backup) {
                "committed backup"
            } else {
                "uncommitted temporary save"
            };
            let mut problems = vec![format!(
                "config `{}` could not be used; recovered a valid {artifact_kind} from `{}`. Save once to make the recovery official. Primary error: {}",
                path.display(),
                candidate.display(),
                primary_errors.join("; ")
            )];
            if let Some(notice) = migration_notice(resolved.source_schema_version) {
                problems.push(notice);
            }
            return LoadedConfig {
                config: resolved.config,
                source_schema_version: resolved.source_schema_version,
                emergency_key: resolved.emergency_key,
                rules: resolved.rules,
                problems,
            };
        }
    }

    LoadedConfig {
        problems: primary_errors
            .into_iter()
            .chain(std::iter::once(
                "no valid recovery artifact was found; starting with no rules (bypass)".to_string(),
            ))
            .collect(),
        ..LoadedConfig::default()
    }
}

fn load_resolved(path: &Path) -> Result<Resolved, Vec<String>> {
    let text = fs::read_to_string(path).map_err(|err| {
        vec![if err.kind() == io::ErrorKind::NotFound {
            format!("config file not found at `{}`", path.display())
        } else {
            format!("failed to read config `{}`: {err}", path.display())
        }]
    })?;

    let migrated = parse_and_migrate_json(&text).map_err(|errors| {
        std::iter::once(format!("config `{}` is invalid", path.display()))
            .chain(errors.into_iter().map(|error| error.0))
            .collect::<Vec<_>>()
    })?;

    validate(&migrated.config)
        .map_err(|errors| {
            std::iter::once(format!(
                "config `{}` is invalid ({} problem(s))",
                path.display(),
                errors.len()
            ))
            .chain(errors.into_iter().map(|error| error.0))
            .collect()
        })
        .map(|mut resolved| {
            resolved.config = migrated.config;
            resolved.source_schema_version = migrated.source_schema_version;
            resolved
        })
}

fn migration_notice(source_schema_version: u32) -> Option<String> {
    (source_schema_version != SCHEMA_VERSION).then(|| {
        format!(
            "loaded schema v{source_schema_version} in compatibility mode and migrated it in memory to schema v{SCHEMA_VERSION}; save the v{SCHEMA_VERSION} document to make the migration official (the replacement backup preserves rollback)"
        )
    })
}

/// Parse a v1, v2, v3, or v4 JSON document, reject unknown/unsupported structures, and
/// return a validated current-schema document without writing to disk.
pub fn parse_and_migrate_json(text: &str) -> Result<MigrationReport, Vec<ConfigError>> {
    let value: Value = serde_json::from_str(text)
        .map_err(|error| vec![ConfigError(format!("invalid JSON: {error}"))])?;
    let source_schema_version = match value.get("schema_version") {
        None => LEGACY_SCHEMA_VERSION,
        Some(Value::Number(number)) => number
            .as_u64()
            .and_then(|number| u32::try_from(number).ok())
            .ok_or_else(|| {
                vec![ConfigError(
                    "schema_version must be an unsigned 32-bit integer".to_string(),
                )]
            })?,
        Some(_) => {
            return Err(vec![ConfigError(
                "schema_version must be an unsigned 32-bit integer".to_string(),
            )]);
        }
    };

    let config = match source_schema_version {
        LEGACY_SCHEMA_VERSION => {
            let legacy: ConfigV1 = serde_json::from_value(value).map_err(|error| {
                vec![ConfigError(format!("invalid schema v1 document: {error}"))]
            })?;
            migrate_v1(legacy)
        }
        KEY_IDENTITY_SCHEMA_VERSION => {
            let previous: ConfigV2 = serde_json::from_value(value).map_err(|error| {
                vec![ConfigError(format!("invalid schema v2 document: {error}"))]
            })?;
            migrate_v2(previous)
        }
        PREVIOUS_SCHEMA_VERSION => {
            let previous: ConfigV3 = serde_json::from_value(value).map_err(|error| {
                vec![ConfigError(format!("invalid schema v3 document: {error}"))]
            })?;
            migrate_v3(previous)
        }
        SCHEMA_VERSION => serde_json::from_value(value)
            .map_err(|error| vec![ConfigError(format!("invalid schema v4 document: {error}"))])?,
        other => {
            return Err(vec![ConfigError(format!(
                "unsupported schema_version {other} (supported: {LEGACY_SCHEMA_VERSION}, {KEY_IDENTITY_SCHEMA_VERSION}, {PREVIOUS_SCHEMA_VERSION}, {SCHEMA_VERSION})"
            ))]);
        }
    };

    validate(&config)?;
    Ok(MigrationReport {
        source_schema_version,
        config,
    })
}

fn migrate_v1(legacy: ConfigV1) -> Config {
    debug_assert_eq!(legacy.schema_version, LEGACY_SCHEMA_VERSION);
    Config {
        schema_version: SCHEMA_VERSION,
        emergency_bypass_key: KeyConfig::Logical {
            key: legacy.emergency_bypass_key,
        },
        rules: legacy
            .rules
            .into_iter()
            .map(|rule| RuleConfig {
                id: rule.id,
                enabled: true,
                trigger: match rule.trigger {
                    TriggerConfigV1::KeyChord { first, second } => TriggerConfig::KeyChord {
                        first: KeyConfig::Logical { key: first },
                        second: KeyConfig::Logical { key: second },
                    },
                    TriggerConfigV1::KeyMouseButton { key, button } => {
                        TriggerConfig::KeyMouseButton {
                            key: KeyConfig::Logical { key },
                            button,
                        }
                    }
                    TriggerConfigV1::Hold { key, timeout_ms } => TriggerConfig::Hold {
                        key: KeyConfig::Logical { key },
                        timeout_ms,
                    },
                    TriggerConfigV1::HoldMouseButton {
                        key,
                        timeout_ms,
                        button,
                    } => TriggerConfig::HoldMouseButton {
                        key: KeyConfig::Logical { key },
                        timeout_ms,
                        button,
                    },
                },
                action: match rule.action {
                    ActionConfigV1::KeyChord { keys } => ActionConfig::KeyChord {
                        keys: keys
                            .into_iter()
                            .map(|key| KeyConfig::Logical { key })
                            .collect(),
                    },
                },
            })
            .collect(),
    }
}

fn migrate_v2(previous: ConfigV2) -> Config {
    debug_assert_eq!(previous.schema_version, KEY_IDENTITY_SCHEMA_VERSION);
    Config {
        schema_version: SCHEMA_VERSION,
        emergency_bypass_key: previous.emergency_bypass_key,
        rules: previous
            .rules
            .into_iter()
            .map(|rule| RuleConfig {
                id: rule.id,
                enabled: true,
                trigger: migrate_pre_v4_trigger(rule.trigger),
                action: rule.action,
            })
            .collect(),
    }
}

fn migrate_v3(previous: ConfigV3) -> Config {
    debug_assert_eq!(previous.schema_version, PREVIOUS_SCHEMA_VERSION);
    Config {
        schema_version: SCHEMA_VERSION,
        emergency_bypass_key: previous.emergency_bypass_key,
        rules: previous
            .rules
            .into_iter()
            .map(|rule| RuleConfig {
                id: rule.id,
                enabled: rule.enabled,
                trigger: migrate_pre_v4_trigger(rule.trigger),
                action: rule.action,
            })
            .collect(),
    }
}

fn migrate_pre_v4_trigger(trigger: TriggerConfigV3) -> TriggerConfig {
    match trigger {
        TriggerConfigV3::KeyChord { first, second } => TriggerConfig::KeyChord { first, second },
        TriggerConfigV3::KeyMouseButton { key, button } => {
            TriggerConfig::KeyMouseButton { key, button }
        }
        TriggerConfigV3::Hold { key, timeout_ms } => TriggerConfig::Hold { key, timeout_ms },
        TriggerConfigV3::HoldMouseButton {
            key,
            timeout_ms,
            button,
        } => TriggerConfig::HoldMouseButton {
            key,
            timeout_ms,
            button,
        },
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RecoveryKind {
    Backup,
    Temp,
}

#[derive(Debug)]
struct RecoveryArtifact {
    kind: RecoveryKind,
    modified: std::time::SystemTime,
    path: PathBuf,
}

fn recovery_kind(path: &Path, candidate: &Path) -> Option<RecoveryKind> {
    let file_name = path.file_name()?.to_string_lossy();
    let candidate_name = candidate.file_name()?.to_string_lossy();
    if candidate_name.starts_with(&format!("{file_name}.bak.")) {
        Some(RecoveryKind::Backup)
    } else if candidate_name.starts_with(&format!("{file_name}.tmp.")) {
        Some(RecoveryKind::Temp)
    } else {
        None
    }
}

fn recovery_artifacts(path: &Path) -> Vec<RecoveryArtifact> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut artifacts: Vec<RecoveryArtifact> = fs::read_dir(parent)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let kind = recovery_kind(path, &entry.path())?;
            let modified = entry
                .metadata()
                .and_then(|metadata| metadata.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            Some(RecoveryArtifact {
                kind,
                modified,
                path: entry.path(),
            })
        })
        .collect();
    artifacts.sort_by(|a, b| {
        let priority = |kind| match kind {
            RecoveryKind::Backup => 0,
            RecoveryKind::Temp => 1,
        };
        priority(a.kind)
            .cmp(&priority(b.kind))
            .then_with(|| b.modified.cmp(&a.modified))
            .then_with(|| b.path.cmp(&a.path))
    });
    artifacts
}

/// Return committed backups newest-first, followed by uncommitted temporary
/// saves newest-first. They are siblings so replacement never crosses volumes.
fn recovery_candidates(path: &Path) -> Vec<PathBuf> {
    recovery_artifacts(path)
        .into_iter()
        .map(|artifact| artifact.path)
        .collect()
}
/// The validated, resolved form of a config.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Resolved {
    config: Config,
    source_schema_version: u32,
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

    let emergency_key = match parse_key(&config.emergency_bypass_key) {
        Ok(Key::Physical { .. }) => {
            errors.push(ConfigError(
                "emergency_bypass_key must use logical match mode".to_string(),
            ));
            DEFAULT_EMERGENCY_KEY
        }
        Ok(key) => key,
        Err(error) => {
            errors.push(ConfigError(format!(
                "invalid emergency_bypass_key: {error}"
            )));
            DEFAULT_EMERGENCY_KEY
        }
    };

    let mut rules = Vec::with_capacity(config.rules.len());
    let mut rule_ids = BTreeSet::new();
    for (i, rule) in config.rules.iter().enumerate() {
        if !rule.id.trim().is_empty() && !rule_ids.insert(rule.id.as_str()) {
            errors.push(ConfigError(format!(
                "rules[{i}]: duplicate rule id `{}`",
                rule.id
            )));
        }
        match resolve_rule(rule) {
            Ok(resolved) if rule.enabled => rules.push(resolved),
            Ok(_) => {}
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
            Trigger::MouseDirection { key, .. } => Some(key),
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
            config: config.clone(),
            source_schema_version: SCHEMA_VERSION,
            emergency_key,
            rules,
        })
    } else {
        Err(errors)
    }
}

/// Validate and resolve a schema-v4 document without reading or writing disk.
pub fn validate_config(config: &Config) -> Result<ValidatedConfig, Vec<ConfigError>> {
    validate(config).map(|resolved| ValidatedConfig {
        config: resolved.config,
        emergency_key: resolved.emergency_key,
        rules: resolved.rules,
    })
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
        TriggerConfig::Hold { key, timeout_ms } => {
            let key = parse_key(key)?;
            Trigger::Hold {
                key,
                timeout_ms: check_timeout(*timeout_ms)?,
            }
        }
        TriggerConfig::HoldMouseButton {
            key,
            timeout_ms,
            button,
        } => {
            let key = parse_key(key)?;
            Trigger::HoldMouseButton {
                key,
                timeout_ms: check_timeout(*timeout_ms)?,
                button: parse_button(button)?,
            }
        }
        TriggerConfig::MouseDirection {
            key,
            direction,
            min_distance_px,
            max_duration_ms,
            off_axis_tolerance_px,
        } => Trigger::MouseDirection {
            key: parse_key(key)?,
            direction: match direction {
                DirectionConfig::Left => MouseDirection::Left,
                DirectionConfig::Right => MouseDirection::Right,
                DirectionConfig::Up => MouseDirection::Up,
                DirectionConfig::Down => MouseDirection::Down,
            },
            min_distance_px: check_direction_distance(*min_distance_px)?,
            max_duration_ms: check_direction_duration(*max_duration_ms)?,
            off_axis_tolerance_px: check_direction_off_axis(*off_axis_tolerance_px)?,
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

fn parse_key(config: &KeyConfig) -> Result<Key, String> {
    match config {
        KeyConfig::Logical { key } => {
            Key::from_name(key).ok_or_else(|| format!("unknown logical key `{key}`"))
        }
        KeyConfig::Physical {
            scan_code,
            extended,
        } => Key::physical(*scan_code, *extended)
            .ok_or_else(|| "physical scan_code must be nonzero".to_string()),
    }
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

fn check_direction_distance(distance_px: u32) -> Result<u32, String> {
    if (MIN_DIRECTION_DISTANCE_PX..=MAX_DIRECTION_DISTANCE_PX).contains(&distance_px) {
        Ok(distance_px)
    } else {
        Err(format!(
            "min_distance_px {distance_px} out of range [{MIN_DIRECTION_DISTANCE_PX}, {MAX_DIRECTION_DISTANCE_PX}]"
        ))
    }
}

fn check_direction_duration(duration_ms: u64) -> Result<u64, String> {
    if (MIN_DIRECTION_DURATION_MS..=MAX_DIRECTION_DURATION_MS).contains(&duration_ms) {
        Ok(duration_ms)
    } else {
        Err(format!(
            "max_duration_ms {duration_ms} out of range [{MIN_DIRECTION_DURATION_MS}, {MAX_DIRECTION_DURATION_MS}]"
        ))
    }
}

fn check_direction_off_axis(tolerance_px: u32) -> Result<u32, String> {
    if tolerance_px <= MAX_DIRECTION_OFF_AXIS_TOLERANCE_PX {
        Ok(tolerance_px)
    } else {
        Err(format!(
            "off_axis_tolerance_px {tolerance_px} out of range [0, {MAX_DIRECTION_OFF_AXIS_TOLERANCE_PX}]"
        ))
    }
}
/// Write `config` to a unique, synced sibling and commit it without first
/// removing or renaming the official file. On Windows an existing file is
/// replaced with `ReplaceFileW`, which can also retain the old file as a unique
/// recovery backup. Failed commits deliberately keep their valid temp artifact.
pub fn save(path: &Path, config: &Config) -> Result<(), String> {
    save_with_report(path, config).map(|_| ())
}

/// Atomically save a validated document and retain post-commit cleanup
/// warnings for agent diagnostics instead of silently discarding them.
pub fn save_with_report(path: &Path, config: &Config) -> Result<SaveReport, String> {
    let _guard = SAVE_LOCK
        .lock()
        .map_err(|_| "config save lock is poisoned".to_string())?;
    save_with_committer(path, config, &SystemCommitter)
}

trait FileCommitter {
    fn replace_existing(&self, target: &Path, replacement: &Path, backup: &Path) -> io::Result<()>;
    fn promote_new(&self, replacement: &Path, target: &Path) -> io::Result<()>;
}

struct SystemCommitter;

impl FileCommitter for SystemCommitter {
    fn replace_existing(&self, target: &Path, replacement: &Path, backup: &Path) -> io::Result<()> {
        replace_existing_file(target, replacement, backup)
    }

    fn promote_new(&self, replacement: &Path, target: &Path) -> io::Result<()> {
        fs::rename(replacement, target)
    }
}

fn save_with_committer(
    path: &Path,
    config: &Config,
    committer: &dyn FileCommitter,
) -> Result<SaveReport, String> {
    if let Err(errors) = validate(config) {
        return Err(format!(
            "refusing to save invalid config: {}",
            errors
                .into_iter()
                .map(|error| error.0)
                .collect::<Vec<_>>()
                .join("; ")
        ));
    }
    let text = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;

    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    // Unique sibling paths plus create_new avoid clobbering artifacts from a
    // concurrent process or a previous crash.
    let tmp = temp_sibling(path, "tmp");
    let backup = temp_sibling(path, "bak");

    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)
        .map_err(|e| format!("failed to create `{}`: {e}", tmp.display()))?;
    file.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
    file.flush().map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    drop(file);

    let replaced_existing = path.exists();
    let commit = if replaced_existing {
        committer.replace_existing(path, &tmp, &backup)
    } else {
        committer.promote_new(&tmp, path)
    };
    if let Err(error) = commit {
        let cleanup_warnings =
            prune_recovery_artifacts(path, RecoveryKind::Temp, RECOVERY_TEMP_LIMIT);
        let cleanup_note = if cleanup_warnings.is_empty() {
            String::new()
        } else {
            format!("; cleanup warning: {}", cleanup_warnings.join("; "))
        };
        return Err(format!(
            "failed to commit config `{}`: {error}; recovery artifact retained at `{}`{cleanup_note}",
            path.display(),
            tmp.display()
        ));
    }
    // Retention is best-effort after the new primary has been committed. It
    // never changes a successful save into a reported failure; a future UI
    // should expose cleanup warnings through its diagnostics channel.
    let mut cleanup_warnings =
        prune_recovery_artifacts(path, RecoveryKind::Backup, RECOVERY_BACKUP_LIMIT);
    cleanup_warnings.extend(prune_recovery_artifacts(
        path,
        RecoveryKind::Temp,
        RECOVERY_TEMP_LIMIT,
    ));
    Ok(SaveReport {
        path: path.to_path_buf(),
        backup_path: replaced_existing.then_some(backup),
        cleanup_warnings,
    })
}

/// Retain the newest `limit` artifacts of one kind and, if they are all corrupt,
/// additionally protect the newest known-valid artifact. Therefore cleanup can
/// never remove the only valid recovery copy merely because it is older.
fn prune_recovery_artifacts(path: &Path, kind: RecoveryKind, limit: usize) -> Vec<String> {
    let artifacts: Vec<RecoveryArtifact> = recovery_artifacts(path)
        .into_iter()
        .filter(|artifact| artifact.kind == kind)
        .collect();
    let protected_valid = artifacts
        .iter()
        .find(|artifact| load_resolved(&artifact.path).is_ok())
        .map(|artifact| artifact.path.clone());
    let mut warnings = Vec::new();
    for (index, artifact) in artifacts.into_iter().enumerate() {
        if index < limit || protected_valid.as_ref() == Some(&artifact.path) {
            continue;
        }
        if let Err(error) = fs::remove_file(&artifact.path) {
            warnings.push(format!(
                "failed to remove `{}`: {error}",
                artifact.path.display()
            ));
        }
    }
    warnings
}

#[cfg(windows)]
fn replace_existing_file(target: &Path, replacement: &Path, backup: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::ReplaceFileW;

    fn wide(path: &Path) -> Vec<u16> {
        path.as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    let target = wide(target);
    let replacement = wide(replacement);
    let backup = wide(backup);
    // SAFETY: all three buffers are NUL-terminated and remain alive for the
    // duration of the call; reserved pointers are required to be null.
    let replaced = unsafe {
        ReplaceFileW(
            target.as_ptr(),
            replacement.as_ptr(),
            backup.as_ptr(),
            0,
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    if replaced == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn replace_existing_file(target: &Path, replacement: &Path, backup: &Path) -> io::Result<()> {
    fs::copy(target, backup)?;
    fs::File::open(backup)?.sync_all()?;
    fs::rename(replacement, target)
}

/// A sibling path with a process-and-sequence-qualified suffix.
fn temp_sibling(path: &Path, ext: &str) -> PathBuf {
    let mut name = path
        .file_name()
        .map(|s| s.to_os_string())
        .unwrap_or_else(|| "config".into());
    let sequence = SAVE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    name.push(format!(".{ext}.{}.{sequence}", std::process::id()));
    path.with_file_name(name)
}

/// A default config: current schema, logical F12 emergency key, no rules.
pub fn default_config() -> Config {
    Config {
        schema_version: SCHEMA_VERSION,
        emergency_bypass_key: KeyConfig::logical(DEFAULT_EMERGENCY_KEY),
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

    fn logical(key: &str) -> KeyConfig {
        KeyConfig::Logical {
            key: key.to_string(),
        }
    }

    fn sample_config() -> Config {
        Config {
            schema_version: SCHEMA_VERSION,
            emergency_bypass_key: logical("F12"),
            rules: vec![RuleConfig {
                id: "hold-ctrl-right-click-copy".to_string(),
                enabled: true,
                trigger: TriggerConfig::HoldMouseButton {
                    key: logical("LeftCtrl"),
                    timeout_ms: 250,
                    button: "Right".to_string(),
                },
                action: ActionConfig::KeyChord {
                    keys: vec![logical("LeftCtrl"), logical("C")],
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

    const V1_GOLDEN: &str = include_str!("../../../fixtures/config/v1-valid.json");
    const V2_GOLDEN: &str = include_str!("../../../fixtures/config/v2-valid.json");
    const V3_GOLDEN: &str = include_str!("../../../fixtures/config/v3-valid.json");
    const V4_GOLDEN: &str = include_str!("../../../fixtures/config/v4-valid.json");
    const DIRECTION_ACCEPTANCE: &str =
        include_str!("../../../scripts/acceptance/configs/mouse-direction-f8-four.json");

    #[test]
    fn v1_golden_migrates_without_changing_legacy_rule_meaning() {
        let migrated = parse_and_migrate_json(V1_GOLDEN).expect("v1 fixture should migrate");
        assert_eq!(migrated.source_schema_version, LEGACY_SCHEMA_VERSION);
        assert_eq!(migrated.config.schema_version, SCHEMA_VERSION);
        assert_eq!(migrated.config.emergency_bypass_key, logical("F12"));
        assert!(migrated.config.rules[0].enabled);

        let resolved = validate(&migrated.config).unwrap();
        assert_eq!(resolved.emergency_key, Key::F12);
        assert_eq!(resolved.rules.len(), 1);
        assert_eq!(
            resolved.rules[0].trigger,
            Trigger::KeyChord {
                first: Key::LeftCtrl,
                second: Key::C,
            }
        );
        assert_eq!(
            resolved.rules[0].action,
            Action::KeyChord(vec![Key::LeftCtrl, Key::C])
        );
    }

    #[test]
    fn v2_golden_migrates_to_v4_without_changing_key_identities() {
        let first = parse_and_migrate_json(V2_GOLDEN).expect("v2 fixture should parse");
        assert_eq!(first.source_schema_version, KEY_IDENTITY_SCHEMA_VERSION);
        assert_eq!(first.config.schema_version, SCHEMA_VERSION);
        assert!(first.config.rules[0].enabled);
        let text = serde_json::to_string_pretty(&first.config).unwrap();
        let second = parse_and_migrate_json(&text).expect("serialized v4 should parse");
        assert_eq!(second.source_schema_version, SCHEMA_VERSION);
        assert_eq!(second.config, first.config);

        let resolved = validate(&second.config).unwrap();
        assert_eq!(
            resolved.rules[0].trigger,
            Trigger::KeyChord {
                first: Key::Physical {
                    scan_code: 30,
                    extended: false,
                },
                second: Key::C,
            }
        );
    }

    #[test]
    fn v3_golden_migrates_enablement_to_v4() {
        let first = parse_and_migrate_json(V3_GOLDEN).expect("v3 fixture should parse");
        assert_eq!(first.source_schema_version, PREVIOUS_SCHEMA_VERSION);
        assert!(first.config.rules[0].enabled);

        let text = serde_json::to_string_pretty(&first.config).unwrap();
        let second = parse_and_migrate_json(&text).expect("serialized v4 should parse");
        assert_eq!(second.source_schema_version, SCHEMA_VERSION);
        assert_eq!(second.config, first.config);
    }

    #[test]
    fn v4_golden_round_trips_mouse_direction_groups() {
        let first = parse_and_migrate_json(V4_GOLDEN).expect("v4 fixture should parse");
        assert_eq!(first.source_schema_version, SCHEMA_VERSION);
        assert_eq!(first.config.rules.len(), 2);
        assert!(matches!(
            first.config.rules[0].trigger,
            TriggerConfig::MouseDirection {
                direction: DirectionConfig::Left,
                min_distance_px: 80,
                max_duration_ms: 500,
                off_axis_tolerance_px: 40,
                ..
            }
        ));

        let text = serde_json::to_string_pretty(&first.config).unwrap();
        let second = parse_and_migrate_json(&text).expect("serialized v4 should parse");
        assert_eq!(second, first);
    }

    #[test]
    fn phase_f_acceptance_config_is_a_valid_four_direction_group() {
        let parsed = parse_and_migrate_json(DIRECTION_ACCEPTANCE)
            .expect("Phase F acceptance config should remain valid");
        assert_eq!(parsed.source_schema_version, SCHEMA_VERSION);
        assert_eq!(parsed.config.rules.len(), 4);
        let directions = parsed
            .config
            .rules
            .iter()
            .map(|rule| match &rule.trigger {
                TriggerConfig::MouseDirection { direction, .. } => *direction,
                other => panic!("unexpected acceptance trigger: {other:?}"),
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(
            directions,
            BTreeSet::from([
                DirectionConfig::Left,
                DirectionConfig::Right,
                DirectionConfig::Up,
                DirectionConfig::Down,
            ])
        );
    }

    #[test]
    fn loading_v1_reports_compatibility_mode_and_exposes_v4_document() {
        let loaded = load_from(V1_GOLDEN);
        assert_eq!(loaded.source_schema_version, LEGACY_SCHEMA_VERSION);
        assert_eq!(loaded.config.schema_version, SCHEMA_VERSION);
        assert_eq!(loaded.rules.len(), 1);
        assert!(loaded.problems.iter().any(|problem| {
            problem.contains("schema v1") && problem.contains("compatibility mode")
        }));
    }

    #[test]
    fn failed_v1_migration_does_not_modify_the_source_file() {
        let dir = std::env::temp_dir().join(format!(
            "inputflow-config-bad-migration-{}-{}",
            std::process::id(),
            line!()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        let invalid = V1_GOLDEN.replace("\"C\"", "\"VendorMystery\"");
        fs::write(&path, &invalid).unwrap();

        let loaded = load(&path);
        assert!(loaded.rules.is_empty());
        assert!(
            loaded
                .problems
                .iter()
                .any(|problem| problem.contains("unknown logical key `VendorMystery`"))
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), invalid);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn supported_schemas_reject_unknown_fields_illegal_keys_and_mixed_shapes() {
        let v2 = V2_GOLDEN.replace("\r\n", "\n");
        let cases = [
            v2.replace("\"key\": \"F12\"", "\"key\": \"F12\", \"unexpected\": true"),
            v2.replace("\"key\": \"F12\"", "\"key\": \"VendorMystery\""),
            v2.replace("\"scan_code\": 30", "\"scan_code\": 0"),
            v2.replace(
                "{\n    \"match\": \"logical\",\n    \"key\": \"F12\"\n  }",
                "{\"match\":\"physical\",\"scan_code\":88,\"extended\":false}",
            ),
            v2.replace(
                "{\n          \"match\": \"logical\",\n          \"key\": \"C\"\n        }",
                "\"C\"",
            ),
        ];

        for text in cases {
            assert!(
                parse_and_migrate_json(&text).is_err(),
                "invalid v2 document was accepted: {text}"
            );
        }

        let missing_enabled = V3_GOLDEN
            .replace("\r\n", "\n")
            .replace("      \"enabled\": true,\n", "");
        assert!(
            parse_and_migrate_json(&missing_enabled).is_err(),
            "schema v3 must require an explicit enabled field"
        );
    }

    #[test]
    fn pre_v4_schemas_cannot_smuggle_mouse_direction_triggers() {
        let mut v3: Value = serde_json::from_str(V4_GOLDEN).unwrap();
        v3["schema_version"] = serde_json::json!(PREVIOUS_SCHEMA_VERSION);
        let v3_errors = parse_and_migrate_json(&serde_json::to_string(&v3).unwrap()).unwrap_err();
        assert!(
            v3_errors
                .iter()
                .any(|error| error.0.contains("mouse_direction")),
            "schema v3 must reject a v4-only direction trigger"
        );

        let mut v2 = v3;
        v2["schema_version"] = serde_json::json!(KEY_IDENTITY_SCHEMA_VERSION);
        for rule in v2["rules"].as_array_mut().unwrap() {
            rule.as_object_mut().unwrap().remove("enabled");
        }
        let v2_errors = parse_and_migrate_json(&serde_json::to_string(&v2).unwrap()).unwrap_err();
        assert!(
            v2_errors
                .iter()
                .any(|error| error.0.contains("mouse_direction")),
            "schema v2 must reject a v4-only direction trigger"
        );
    }

    #[test]
    fn official_v4_save_keeps_v1_backup_for_rollback() {
        let dir = std::env::temp_dir().join(format!(
            "inputflow-config-migration-{}-{}",
            std::process::id(),
            line!()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        fs::write(&path, V1_GOLDEN).unwrap();

        let loaded = load(&path);
        assert_eq!(loaded.source_schema_version, LEGACY_SCHEMA_VERSION);
        save(&path, &loaded.config).expect("migrated v4 save should succeed");

        let official = parse_and_migrate_json(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(official.source_schema_version, SCHEMA_VERSION);
        let backup = recovery_artifacts(&path)
            .into_iter()
            .find(|artifact| artifact.kind == RecoveryKind::Backup)
            .expect("replacement should retain the v1 primary");
        let backup_value: Value =
            serde_json::from_str(&fs::read_to_string(backup.path).unwrap()).unwrap();
        assert_eq!(backup_value["schema_version"], LEGACY_SCHEMA_VERSION);

        let _ = fs::remove_dir_all(&dir);
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
            key: logical("NotAKey"),
            timeout_ms: 250,
        };
        let problems = validate(&config).unwrap_err();
        assert!(
            problems
                .iter()
                .any(|e| e.0.contains("unknown logical key `NotAKey`"))
        );
    }

    #[test]
    fn timeout_out_of_range_is_rejected() {
        let mut config = sample_config();
        config.rules[0].trigger = TriggerConfig::Hold {
            key: logical("LeftCtrl"),
            timeout_ms: 0,
        };
        let problems = validate(&config).unwrap_err();
        assert!(problems.iter().any(|e| e.0.contains("out of range")));
    }

    #[test]
    fn every_trigger_kind_accepts_a_repeating_prefix() {
        let triggers = [
            TriggerConfig::KeyChord {
                first: logical("A"),
                second: logical("B"),
            },
            TriggerConfig::KeyMouseButton {
                key: logical("A"),
                button: "Right".to_string(),
            },
            TriggerConfig::Hold {
                key: logical("A"),
                timeout_ms: 250,
            },
            TriggerConfig::HoldMouseButton {
                key: logical("A"),
                timeout_ms: 250,
                button: "Right".to_string(),
            },
            TriggerConfig::MouseDirection {
                key: logical("A"),
                direction: DirectionConfig::Right,
                min_distance_px: DEFAULT_DIRECTION_MIN_DISTANCE_PX,
                max_duration_ms: DEFAULT_DIRECTION_MAX_DURATION_MS,
                off_axis_tolerance_px: DEFAULT_DIRECTION_OFF_AXIS_TOLERANCE_PX,
            },
        ];

        for trigger in triggers {
            let mut config = sample_config();
            config.rules[0].trigger = trigger;
            assert!(validate(&config).is_ok());
        }
    }

    #[test]
    fn direction_ranges_and_group_consistency_are_validated() {
        let base = RuleConfig {
            id: "right".to_string(),
            enabled: true,
            trigger: TriggerConfig::MouseDirection {
                key: logical("F8"),
                direction: DirectionConfig::Right,
                min_distance_px: 80,
                max_duration_ms: 1_000,
                off_axis_tolerance_px: 40,
            },
            action: ActionConfig::KeyChord {
                keys: vec![logical("C")],
            },
        };
        let mut config = Config {
            schema_version: SCHEMA_VERSION,
            emergency_bypass_key: logical("F12"),
            rules: vec![base.clone()],
        };
        assert!(validate_config(&config).is_ok());

        config.rules[0].trigger = TriggerConfig::MouseDirection {
            key: logical("F8"),
            direction: DirectionConfig::Right,
            min_distance_px: 9,
            max_duration_ms: 1_000,
            off_axis_tolerance_px: 40,
        };
        assert!(
            validate_config(&config)
                .unwrap_err()
                .iter()
                .any(|error| error.0.contains("min_distance_px"))
        );

        config.rules[0] = base.clone();
        let mut up = base;
        up.id = "up".to_string();
        up.trigger = TriggerConfig::MouseDirection {
            key: logical("F8"),
            direction: DirectionConfig::Up,
            min_distance_px: 81,
            max_duration_ms: 1_000,
            off_axis_tolerance_px: 40,
        };
        config.rules.push(up);
        assert!(
            validate_config(&config)
                .unwrap_err()
                .iter()
                .any(|error| error.0.contains("must use identical"))
        );
    }

    #[test]
    fn duplicate_ids_are_rejected() {
        let mut config = sample_config();
        config.rules.push(RuleConfig {
            id: "hold-ctrl-right-click-copy".to_string(),
            enabled: true,
            trigger: TriggerConfig::Hold {
                key: logical("LeftCtrl"),
                timeout_ms: 250,
            },
            action: ActionConfig::KeyChord {
                keys: vec![logical("LeftCtrl"), logical("C")],
            },
        });
        let problems = validate(&config).unwrap_err();
        assert!(problems.iter().any(|e| e.0.contains("duplicate rule id")));
    }

    #[test]
    fn disabled_rules_are_validated_but_excluded_from_conflicts_and_rule_count() {
        let mut config = sample_config();
        config.rules[0].enabled = false;
        config.rules.push(RuleConfig {
            id: "enabled-same-trigger".to_string(),
            enabled: true,
            trigger: config.rules[0].trigger.clone(),
            action: config.rules[0].action.clone(),
        });

        let validated = validate_config(&config).expect("disabled trigger must not conflict");
        assert_eq!(validated.rules.len(), 1);
        assert_eq!(validated.rules[0].id, "enabled-same-trigger");

        config.rules[0].id = "enabled-same-trigger".to_string();
        let errors = validate_config(&config).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|error| error.0.contains("duplicate rule id"))
        );

        config.rules[0].id = "disabled-invalid".to_string();
        config.rules[0].trigger = TriggerConfig::Hold {
            key: logical("A"),
            timeout_ms: 0,
        };
        let errors = validate_config(&config).unwrap_err();
        assert!(errors.iter().any(|error| error.0.contains("out of range")));
    }

    #[test]
    fn disabled_emergency_prefix_is_allowed_until_the_rule_is_enabled() {
        let mut config = sample_config();
        config.emergency_bypass_key = logical("LeftCtrl");
        config.rules[0].enabled = false;
        assert!(validate_config(&config).is_ok());

        config.rules[0].enabled = true;
        let errors = validate_config(&config).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|error| error.0.contains("collides with a rule prefix key"))
        );
    }

    #[test]
    fn emergency_key_collision_is_rejected() {
        let mut config = sample_config();
        config.emergency_bypass_key = logical("LeftCtrl");
        let problems = validate(&config).unwrap_err();
        assert!(
            problems
                .iter()
                .any(|e| e.0.contains("collides with a rule prefix key"))
        );
    }
    #[test]
    fn rule_conflict_is_rejected() {
        let mut config = sample_config();
        config.rules.push(RuleConfig {
            id: "hold-ctrl-copy".to_string(),
            enabled: true,
            trigger: TriggerConfig::Hold {
                key: logical("LeftCtrl"),
                timeout_ms: 250,
            },
            action: ActionConfig::KeyChord {
                keys: vec![logical("LeftCtrl"), logical("C")],
            },
        });
        let problems = validate(&config).unwrap_err();
        assert!(
            problems
                .iter()
                .any(|e| e.0.contains("different kinds share prefix key"))
        );
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir =
            std::env::temp_dir().join(format!("inputflow-config-test-{}", std::process::id()));
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
    fn structured_save_report_exposes_commit_backup_and_cleanup_warnings() {
        let dir = std::env::temp_dir().join(format!(
            "inputflow-config-save-report-{}-{}",
            std::process::id(),
            line!()
        ));
        let path = dir.join("config.json");
        let config = sample_config();

        let first = save_with_report(&path, &config).unwrap();
        assert_eq!(first.path, path);
        assert_eq!(first.backup_path, None);
        assert!(first.cleanup_warnings.is_empty());

        let second = save_with_report(&path, &config).unwrap();
        assert!(
            second
                .backup_path
                .as_ref()
                .is_some_and(|backup| backup.exists())
        );
        assert!(second.cleanup_warnings.is_empty());
        assert_eq!(validate_config(&config).unwrap().rules.len(), 1);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_overwrites_existing_config() {
        let dir = std::env::temp_dir().join(format!("inputflow-config-ow-{}", std::process::id()));
        let path = dir.join("config.json");

        let mut first = sample_config();
        first.rules.clear();
        save(&path, &first).expect("first save should succeed");

        let second = sample_config();
        save(&path, &second).expect("overwrite save should succeed");

        let loaded = load(&path);
        assert!(loaded.problems.is_empty());
        assert_eq!(loaded.rules, validate(&second).unwrap().rules);

        // ReplaceFile retains exactly one valid old config as a recovery copy;
        // the committed temp path itself has been consumed.
        let leftovers: Vec<PathBuf> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|path| {
                let name = path.file_name().unwrap().to_string_lossy();
                name.contains(".tmp.") || name.contains(".bak.")
            })
            .collect();
        assert_eq!(leftovers.len(), 1, "recovery files: {leftovers:?}");
        assert!(
            leftovers[0]
                .file_name()
                .unwrap()
                .to_string_lossy()
                .contains(".bak.")
        );
        let backup: Config =
            serde_json::from_str(&fs::read_to_string(&leftovers[0]).unwrap()).unwrap();
        assert!(backup.rules.is_empty());

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

    struct FailingCommitter {
        fail_replace: bool,
        fail_promote: bool,
    }

    impl FileCommitter for FailingCommitter {
        fn replace_existing(
            &self,
            _target: &Path,
            _replacement: &Path,
            _backup: &Path,
        ) -> io::Result<()> {
            if self.fail_replace {
                Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "injected replace failure",
                ))
            } else {
                unreachable!("test committer only exercises failure")
            }
        }

        fn promote_new(&self, _replacement: &Path, _target: &Path) -> io::Result<()> {
            if self.fail_promote {
                Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "injected promote failure",
                ))
            } else {
                unreachable!("test committer only exercises failure")
            }
        }
    }

    #[test]
    fn failed_replace_keeps_official_config_and_recovery_temp() {
        let dir = std::env::temp_dir().join(format!(
            "inputflow-config-replace-failure-{}-{}",
            std::process::id(),
            line!()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        let mut old = sample_config();
        old.rules.clear();
        fs::write(&path, serde_json::to_vec_pretty(&old).unwrap()).unwrap();

        let error = save_with_committer(
            &path,
            &sample_config(),
            &FailingCommitter {
                fail_replace: true,
                fail_promote: false,
            },
        )
        .unwrap_err();

        assert!(error.contains("injected replace failure"));
        let official: Config = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(official, old);
        let candidates = recovery_candidates(&path);
        assert_eq!(candidates.len(), 1);
        assert!(
            candidates[0]
                .file_name()
                .unwrap()
                .to_string_lossy()
                .contains(".tmp.")
        );
        assert_eq!(load(&path).rules.len(), 0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn failed_first_commit_is_recovered_from_valid_temp() {
        let dir = std::env::temp_dir().join(format!(
            "inputflow-config-promote-failure-{}-{}",
            std::process::id(),
            line!()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");

        save_with_committer(
            &path,
            &sample_config(),
            &FailingCommitter {
                fail_replace: false,
                fail_promote: true,
            },
        )
        .unwrap_err();
        assert!(!path.exists());

        let recovered = load(&path);
        assert_eq!(recovered.rules.len(), 1);
        assert!(
            recovered
                .problems
                .iter()
                .any(|problem| problem.contains("recovered"))
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_official_config_recovers_valid_backup() {
        let dir = std::env::temp_dir().join(format!(
            "inputflow-config-backup-recovery-{}-{}",
            std::process::id(),
            line!()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        fs::write(&path, "{ invalid").unwrap();
        let backup = path.with_file_name(format!("config.json.bak.{}.0", std::process::id()));
        fs::write(
            &backup,
            serde_json::to_vec_pretty(&sample_config()).unwrap(),
        )
        .unwrap();

        let recovered = load(&path);
        assert_eq!(recovered.rules.len(), 1);
        assert!(
            recovered
                .problems
                .iter()
                .any(|problem| problem.contains("recovered"))
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn committed_backup_is_preferred_over_newer_uncommitted_temp() {
        let dir = std::env::temp_dir().join(format!(
            "inputflow-config-recovery-priority-{}-{}",
            std::process::id(),
            line!()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        fs::write(&path, "{ invalid").unwrap();

        let mut committed = sample_config();
        committed.emergency_bypass_key = logical("F1");
        let backup = path.with_file_name("config.json.bak.test.0");
        fs::write(&backup, serde_json::to_vec_pretty(&committed).unwrap()).unwrap();

        let mut uncommitted = sample_config();
        uncommitted.emergency_bypass_key = logical("F2");
        let temp = path.with_file_name("config.json.tmp.test.1");
        fs::write(&temp, serde_json::to_vec_pretty(&uncommitted).unwrap()).unwrap();

        let loaded = load(&path);
        assert_eq!(loaded.emergency_key, Key::F1);
        assert!(loaded.problems[0].contains("committed backup"));
        assert_eq!(recovery_candidates(&path)[0], backup);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn successful_saves_bound_committed_backup_generations() {
        let dir = std::env::temp_dir().join(format!(
            "inputflow-config-retention-{}-{}",
            std::process::id(),
            line!()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        for key in ["F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10"] {
            let config = Config {
                schema_version: SCHEMA_VERSION,
                emergency_bypass_key: logical(key),
                rules: Vec::new(),
            };
            save(&path, &config).unwrap();
        }
        let backups = recovery_artifacts(&path)
            .into_iter()
            .filter(|artifact| artifact.kind == RecoveryKind::Backup)
            .count();
        assert_eq!(backups, RECOVERY_BACKUP_LIMIT);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn retention_never_deletes_the_only_valid_artifact() {
        let dir = std::env::temp_dir().join(format!(
            "inputflow-config-retention-valid-{}-{}",
            std::process::id(),
            line!()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        let valid = path.with_file_name("config.json.tmp.test.0");
        fs::write(&valid, serde_json::to_vec_pretty(&sample_config()).unwrap()).unwrap();
        for suffix in 1..=4 {
            fs::write(
                path.with_file_name(format!("config.json.tmp.test.{suffix}")),
                "{ corrupt",
            )
            .unwrap();
        }

        assert!(prune_recovery_artifacts(&path, RecoveryKind::Temp, 3).is_empty());
        assert!(valid.exists());
        assert_eq!(
            recovery_artifacts(&path)
                .into_iter()
                .filter(|artifact| artifact.kind == RecoveryKind::Temp)
                .count(),
            4
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn concurrent_saves_do_not_share_artifact_names() {
        let dir = std::env::temp_dir().join(format!(
            "inputflow-config-concurrent-{}-{}",
            std::process::id(),
            line!()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = std::sync::Arc::new(dir.join("config.json"));
        let mut workers = Vec::new();

        for key in ["F1", "F2", "F3", "F4"] {
            let path = std::sync::Arc::clone(&path);
            workers.push(std::thread::spawn(move || {
                let config = Config {
                    schema_version: SCHEMA_VERSION,
                    emergency_bypass_key: logical(key),
                    rules: Vec::new(),
                };
                save(&path, &config)
            }));
        }
        for worker in workers {
            worker.join().unwrap().unwrap();
        }

        let loaded = load(&path);
        assert!(loaded.problems.is_empty());
        assert!([Key::F1, Key::F2, Key::F3, Key::F4].contains(&loaded.emergency_key));
        let artifact_names: Vec<String> = recovery_candidates(&path)
            .into_iter()
            .map(|candidate| {
                candidate
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        let unique: std::collections::BTreeSet<_> = artifact_names.iter().collect();
        assert_eq!(unique.len(), artifact_names.len());
        assert!(!artifact_names.iter().any(|name| name.contains(".tmp.")));
        let _ = fs::remove_dir_all(&dir);
    }
}

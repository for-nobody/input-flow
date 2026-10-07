//! InputFlow configuration: a versioned, human-readable JSON schema with
//! validation, crash-recoverable replacement, and bad-file fallback.
//!
//! Loading never fails the program: a missing, unreadable, or invalid config
//! produces a default (empty rules, default emergency key) result plus a list of
//! human-readable problems, so the caller can start in bypass (NFR-03).

mod config;

pub use config::{
    ActionConfig, Config, ConfigError, DEFAULT_DIRECTION_MAX_DURATION_MS,
    DEFAULT_DIRECTION_MIN_DISTANCE_PX, DEFAULT_DIRECTION_OFF_AXIS_TOLERANCE_PX,
    DEFAULT_EMERGENCY_KEY, DirectionConfig, KEY_IDENTITY_SCHEMA_VERSION, KeyConfig,
    LEGACY_SCHEMA_VERSION, LoadedConfig, MAX_DIRECTION_DISTANCE_PX, MAX_DIRECTION_DURATION_MS,
    MAX_DIRECTION_OFF_AXIS_TOLERANCE_PX, MAX_TIMEOUT_MS, MIN_DIRECTION_DISTANCE_PX,
    MIN_DIRECTION_DURATION_MS, MIN_TIMEOUT_MS, MigrationReport, PREVIOUS_SCHEMA_VERSION,
    RuleConfig, SCHEMA_VERSION, SaveReport, TriggerConfig, ValidatedConfig, default_config, load,
    parse_and_migrate_json, save, save_with_report, to_json_pretty, validate_config,
};

//! InputFlow configuration: a versioned, human-readable JSON schema with
//! validation, crash-recoverable replacement, and bad-file fallback.
//!
//! Loading never fails the program: a missing, unreadable, or invalid config
//! produces a default (empty rules, default emergency key) result plus a list of
//! human-readable problems, so the caller can start in bypass (NFR-03).

mod config;

pub use config::{
    ActionConfig, Config, ConfigError, DEFAULT_EMERGENCY_KEY, KeyConfig, LEGACY_SCHEMA_VERSION,
    LoadedConfig, MAX_TIMEOUT_MS, MIN_TIMEOUT_MS, MigrationReport, RuleConfig, SCHEMA_VERSION,
    TriggerConfig, default_config, load, parse_and_migrate_json, save, to_json_pretty,
};

//! InputFlow configuration: a versioned, human-readable JSON schema with
//! validation, atomic save, and bad-file fallback.
//!
//! Loading never fails the program: a missing, unreadable, or invalid config
//! produces a default (empty rules, default emergency key) result plus a list of
//! human-readable problems, so the caller can start in bypass (NFR-03).

mod config;

pub use config::{
    default_config, load, save, to_json_pretty, ActionConfig, Config, ConfigError, LoadedConfig,
    RuleConfig, TriggerConfig, DEFAULT_EMERGENCY_KEY, MAX_TIMEOUT_MS, MIN_TIMEOUT_MS,
    SCHEMA_VERSION,
};

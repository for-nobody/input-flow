# Configuration golden fixtures

These UTF-8 JSON files are the cross-language contract for Rust and the future
C# settings DTOs.

- `v1-valid.json` freezes the legacy string-key representation. Rust must keep
  loading it and migrate it in memory without changing the rule meaning.
- `v2-valid.json` freezes the explicit logical/physical key representation.
  Rust and C# migrate it to v3 without losing the match mode, scan code, or
  extended bit; existing rules become enabled.
- `v3-valid.json` is the current write contract. It retains explicit key
  identities and requires a persistent boolean `enabled` on every rule. Rust
  and the C# settings core must round-trip this shape.
- `v2-phase-b-manual-acceptance.json` is a deliberately narrow live-input
  acceptance configuration. A physical `Oem1` or logical `CapsLock` starts a
  candidate, `F9` completes it, and a successful match emits `C`. Releasing the
  first key without `F9` exercises lossless scan-code replay; `F12` remains the
  emergency bypass key.

Unknown fields, unknown logical names, physical scan code zero, a physical
emergency key, mixed key shapes, missing v3 enablement, and unsupported schema versions are
invalid rather than silently coerced.

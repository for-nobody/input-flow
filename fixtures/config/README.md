# Configuration golden fixtures

These UTF-8 JSON files are the cross-language contract for Rust and the future
C# settings DTOs.

- `v1-valid.json` freezes the legacy string-key representation. Rust must keep
  loading it and migrate it in memory without changing the rule meaning.
- `v2-valid.json` freezes the explicit logical/physical key representation.
  Rust must round-trip it without losing the match mode, scan code, or extended
  bit. C# must deserialize and serialize the same shape before it may save a
  configuration through the agent protocol.
- `v2-phase-b-manual-acceptance.json` is a deliberately narrow live-input
  acceptance configuration. A physical `Oem1` or logical `CapsLock` starts a
  candidate, `F9` completes it, and a successful match emits `C`. Releasing the
  first key without `F9` exercises lossless scan-code replay; `F12` remains the
  emergency bypass key.

Unknown fields, unknown logical names, physical scan code zero, a physical
emergency key, mixed v1/v2 key shapes, and unsupported schema versions are
invalid rather than silently coerced.

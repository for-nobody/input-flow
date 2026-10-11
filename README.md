# InputFlow

[English](README.md) | [简体中文](README.zh-CN.md)

InputFlow is a Windows-wide keyboard and mouse combination engine. It withholds only input that may form an
enabled rule. A matching rule consumes that input and sends its action; a failed or timed-out candidate replays
the withheld input in order on a best-effort basis.

## Current status

InputFlow v0.9.0 is available as a public GitHub Pre-release. The final RC-01 build adds complete English and
Simplified Chinese resources to WinUI Settings and passed the affected automated, UI Automation, Narrator,
125%/150% layout, configuration, and physical-input smoke tests. The published ZIP was downloaded again and its
SHA-256 verified. See the [v0.9.0 release](https://github.com/for-nobody/input-flow/releases/tag/v0.9.0) and
[the authoritative current status](docs/status/CURRENT_STATUS.md).

## Architecture and safety boundaries

- `inputflow-agent.exe` is the only owner of hooks, tray state, the rule runtime, formal configuration, and IPC.
- `InputFlow.Settings.exe` is an on-demand C# + WinUI 3 application. Closing its last window exits Settings only.
- Settings never installs hooks or directly writes formal configuration. The Agent validates, atomically saves,
  and hot-applies changes.
- The Agent does not load .NET, WinUI, WebView, or JavaScript runtimes.
- The hook hot path performs no UI, disk or network I/O, unbounded allocation, unbounded queueing, or UI waits.
- `F12` is reserved as the emergency bypass and cannot be swallowed by input capture.
- The project does not use Tauri, React, Node.js, npm, WebView2, or Electron.

## Implemented features

- KeyChord, Key+MouseButton, Hold, Hold+MouseButton, and four-direction mouse-movement triggers.
- Logical and physical key identities, named keys, scan-code replay, and layout-aware display names.
- Key-chord actions, persistent rule enablement, validation, atomic save, recovery, and runtime replacement.
- Rust/Win32 resident Agent with tray controls, single instance handling, and Explorer tray recovery.
- Versioned, bounded Windows Named Pipe IPC protected for the current user.
- WinUI rule editing, input capture, connection coordination, diagnostics, and accessible labels/status regions.
- Schema v4 mouse direction parameters: activation key, net displacement, off-axis tolerance, timeout, and one
  match per hold. Normal pointer movement always passes through. Schemas v1–v3 migrate deterministically.

## Install and run the portable preview

The release package is `InputFlow-0.9.0-win-x64.zip`. It targets Windows x64 and declares Windows 10 build 17763
as its minimum. The unpackaged directory carries self-contained .NET and Windows App SDK components. Microsoft
Visual C++ Redistributable 2015–2022 x64 is still required. The first release is unsigned.

1. Verify the downloaded zip against the accompanying `SHA256SUMS.txt`.
2. Extract the complete archive into a user-writable directory; keep all DLL, PRI, documentation, and license files.
3. Run `inputflow-agent.exe`, then open Settings from the tray icon.
4. Use `F12` to pause or resume immediately. Exit the Agent normally from its tray menu.

The exact RC artifact is fixed locally; a public download URL will only be added after the GitHub pre-release is
published. Detailed operation, upgrade, autostart, and removal instructions are in the
[English user guide](docs/guides/USER_GUIDE.en-US.md).

The repository also contains a separate Windows x64 setup path for 0.9.1. It is not the published v0.9.0
download. The setup installs into `C:\Program Files\InputFlow` and uses the system .NET 10 runtime, Windows App
Runtime 2.5.1, and the Visual C++ 2015–2022 x64 redistributable. Build it from
[BUILD_WINDOWS.md section 13](docs/guides/BUILD_WINDOWS.md). The setup executable is not committed.

## Display language and accessibility

Settings follows the Windows display language by default and falls back to English when no supported resource
matches. Under **Settings and diagnostics > Display language**, select **System default / 跟随系统**,
**English**, or **简体中文**, then close and reopen Settings. The Agent and active rules keep running.

Visible text, dynamic notifications, and UI Automation names are localized in `en-US` and `zh-CN`. Narrator
reads the selected language's labels; voice selection depends on language voices installed in Windows. InputFlow
does not install voices or modify system Narrator settings.

## Build and test

Prerequisites and exact toolchain details are in the [Windows build guide](docs/guides/BUILD_WINDOWS.md).

```powershell
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build -p inputflow-agent --release
./scripts/build-windows.ps1
```

The complete script validates Rust, the protocol and Settings Core runners, WinUI Debug/Release builds, resource
key parity, and English/Chinese runtime resource loading. Release packaging is performed separately by
`scripts/package-release.ps1` and requires a clean fixed commit for an RC artifact.

## Known limitations

- Real cross-monitor, cross-DPI, and hot-plug behavior has not been verified on representative hardware.
- `SendInput` is affected by UIPI, focus, and modifier state; perfect replay into every target is not guaranteed.
- Force-ending the Agent cannot guarantee recovery of historical input already withheld.
- Some special-key hardware, partial `SendInput`, sleep/resume, and Narrator voice availability on other systems
  remain environment-dependent or not fully tested.
- The public preview does not claim 24-hour or 72-hour endurance evidence.
- Fn, arbitrary three-key chords, per-application rules, and URL/program/folder actions are not implemented.

## Documentation and contribution

- [Documentation index](docs/README.md)
- [Current status](docs/status/CURRENT_STATUS.md)
- [Post-release soak task](docs/tasks/POST_RELEASE_SOAK.md)
- [v0.9.0 release notes](docs/releases/V0.9.0.md)
- [Archived first-release record](docs/archive/first-release/FIRST_RELEASE_RC_EXECUTION.md)
- [Windows build guide](docs/guides/BUILD_WINDOWS.md)
- [Windows setup notes](docs/guides/BUILD_WINDOWS.md) (section 13)

When filing an issue, include the InputFlow version, Windows build, rule type and parameters, reproduction steps,
target elevation state, and relevant logs with sensitive data removed. Do not upload private configurations or
complete input traces.

## License

InputFlow is licensed under the [MIT License](LICENSE). Third-party notices are in
[THIRD-PARTY-NOTICES.txt](THIRD-PARTY-NOTICES.txt); distribution packages also include the locked dependencies'
license files.

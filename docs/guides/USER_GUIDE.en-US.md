# InputFlow v0.9.0 User Guide (Windows x64 Public Preview)

[English](USER_GUIDE.en-US.md) | [简体中文](USER_GUIDE.md)

InputFlow is a local keyboard and mouse combination tool. `inputflow-agent.exe` owns the tray icon,
input hooks, formal configuration, and IPC. `InputFlow.Settings.exe` runs only while you configure the
application. This first version is a public preview and has not completed 24-hour or 72-hour endurance tests.

## First run

1. Download `InputFlow-0.9.0-win-x64.zip` and `SHA256SUMS.txt`. Verify the published checksum with
   `Get-FileHash .\InputFlow-0.9.0-win-x64.zip -Algorithm SHA256`.
2. Extract the complete zip into a normal user-writable directory. Do not copy only the two executable files;
   the DLLs, resource PRI, documentation, and license files must remain together.
3. Run `inputflow-agent.exe`. The InputFlow icon appears in the notification area.
4. Double-click the tray icon, or select **Open Settings** from its menu.
5. A first run has no enabled sample rules, so it does not intercept ordinary input.

The package targets Windows x64 and declares Windows 10 build 17763 as its minimum. The release notes define
the Windows builds actually covered by distribution testing. The directory carries self-contained .NET and
Windows App SDK components, but Microsoft Visual C++ Redistributable 2015–2022 x64 is still required. This
version is unsigned; if Windows shows a source or reputation warning, verify the published SHA-256 rather than
disabling system protection.

## Display language and Narrator

Settings follows the Windows display language by default, with English as the fallback. Under **Settings and
diagnostics > Display language**, you can choose **System default / 跟随系统**, **English**, or **简体中文**.
Close and reopen Settings after changing the selection. The Agent, hooks, and active rules keep running.

Visible labels, status notifications, and UI Automation names are localized. Windows Narrator reads those
labels in the selected UI language; the actual voice depends on language voices installed in Windows. InputFlow
does not install a voice or change the user's Narrator configuration.

## Create a mouse-direction rule

In Settings, create a Mouse Direction rule, then select an activation key, direction, action, and these values:

- Minimum distance in screen pixels, for example 80 px.
- Maximum duration in milliseconds, for example 500 ms.
- Off-axis tolerance in screen pixels, for example 40 px.

After saving, hold the activation key and move the mouse. Pointer movement always passes through. Each hold can
trigger at most once and rearms after release. A failed, released, or timed-out candidate replays the withheld
activation key on a best-effort basis.

## Pause, close, and exit

- `F12`: emergency pause or resume.
- Tray menu: shows authoritative state and can pause, resume, open Settings, or exit the Agent.
- Closing Settings: closes only the Settings process; the Agent and rules continue running.
- **Exit InputFlow**: performs a normal Agent shutdown and allows up to two seconds for releases of consumed keys.
- Force-ending the process cannot guarantee recovery of historical input that was already withheld.

Output is affected by target integrity (UIPI), focus, and current modifier state. A non-elevated Agent cannot
guarantee injection into an elevated window.

## Optional sign-in autostart

Autostart is disabled by default. It starts only the current user's Agent, not Settings, and creates neither a
service nor a scheduled task.

- Enable: run `Enable-InputFlow-Autostart.cmd`.
- Inspect: run `Get-InputFlow-Autostart-Status.cmd`.
- Disable: run `Disable-InputFlow-Autostart.cmd`.

The scripts manage one `InputFlow Agent.lnk` in the current user's Startup folder. If you move or upgrade the
program directory, run the enable script from the new directory to update the shortcut target.

## Upgrade and remove

To upgrade, close Settings, exit the old Agent from the tray, extract the new zip to a new directory, and start
the new Agent. Keep `%LOCALAPPDATA%\InputFlow` to retain rules, UI language preference, limited backups, and local
logs. Re-enable autostart from the new directory if you use it. Schema v4 contains mouse-direction rules; do not
open and save a v4 configuration with an older Agent or Settings build.

To remove InputFlow, close Settings, exit the Agent normally, run `Disable-InputFlow-Autostart.cmd`, and delete
the extracted program directory after confirming that no InputFlow process remains. The per-user data directory
is retained by default. Delete it manually only if you no longer need its configuration or logs.

## Diagnostics and feedback

- Configuration, UI preferences, and logs: `%LOCALAPPDATA%\InputFlow`.
- Default logs do not record each key identity; only explicit `--debug-input` diagnostics record detailed input.
- When reporting a problem, include the InputFlow version, Windows build, rule type and parameters, reproduction
  steps, whether the target process is elevated, and relevant logs with sensitive data removed.

Known gaps include real multi-monitor/cross-DPI hot-plug testing, some special-key hardware, Narrator voice
availability, partial `SendInput`, sleep/resume evidence, and 24-hour/72-hour endurance tests.

## License

InputFlow uses the MIT License. See `LICENSE.txt` in the package. Third-party notices and full license materials
are in `THIRD-PARTY-NOTICES.txt` and `Licenses/`.

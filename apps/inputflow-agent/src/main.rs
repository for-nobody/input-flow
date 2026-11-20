#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Thin product host for the shared InputFlow runtime.

mod ipc;

use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use inputflow_runtime::{ApplyOutcome, CaptureOutcome, Runtime, RuntimeOptions, default_data_dir};
use inputflow_windows::platform::shell::{
    SingleInstance, SingleInstanceGuard, TrayEvent, TrayHandle, allow_settings_foreground,
    notify_settings_error, show_settings_error,
};

use crate::ipc::AgentIpc;

const INSTANCE_NAME: &str = "Local\\InputFlow.Agent.v1";
const MAX_LOG_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Args {
    config_path: PathBuf,
    settings_path: Option<PathBuf>,
    debug_input: bool,
    observe_touchpad: bool,
    observe_numpad: bool,
    no_tray: bool,
    run_for: Option<Duration>,
    smoke_iterations: Option<usize>,
}

enum ParseResult {
    Run(Args),
    Help,
}

struct AgentLog {
    file: Mutex<File>,
}

impl AgentLog {
    fn open(allow_truncate: bool) -> Result<Self, String> {
        let directory = default_data_dir();
        fs::create_dir_all(&directory).map_err(|error| {
            format!(
                "failed to create the InputFlow data directory `{}`: {error}",
                directory.display()
            )
        })?;
        let path = directory.join("agent.log");
        let truncate = allow_truncate
            && path
                .metadata()
                .map(|metadata| metadata.len() >= MAX_LOG_BYTES)
                .unwrap_or(false);
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .append(!truncate)
            .truncate(truncate)
            .open(&path)
            .map_err(|error| format!("failed to open `{}`: {error}", path.display()))?;
        Ok(Self {
            file: Mutex::new(file),
        })
    }

    fn write(&self, level: &str, message: &str) {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|value| value.as_secs())
            .unwrap_or(0);
        if let Ok(mut file) = self.file.lock() {
            let _ = writeln!(file, "{timestamp} {level} {message}");
            let _ = file.flush();
        }
    }
}

fn main() {
    let args = match parse_args(env::args().skip(1)) {
        Ok(ParseResult::Run(args)) => args,
        Ok(ParseResult::Help) => {
            print_usage();
            return;
        }
        Err(error) => {
            eprintln!("inputflow-agent: {error}");
            print_usage();
            std::process::exit(2);
        }
    };

    let (instance, instance_guard) = match SingleInstanceGuard::acquire(INSTANCE_NAME) {
        Ok(result) => result,
        Err(error) => {
            eprintln!("inputflow-agent: {error}");
            std::process::exit(1);
        }
    };
    let log = match AgentLog::open(instance == SingleInstance::Acquired) {
        Ok(log) => Arc::new(log),
        Err(error) => {
            eprintln!("inputflow-agent: {error}");
            std::process::exit(1);
        }
    };

    if instance == SingleInstance::AlreadyRunning {
        let settings = resolve_settings_path(args.settings_path.as_deref());
        if let Err(error) = launch_settings(&settings, &log) {
            log.write("ERROR", &format!("second_instance: {error}"));
            show_settings_error(&error);
            std::process::exit(3);
        }
        log.write("INFO", "second_instance: settings launch requested");
        return;
    }
    let _instance_guard = instance_guard.expect("acquired instances own a mutex guard");

    if let Err(error) = run_agent(args, &log) {
        log.write("ERROR", &error);
        std::process::exit(1);
    }
}

fn run_agent(args: Args, log: &Arc<AgentLog>) -> Result<(), String> {
    if args.no_tray && args.run_for.is_none() && args.smoke_iterations.is_none() {
        return Err("--no-tray requires --run-for-ms or --smoke[-iterations]".to_string());
    }

    let runtime_log = Arc::clone(log);
    let sink = Arc::new(move |line: String| runtime_log.write("RUNTIME", &line));
    let mut options = RuntimeOptions::new(sink);
    options.config_path = args.config_path;
    options.debug_input = args.debug_input;
    options.observe_touchpad = args.observe_touchpad;
    options.observe_numpad = args.observe_numpad;

    let (runtime, start) = Runtime::start(options)?;
    let runtime = Arc::new(runtime);
    log.write(
        "INFO",
        &format!(
            "ready: rules={} source_schema={} config_write_blocked={} previous_abnormal={} config_warnings={}",
            start.rule_count,
            start.source_schema_version,
            start.config_write_blocked,
            start.previous_abnormal_termination,
            start.config_problems.len()
        ),
    );
    if args.debug_input {
        for problem in &start.config_problems {
            log.write("DEBUG", &format!("config_warning: {problem}"));
        }
    }

    let mut ipc = AgentIpc::start(Arc::clone(&runtime), Arc::clone(log))?;
    log.write("INFO", &format!("ipc_ready: pipe={}", ipc.pipe_name()));

    let mut tray = if args.no_tray {
        None
    } else {
        Some(TrayHandle::start()?)
    };

    if let Some(iterations) = args.smoke_iterations {
        run_smoke(&runtime, iterations, log)?;
    }

    let settings_path = resolve_settings_path(args.settings_path.as_deref());
    if let Some(tray) = tray.as_ref()
        && (args.smoke_iterations.is_none() || args.run_for.is_some())
    {
        run_tray_loop(&runtime, tray, &settings_path, args.run_for, log)?;
    } else if let Some(duration) = args.run_for {
        std::thread::sleep(duration);
    }

    if args.observe_numpad {
        let status = runtime.status().numpad;
        log.write("INFO", &format!("numpad_observe_summary reports={} confirmed={} cancelled={} requested={} inserted={}",
            status.reports, status.confirmed, status.cancelled, status.requested_inputs, status.inserted_inputs));
    }
    ipc.shutdown()?;
    let shutdown = runtime.shutdown();
    log.write(
        "INFO",
        &format!(
            "stopped: observed={} output_sent={} output_failed={} output_dropped={} hook_panicked={} logger_panicked={}",
            shutdown.observed_events,
            shutdown.output_batches_sent,
            shutdown.output_batches_failed,
            shutdown.output_batches_dropped,
            shutdown.hook_thread_panicked,
            shutdown.logger_thread_panicked
        ),
    );
    if let Some(tray) = tray.as_mut() {
        tray.shutdown()?;
    }
    Ok(())
}

fn run_smoke(runtime: &Runtime, iterations: usize, log: &AgentLog) -> Result<(), String> {
    for _ in 0..iterations {
        runtime.pause()?;
        runtime.resume()?;
    }

    let apply = runtime.apply_config(runtime.current_config());
    if apply.outcome != ApplyOutcome::Applied {
        return Err(format!(
            "smoke apply failed: outcome={:?} error={:?} recovery_required={}",
            apply.outcome, apply.error, apply.recovery_required
        ));
    }
    let cleanup_warnings = apply
        .save
        .as_ref()
        .map(|save| save.cleanup_warnings.len())
        .unwrap_or(0);

    let capture = runtime.begin_capture(Duration::from_secs(1))?;
    runtime.cancel_capture(capture.id())?;
    let outcome = capture
        .recv_timeout(Duration::from_secs(1))
        .map_err(|error| format!("smoke capture result was not delivered: {error}"))?;
    if outcome != CaptureOutcome::Cancelled {
        return Err(format!("smoke capture ended unexpectedly: {outcome:?}"));
    }
    log.write(
        "INFO",
        &format!("smoke_complete: iterations={iterations} cleanup_warnings={cleanup_warnings}"),
    );
    Ok(())
}

fn run_tray_loop(
    runtime: &Runtime,
    tray: &TrayHandle,
    settings_path: &Path,
    run_for: Option<Duration>,
    log: &AgentLog,
) -> Result<(), String> {
    let deadline = run_for.map(|duration| Instant::now() + duration);
    let mut settings_start: Vec<SettingsStart> = Vec::new();
    loop {
        settings_start.retain_mut(|start| !start.finished(log));
        let now = Instant::now();
        if deadline.is_some_and(|deadline| now >= deadline) {
            break;
        }
        let wait = deadline.map(|deadline| deadline - now);
        let wait = if !settings_start.is_empty() {
            let poll = Duration::from_millis(100);
            Some(wait.map_or(poll, |remaining| remaining.min(poll)))
        } else {
            wait
        };
        let event = if let Some(wait) = wait {
            match tray.recv_timeout(wait) {
                Ok(event) => event,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    return Err("tray event channel disconnected".to_string());
                }
            }
        } else {
            tray.recv()
                .map_err(|_| "tray event channel disconnected".to_string())?
        };

        match event {
            TrayEvent::OpenSettings => {
                if settings_start.len() >= 4 {
                    log.write(
                        "INFO",
                        "open_settings: four recent Settings launches are still being checked",
                    );
                    continue;
                }
                match launch_settings(settings_path, log) {
                    Ok(child) => settings_start.push(SettingsStart {
                        child,
                        at: Instant::now(),
                    }),
                    Err(error) => report_settings_error(error, log),
                }
            }
            TrayEvent::TogglePause => {
                if runtime.status().suspended {
                    runtime.resume()?;
                    log.write("INFO", "tray: resumed");
                } else {
                    let report = runtime.pause()?;
                    log.write(
                        "INFO",
                        &format!(
                            "tray: paused held={} output_complete={}",
                            report.held_events, report.output_complete
                        ),
                    );
                }
            }
            TrayEvent::Exit => break,
        }
    }
    Ok(())
}

fn resolve_settings_path(explicit: Option<&Path>) -> PathBuf {
    explicit.map(Path::to_path_buf).unwrap_or_else(|| {
        env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| PathBuf::from("."))
            .join("InputFlow.Settings.exe")
    })
}

fn launch_settings(path: &Path, log: &AgentLog) -> Result<Child, String> {
    let path = std::path::absolute(path).map_err(|error| {
        format!(
            "failed to resolve Settings path `{}`: {error}",
            path.display()
        )
    })?;
    if !path.is_file() {
        return Err(format!(
            "settings executable is unavailable at `{}`; pass --settings PATH",
            path.display()
        ));
    }
    let directory = path
        .parent()
        .ok_or_else(|| format!("Settings path has no parent: `{}`", path.display()))?;
    let child = Command::new(&path)
        .current_dir(directory)
        .spawn()
        .map_err(|error| format!("failed to launch settings at `{}`: {error}", path.display()))?;
    allow_settings_foreground(child.id());
    log.write(
        "INFO",
        &format!(
            "settings launch requested: pid={} path=`{}` working_directory=`{}`",
            child.id(),
            path.display(),
            directory.display()
        ),
    );
    Ok(child)
}

fn report_settings_error(error: String, log: &AgentLog) {
    log.write("ERROR", &format!("open_settings: {error}"));
    if let Err(error) = notify_settings_error(error) {
        log.write(
            "ERROR",
            &format!("open_settings: failed to display error: {error}"),
        );
    }
}

struct SettingsStart {
    child: Child,
    at: Instant,
}

impl SettingsStart {
    fn finished(&mut self, log: &AgentLog) -> bool {
        let pid = self.child.id();
        match self.child.try_wait() {
            Ok(Some(exit)) => {
                match exit.code() {
                    Some(0) => log.write("INFO", &format!("open_settings: Settings pid={pid} exited during launch check with code 0")),
                    Some(10) => report_settings_error(format!("Settings pid={pid} could not find the existing Settings window (exit 10)."), log),
                    Some(11) => log.write("WARN", &format!("open_settings: Windows did not foreground the existing Settings window (pid={pid}, exit 11). The window requested a taskbar flash.")),
                    _ => report_settings_error(format!("Settings pid={pid} exited during startup ({exit}). See the Windows Application event log for crash details."), log),
                }
                true
            }
            Ok(None) => self.at.elapsed() >= Duration::from_secs(3),
            Err(error) => {
                log.write(
                    "WARN",
                    &format!("open_settings: could not check Settings pid={pid}: {error}"),
                );
                true
            }
        }
    }
}

fn parse_args(arguments: impl IntoIterator<Item = String>) -> Result<ParseResult, String> {
    let mut config_path = default_data_dir().join("config.json");
    let mut settings_path = None;
    let mut debug_input = false;
    let mut observe_touchpad = false;
    let mut observe_numpad = false;
    let mut no_tray = false;
    let mut run_for = None;
    let mut smoke_iterations = None;
    let arguments = arguments.into_iter().collect::<Vec<_>>();
    let mut index = 0;
    while index < arguments.len() {
        match arguments[index].as_str() {
            "--config" => {
                index += 1;
                config_path = PathBuf::from(
                    arguments
                        .get(index)
                        .ok_or_else(|| "--config requires a path".to_string())?,
                );
            }
            "--settings" => {
                index += 1;
                settings_path = Some(PathBuf::from(
                    arguments
                        .get(index)
                        .ok_or_else(|| "--settings requires a path".to_string())?,
                ));
            }
            "--debug-input" => debug_input = true,
            "--observe-touchpad" => observe_touchpad = true,
            "--observe-numpad" => observe_numpad = true,
            "--no-tray" => no_tray = true,
            "--run-for-ms" => {
                index += 1;
                let milliseconds = parse_positive::<u64>(
                    arguments
                        .get(index)
                        .ok_or_else(|| "--run-for-ms requires a value".to_string())?,
                    "--run-for-ms",
                )?;
                run_for = Some(Duration::from_millis(milliseconds));
            }
            "--smoke" => smoke_iterations = Some(10),
            "--smoke-iterations" => {
                index += 1;
                smoke_iterations = Some(parse_positive::<usize>(
                    arguments
                        .get(index)
                        .ok_or_else(|| "--smoke-iterations requires a value".to_string())?,
                    "--smoke-iterations",
                )?);
            }
            "--help" | "-h" => return Ok(ParseResult::Help),
            other => return Err(format!("unknown argument `{other}`")),
        }
        index += 1;
    }
    Ok(ParseResult::Run(Args {
        config_path,
        settings_path,
        debug_input,
        observe_touchpad,
        observe_numpad,
        no_tray,
        run_for,
        smoke_iterations,
    }))
}

fn parse_positive<T>(value: &str, option: &str) -> Result<T, String>
where
    T: std::str::FromStr + PartialEq + Default,
{
    let parsed = value
        .parse::<T>()
        .map_err(|_| format!("{option} requires a positive integer"))?;
    if parsed == T::default() {
        Err(format!("{option} requires a positive integer"))
    } else {
        Ok(parsed)
    }
}

fn print_usage() {
    println!("InputFlow background agent");
    println!("usage: inputflow-agent [--config PATH] [--settings PATH] [--debug-input] [--help]");
    println!("  --debug-input             opt in to per-input local diagnostics");
    println!("  --observe-touchpad        read-only touchpad recognition diagnostics");
    println!("  --observe-numpad          read-only digit recognition diagnostics");
    println!("  --run-for-ms N            exit after N milliseconds (verification)");
    println!("  --smoke[-iterations N]    exercise lifecycle operations (verification)");
    println!("  --no-tray                 verification only; requires a bounded run mode");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_keeps_input_diagnostics_opt_in() {
        let ParseResult::Run(args) = parse_args(Vec::<String>::new()).unwrap() else {
            panic!("expected runnable arguments");
        };
        assert!(!args.debug_input);
        assert!(args.run_for.is_none());
        assert!(args.smoke_iterations.is_none());
    }

    #[test]
    fn parser_accepts_bounded_smoke_options() {
        let ParseResult::Run(args) = parse_args(
            [
                "--config",
                "config.json",
                "--settings",
                "settings.exe",
                "--debug-input",
                "--run-for-ms",
                "250",
                "--smoke-iterations",
                "12",
            ]
            .into_iter()
            .map(str::to_string),
        )
        .unwrap() else {
            panic!("expected runnable arguments");
        };
        assert_eq!(args.config_path, PathBuf::from("config.json"));
        assert_eq!(args.settings_path, Some(PathBuf::from("settings.exe")));
        assert!(args.debug_input);
        assert_eq!(args.run_for, Some(Duration::from_millis(250)));
        assert_eq!(args.smoke_iterations, Some(12));
    }

    #[test]
    fn parser_rejects_zero_values() {
        assert!(parse_args(["--run-for-ms".to_string(), "0".to_string()]).is_err());
        assert!(parse_args(["--smoke-iterations".to_string(), "0".to_string()]).is_err());
    }
}

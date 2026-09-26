//! probe-cli: a Windows keyboard/mouse low-level hook probe driven by the
//! InputFlow engine (M6).
//!
//! It loads a versioned JSON config (invalid config falls back to empty rules /
//! bypass), installs `WH_KEYBOARD_LL` and `WH_MOUSE_LL` on a dedicated
//! message-loop thread, and runs a console loop for `pause`/`resume`/`stats`/
//! `quit`. `--debug` enables per-event logging (off by default, NFR-05).

use std::env;
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

use inputflow_config::{default_config, load as load_config, to_json_pretty};
use inputflow_engine::{Command, Matcher, RuleIndex, SystemClock};
use inputflow_windows::platform::windows;

/// Bounded capacity for the diagnostic-log channel.
const LOG_QUEUE_CAPACITY: usize = 1024;
/// Bounded capacity for the output-command channel (replay / action).
const OUTPUT_QUEUE_CAPACITY: usize = 8;

fn main() {
    let (config_path, debug) = parse_args();
    windows::set_debug_log(debug);

    // Detect a previous abnormal termination before overwriting the marker.
    let marker = CrashMarker::new();
    if let Some(warning) = marker.warning_if_previous_abnormal() {
        eprintln!("probe-cli: warning: {warning}");
    }
    marker.mark_running();

    let loaded = load_config(&config_path);
    for problem in &loaded.problems {
        eprintln!("probe-cli: warning: {problem}");
    }
    let emergency_key = loaded.emergency_key;
    let rules = loaded.rules;
    let rule_count = rules.len();

    // load() already validated the rules; this is a defensive fallback only.
    let index = RuleIndex::compile(rules).unwrap_or_else(|errors| {
        for error in errors {
            eprintln!("probe-cli: invalid rule: {error}");
        }
        RuleIndex::compile(Vec::new()).expect("empty rule index always compiles")
    });
    let matcher = Matcher::new(Box::new(SystemClock::new()), index, 16);

    let (log_tx, log_rx) = mpsc::sync_channel::<String>(LOG_QUEUE_CAPACITY);
    let (output_tx, output_rx) = mpsc::sync_channel::<Command>(OUTPUT_QUEUE_CAPACITY);
    // Unbounded acknowledgement channel: the output worker sends one `()` after
    // each executed command, and the hook thread waits on it as an ordering
    // barrier so a pass-through event cannot overtake a just-dispatched replay.
    let (ack_tx, ack_rx) = mpsc::channel::<()>();

    // Make the matcher and senders visible to the hook callbacks before any
    // hook is installed.
    windows::install_matcher(matcher);
    windows::install_output_sender(output_tx);
    windows::install_log_sender(log_tx);
    windows::install_emergency_key(emergency_key);
    windows::install_output_ack_receiver(ack_rx);

    let shutdown = Arc::new(AtomicBool::new(false));

    let logger_shutdown = Arc::clone(&shutdown);
    let logger = std::thread::spawn(move || logger_loop(log_rx, logger_shutdown));

    let output_shutdown = Arc::clone(&shutdown);
    let output_worker = std::thread::spawn(move || output_loop(output_rx, ack_tx, output_shutdown));

    let (ready_tx, ready_rx) = mpsc::channel::<Result<u32, String>>();
    let hook = std::thread::spawn(move || windows::run_hook_thread(ready_tx));

    let hook_thread_id = match ready_rx.recv() {
        Ok(Ok(tid)) => {
            println!("probe-cli: low-level hooks installed on message-loop thread {tid}.");
            println!(
                "probe-cli: {rule_count} rule(s) loaded from `{}`.",
                config_path.display()
            );
            println!("probe-cli: emergency bypass key `{emergency_key}`.");
            println!("probe-cli: type `pause`, `resume`, `stats`, or `quit`.");
            let _ = io::stdout().flush();
            tid
        }
        Ok(Err(err)) => {
            eprintln!("probe-cli: failed to install hooks: {err}");
            shutdown.store(true, Ordering::Relaxed);
            let _ = hook.join();
            let _ = logger.join();
            let _ = output_worker.join();
            marker.clear();
            std::process::exit(1);
        }
        Err(_) => {
            eprintln!("probe-cli: hook thread ended unexpectedly.");
            shutdown.store(true, Ordering::Relaxed);
            let _ = hook.join();
            let _ = logger.join();
            let _ = output_worker.join();
            marker.clear();
            std::process::exit(1);
        }
    };

    run_console_loop();

    // Ask the hook thread to uninstall its hooks and stop.
    if !windows::post_quit(hook_thread_id) {
        eprintln!("probe-cli: warning: failed to post WM_QUIT to hook thread {hook_thread_id}.");
    }
    if hook.join().is_err() {
        eprintln!("probe-cli: hook thread panicked.");
    }

    // Stop the logger and output worker only after the hook thread has exited.
    shutdown.store(true, Ordering::Relaxed);
    if logger.join().is_err() {
        eprintln!("probe-cli: logger thread panicked.");
    }
    if output_worker.join().is_err() {
        eprintln!("probe-cli: output worker thread panicked.");
    }

    marker.clear();

    println!(
        "probe-cli: shut down cleanly. observed {} events; {} output batch(es) sent, {} failed, {} dropped.",
        windows::seq_count(),
        windows::output_sent(),
        windows::output_failed(),
        windows::output_dropped(),
    );
    print_stats();
    let _ = io::stdout().flush();
}

/// Parse command-line arguments; returns `(config_path, debug)`.
fn parse_args() -> (PathBuf, bool) {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut config_path = default_config_path();
    let mut debug = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--config" => {
                i += 1;
                match args.get(i) {
                    Some(path) => config_path = PathBuf::from(path),
                    None => {
                        eprintln!("probe-cli: `--config` requires a path argument.");
                        std::process::exit(2);
                    }
                }
            }
            "--debug" => debug = true,
            "--print-default-config" => {
                println!("{}", to_json_pretty(&default_config()));
                std::process::exit(0);
            }
            "--help" | "-h" => {
                print_usage();
                std::process::exit(0);
            }
            other => {
                eprintln!("probe-cli: unknown argument `{other}`");
                print_usage();
                std::process::exit(2);
            }
        }
        i += 1;
    }

    (config_path, debug)
}

/// The default config file path under `%LOCALAPPDATA%`.
fn default_config_path() -> PathBuf {
    if let Some(base) = env::var_os("LOCALAPPDATA") {
        PathBuf::from(base).join("InputFlow").join("config.json")
    } else {
        PathBuf::from("inputflow.json")
    }
}

fn print_usage() {
    println!("probe-cli: InputFlow engine probe (M6).");
    println!("usage: probe-cli [--config PATH] [--debug] [--print-default-config] [--help]");
    println!("  --config PATH           config file (default: %LOCALAPPDATA%\\InputFlow\\config.json)");
    println!("  --debug                 enable per-event debug logging");
    println!("  --print-default-config  print a default config template and exit");
}

/// A best-effort crash marker: written on startup, removed on clean exit, so a
/// subsequent run can warn about a previous abnormal termination.
struct CrashMarker {
    path: PathBuf,
}

impl CrashMarker {
    fn new() -> Self {
        let dir = if let Some(base) = env::var_os("LOCALAPPDATA") {
            PathBuf::from(base).join("InputFlow")
        } else {
            PathBuf::from(".")
        };
        let _ = fs::create_dir_all(&dir);
        Self { path: dir.join("running") }
    }

    fn warning_if_previous_abnormal(&self) -> Option<String> {
        if self.path.exists() {
            Some(
                "previous session ended abnormally (e.g. was killed); held input was not recovered"
                    .to_string(),
            )
        } else {
            None
        }
    }

    fn mark_running(&self) {
        let _ = fs::write(&self.path, format!("{}\n", std::process::id()));
    }

    fn clear(&self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// Block reading stdin until `quit`/`exit`/`q`/EOF; handle pause/resume/stats.
fn run_console_loop() {
    let stdin = io::stdin();
    let mut line = String::new();
    loop {
        line.clear();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => break, // EOF
            Ok(_) => match line.trim().to_ascii_lowercase().as_str() {
                "quit" | "exit" | "q" => break,
                "pause" => {
                    if windows::is_suspended() {
                        println!("probe-cli: already paused (interception stopped).");
                    } else {
                        windows::suspend();
                        println!("probe-cli: paused; held input flushed, interception stopped.");
                    }
                }
                "resume" => {
                    if windows::is_suspended() {
                        windows::resume();
                        println!("probe-cli: resumed; interception active.");
                    } else {
                        println!("probe-cli: already running.");
                    }
                }
                "stats" => print_stats(),
                "" => {}
                other => {
                    println!(
                        "probe-cli: unknown command `{other}` (pause | resume | stats | quit)."
                    );
                }
            },
            Err(err) => {
                eprintln!("probe-cli: stdin read error: {err}");
                break;
            }
        }
    }
}

fn print_stats() {
    match windows::callback_latency_stats() {
        Some((total, p50, p95, p99)) => println!(
            "probe-cli: callback latency (us): total={total} p50={} p95={} p99={}",
            fmt_opt(p50),
            fmt_opt(p95),
            fmt_opt(p99)
        ),
        None => println!("probe-cli: callback latency: no samples."),
    }
    match windows::hold_delay_stats() {
        Some((total, p50, p95, p99)) => println!(
            "probe-cli: hold delay (us): total={total} p50={} p95={} p99={}",
            fmt_opt(p50),
            fmt_opt(p95),
            fmt_opt(p99)
        ),
        None => println!("probe-cli: hold delay: no samples."),
    }
}

fn fmt_opt(value: Option<u64>) -> String {
    value.map(|n| n.to_string()).unwrap_or_else(|| "-".to_string())
}

/// Print diagnostic lines as they arrive; stop when asked and after draining.
fn logger_loop(rx: mpsc::Receiver<String>, shutdown: Arc<AtomicBool>) {
    loop {
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(line) => print_line(&format!("probe-cli: {line}")),
            Err(RecvTimeoutError::Timeout) => {
                if shutdown.load(Ordering::Relaxed) {
                    while let Ok(line) = rx.try_recv() {
                        print_line(&format!("probe-cli: {line}"));
                    }
                    break;
                }
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

/// Output worker: execute each replay/action command via `SendInput` and then
/// acknowledge completion so the hook thread's ordering barrier can proceed.
/// Drains remaining commands on shutdown so a clean-exit flush is not dropped.
fn output_loop(
    rx: mpsc::Receiver<Command>,
    ack_tx: mpsc::Sender<()>,
    shutdown: Arc<AtomicBool>,
) {
    loop {
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(command) => {
                let inserted = windows::execute(&command);
                print_line(&format!("probe-cli: output: {inserted} event(s) inserted"));
                let _ = ack_tx.send(());
            }
            Err(RecvTimeoutError::Timeout) => {
                if shutdown.load(Ordering::Relaxed) {
                    while let Ok(command) = rx.try_recv() {
                        let inserted = windows::execute(&command);
                        print_line(&format!("probe-cli: output: {inserted} event(s) inserted"));
                        let _ = ack_tx.send(());
                    }
                    break;
                }
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

/// Write one line to stdout and flush it, holding the stdout lock only briefly.
fn print_line(line: &str) {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    let _ = writeln!(out, "{line}");
    let _ = out.flush();
}

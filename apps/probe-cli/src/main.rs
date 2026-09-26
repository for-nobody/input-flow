//! probe-cli: a Windows keyboard/mouse low-level hook probe driven by the
//! InputFlow engine (M5).
//!
//! It installs `WH_KEYBOARD_LL` and `WH_MOUSE_LL` on a dedicated message-loop
//! thread. Each normalized event is fed to the engine matcher synchronously in
//! the callback: suppressed events are withheld, matches emit an action, and
//! failures replay the held events (both via `SendInput` on a worker thread).
//! F12 toggles bypass; type `quit` and press Enter to exit.

use std::io::{self, BufRead, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

use inputflow_engine::{
    Action, Command, Key, Matcher, MouseButton, Rule, RuleIndex, SystemClock, Trigger,
};
use inputflow_windows::platform::windows;

/// Bounded capacity for the diagnostic-log channel.
const LOG_QUEUE_CAPACITY: usize = 1024;
/// Bounded capacity for the output-command channel (replay / action).
const OUTPUT_QUEUE_CAPACITY: usize = 8;

fn main() {
    let (log_tx, log_rx) = mpsc::sync_channel::<String>(LOG_QUEUE_CAPACITY);
    let (output_tx, output_rx) = mpsc::sync_channel::<Command>(OUTPUT_QUEUE_CAPACITY);

    // M5 demo rule: hold LeftCtrl for 250 ms then click the right mouse button
    // to send Ctrl+C.
    let rules = vec![Rule {
        id: "hold-ctrl-right-click-copy".to_string(),
        trigger: Trigger::HoldMouseButton {
            key: Key::LeftCtrl,
            timeout_ms: 250,
            button: MouseButton::Right,
        },
        action: Action::KeyChord(vec![Key::LeftCtrl, Key::C]),
    }];
    let index = match RuleIndex::compile(rules) {
        Ok(index) => index,
        Err(errors) => {
            for error in errors {
                eprintln!("probe-cli: invalid rule: {error}");
            }
            std::process::exit(1);
        }
    };
    let matcher = Matcher::new(Box::new(SystemClock::new()), index, 16);

    // Make the matcher and senders visible to the hook callbacks before any
    // hook is installed.
    windows::install_matcher(matcher);
    windows::install_output_sender(output_tx);
    windows::install_log_sender(log_tx);

    let shutdown = Arc::new(AtomicBool::new(false));

    let logger_shutdown = Arc::clone(&shutdown);
    let logger = std::thread::spawn(move || logger_loop(log_rx, logger_shutdown));

    let output_shutdown = Arc::clone(&shutdown);
    let output_worker = std::thread::spawn(move || output_loop(output_rx, output_shutdown));

    let (ready_tx, ready_rx) = mpsc::channel::<Result<u32, String>>();
    let hook = std::thread::spawn(move || windows::run_hook_thread(ready_tx));

    let hook_thread_id = match ready_rx.recv() {
        Ok(Ok(tid)) => {
            println!("probe-cli: low-level hooks installed on message-loop thread {tid}.");
            println!("probe-cli: hold LeftCtrl for 250 ms then right-click to send Ctrl+C.");
            println!(
                "probe-cli: non-matching chords (e.g. Ctrl+Q) are replayed in order on failure."
            );
            println!("probe-cli: F12 toggles bypass; type `quit` and press Enter to exit.");
            let _ = io::stdout().flush();
            tid
        }
        Ok(Err(err)) => {
            eprintln!("probe-cli: failed to install hooks: {err}");
            shutdown.store(true, Ordering::Relaxed);
            let _ = hook.join();
            let _ = logger.join();
            let _ = output_worker.join();
            std::process::exit(1);
        }
        Err(_) => {
            eprintln!("probe-cli: hook thread ended unexpectedly.");
            shutdown.store(true, Ordering::Relaxed);
            let _ = hook.join();
            let _ = logger.join();
            let _ = output_worker.join();
            std::process::exit(1);
        }
    };

    wait_for_quit_command();

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

    println!(
        "probe-cli: shut down cleanly. observed {} events; {} output batch(es) sent, {} failed, {} dropped.",
        windows::seq_count(),
        windows::output_sent(),
        windows::output_failed(),
        windows::output_dropped(),
    );
    let _ = io::stdout().flush();
}

/// Block reading stdin until the user types `quit`/`exit`/`q` or EOF arrives.
fn wait_for_quit_command() {
    let stdin = io::stdin();
    let mut line = String::new();
    loop {
        line.clear();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => break, // EOF
            Ok(_) => {
                let command = line.trim().to_ascii_lowercase();
                if command == "quit" || command == "exit" || command == "q" {
                    break;
                }
                println!("probe-cli: unknown command `{command}` (type `quit` to exit).");
            }
            Err(err) => {
                eprintln!("probe-cli: stdin read error: {err}");
                break;
            }
        }
    }
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

/// Output worker: execute each replay/action command via `SendInput`.
fn output_loop(rx: mpsc::Receiver<Command>, shutdown: Arc<AtomicBool>) {
    loop {
        if shutdown.load(Ordering::Relaxed) {
            break;
        }
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(command) => {
                let inserted = windows::execute(&command);
                print_line(&format!("probe-cli: output: {inserted} event(s) inserted"));
            }
            Err(RecvTimeoutError::Timeout) => {}
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

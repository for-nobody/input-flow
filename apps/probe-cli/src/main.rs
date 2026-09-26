//! probe-cli: a read-only Windows keyboard/mouse low-level hook probe.
//!
//! It installs `WH_KEYBOARD_LL` and `WH_MOUSE_LL` on a dedicated message-loop
//! thread, normalizes events in the hook callback, pushes them through a bounded
//! channel, and prints them on a dedicated logger thread. It never suppresses,
//! replays, or synthesizes input. Type `quit` and press Enter to exit cleanly.

mod event;
mod platform;

use std::io::{self, BufRead, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

use event::InputEvent;
use platform::windows;

/// Bounded capacity for the event channel between hook callbacks and the logger.
const EVENT_QUEUE_CAPACITY: usize = 1024;

fn main() {
    let (tx, rx) = mpsc::sync_channel::<InputEvent>(EVENT_QUEUE_CAPACITY);

    // Make the sender visible to the hook callbacks before any hook is installed.
    windows::install_event_sender(tx);

    let shutdown = Arc::new(AtomicBool::new(false));
    let logger_shutdown = Arc::clone(&shutdown);
    let logger = std::thread::spawn(move || logger_loop(rx, logger_shutdown));

    let (ready_tx, ready_rx) = mpsc::channel::<Result<u32, String>>();
    let hook = std::thread::spawn(move || windows::run_hook_thread(ready_tx));

    let hook_thread_id = match ready_rx.recv() {
        Ok(Ok(tid)) => {
            println!("probe-cli: low-level hooks installed on message-loop thread {tid}.");
            println!("probe-cli: read-only keyboard/mouse probe running.");
            println!("probe-cli: type `quit` and press Enter to exit cleanly.");
            // Flush so the instructions are visible even when stdout is redirected.
            let _ = io::stdout().flush();
            tid
        }
        Ok(Err(err)) => {
            eprintln!("probe-cli: failed to install hooks: {err}");
            shutdown.store(true, Ordering::Relaxed);
            let _ = hook.join();
            let _ = logger.join();
            std::process::exit(1);
        }
        Err(_) => {
            eprintln!("probe-cli: hook thread ended unexpectedly.");
            shutdown.store(true, Ordering::Relaxed);
            let _ = hook.join();
            let _ = logger.join();
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

    // Stop the logger only after the hook thread has exited, so the channel has
    // no remaining producers and the logger can drain before it stops.
    shutdown.store(true, Ordering::Relaxed);
    if logger.join().is_err() {
        eprintln!("probe-cli: logger thread panicked.");
    }

    let total = windows::seq_count();
    let dropped = windows::dropped_count();
    println!(
        "probe-cli: shut down cleanly. observed {total} events, dropped {dropped} (queue full)."
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

/// Print normalized events as they arrive; stop when asked and after draining.
fn logger_loop(rx: mpsc::Receiver<InputEvent>, shutdown: Arc<AtomicBool>) {
    loop {
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(event) => {
                let line = event.to_string();
                print_line(&line);
            }
            Err(RecvTimeoutError::Timeout) => {
                if shutdown.load(Ordering::Relaxed) {
                    // Drain anything still buffered before exiting.
                    while let Ok(event) = rx.try_recv() {
                        let line = event.to_string();
                        print_line(&line);
                    }
                    break;
                }
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

/// Write one line to stdout and flush it, holding the stdout lock only briefly so
/// other threads (such as the main thread's banner) are never blocked indefinitely.
fn print_line(line: &str) {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    let _ = writeln!(out, "{line}");
    let _ = out.flush();
}

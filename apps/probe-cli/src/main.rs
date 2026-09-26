//! probe-cli: a Windows keyboard/mouse low-level hook probe with F8 suppression
//! and delayed replay (M2).
//!
//! It installs `WH_KEYBOARD_LL` and `WH_MOUSE_LL` on a dedicated message-loop
//! thread. In the keyboard callback, physical F8 down/up is suppressed and a
//! delayed, tag-marked F8 down+up pair is replayed via `SendInput` on a worker
//! thread; all other input passes through. The `dwExtraInfo` tag lets the
//! callback recognize its own injected events, so replay never recurses. F12
//! toggles bypass (stop intercepting). Type `quit` and press Enter to exit.

mod event;
mod platform;

use std::io::{self, BufRead, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

use event::HookMessage;
use platform::windows;

/// Bounded capacity for the event channel between hook callbacks and the logger.
const EVENT_QUEUE_CAPACITY: usize = 1024;
/// Bounded capacity for the delayed-replay request channel.
const REPLAY_QUEUE_CAPACITY: usize = 8;
/// Delay between suppressing an F8 down and replaying it.
const REPLAY_DELAY: Duration = Duration::from_millis(300);

fn main() {
    let (tx, rx) = mpsc::sync_channel::<HookMessage>(EVENT_QUEUE_CAPACITY);
    let (replay_tx, replay_rx) =
        mpsc::sync_channel::<windows::ReplayRequest>(REPLAY_QUEUE_CAPACITY);

    // Make the senders visible to the hook callbacks before any hook is installed.
    windows::install_event_sender(tx);
    windows::install_replay_sender(replay_tx);

    let shutdown = Arc::new(AtomicBool::new(false));

    let logger_shutdown = Arc::clone(&shutdown);
    let logger = std::thread::spawn(move || logger_loop(rx, logger_shutdown));

    let replay_shutdown = Arc::clone(&shutdown);
    let replay_worker = std::thread::spawn(move || replay_loop(replay_rx, replay_shutdown));

    let (ready_tx, ready_rx) = mpsc::channel::<Result<u32, String>>();
    let hook = std::thread::spawn(move || windows::run_hook_thread(ready_tx));

    let hook_thread_id = match ready_rx.recv() {
        Ok(Ok(tid)) => {
            println!("probe-cli: low-level hooks installed on message-loop thread {tid}.");
            println!("probe-cli: F8 is held ~300ms then replayed once; all other keys pass through.");
            println!("probe-cli: F12 toggles bypass (stop intercepting); type `quit` and press Enter to exit.");
            // Flush so the instructions are visible even when stdout is redirected.
            let _ = io::stdout().flush();
            tid
        }
        Ok(Err(err)) => {
            eprintln!("probe-cli: failed to install hooks: {err}");
            shutdown.store(true, Ordering::Relaxed);
            let _ = hook.join();
            let _ = logger.join();
            let _ = replay_worker.join();
            std::process::exit(1);
        }
        Err(_) => {
            eprintln!("probe-cli: hook thread ended unexpectedly.");
            shutdown.store(true, Ordering::Relaxed);
            let _ = hook.join();
            let _ = logger.join();
            let _ = replay_worker.join();
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

    // Stop the logger and replay worker only after the hook thread has exited, so
    // no more producers exist. The logger drains remaining messages; the replay
    // worker discards pending requests (no input is synthesized during exit).
    shutdown.store(true, Ordering::Relaxed);
    if logger.join().is_err() {
        eprintln!("probe-cli: logger thread panicked.");
    }
    if replay_worker.join().is_err() {
        eprintln!("probe-cli: replay worker thread panicked.");
    }

    let total = windows::seq_count();
    let dropped = windows::dropped_count();
    let replayed = windows::replay_sent();
    let replay_failed = windows::replay_failed();
    let replay_dropped = windows::replay_dropped();
    println!(
        "probe-cli: shut down cleanly. observed {total} events, dropped {dropped} (queue full); replayed {replayed} F8, {replay_failed} failed, {replay_dropped} replay requests dropped."
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

/// Print messages as they arrive; stop when asked and after draining.
fn logger_loop(rx: mpsc::Receiver<HookMessage>, shutdown: Arc<AtomicBool>) {
    loop {
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(message) => print_hook_message(&message),
            Err(RecvTimeoutError::Timeout) => {
                if shutdown.load(Ordering::Relaxed) {
                    // Drain anything still buffered before exiting.
                    while let Ok(message) = rx.try_recv() {
                        print_hook_message(&message);
                    }
                    break;
                }
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

/// Render one hook message (an input event or a bypass note) to stdout.
fn print_hook_message(message: &HookMessage) {
    match message {
        HookMessage::Input(event) => print_line(&event.to_string()),
        HookMessage::Bypass(on) => print_line(&format!(
            "probe-cli: bypass {}.",
            if *on {
                "ON (interception stopped)"
            } else {
                "OFF (interception active)"
            }
        )),
    }
}

/// Replay worker: for each suppressed F8 down, wait a short delay, then synthesize
/// one tagged down+up pair. A `SendInput` failure sets bypass. Pending requests
/// are discarded once shutdown begins.
fn replay_loop(rx: mpsc::Receiver<windows::ReplayRequest>, shutdown: Arc<AtomicBool>) {
    loop {
        if shutdown.load(Ordering::Relaxed) {
            break;
        }
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(req) => {
                std::thread::sleep(REPLAY_DELAY);
                if shutdown.load(Ordering::Relaxed) {
                    break;
                }
                let inserted = windows::replay_held_key();
                if inserted == 2 {
                    print_line(&format!(
                        "probe-cli: replayed F8 (seq={:06}); SendInput inserted 2/2 events.",
                        req.seq
                    ));
                } else {
                    print_line(&format!(
                        "probe-cli: replay failed (seq={:06}); SendInput inserted {inserted}/2 events; entering bypass.",
                        req.seq
                    ));
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
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

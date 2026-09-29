//! Diagnostic console for the shared InputFlow runtime.

use std::env;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use inputflow_config::{default_config, to_json_pretty};
use inputflow_runtime::{Runtime, RuntimeOptions, RuntimeStatus, default_data_dir};

fn main() {
    let (config_path, debug) = parse_args();
    let sink = Arc::new(|line: String| print_line(&format!("probe-cli: {line}")));
    let mut options = RuntimeOptions::new(sink);
    options.config_path = config_path.clone();
    options.crash_marker_path = default_data_dir().join("probe.running");
    options.debug_input = debug;

    let (runtime, start) = match Runtime::start(options) {
        Ok(started) => started,
        Err(error) => {
            eprintln!("probe-cli: failed to start runtime: {error}");
            std::process::exit(1);
        }
    };
    if start.previous_abnormal_termination {
        eprintln!(
            "probe-cli: warning: previous probe session ended abnormally; held input was not recovered"
        );
    }
    for problem in &start.config_problems {
        eprintln!("probe-cli: warning: {problem}");
    }
    println!(
        "probe-cli: low-level hooks installed on message-loop thread {}.",
        start.hook_thread_id
    );
    println!(
        "probe-cli: {} rule(s) loaded from `{}`.",
        start.rule_count,
        config_path.display()
    );
    println!("probe-cli: emergency bypass key `{}`.", start.emergency_key);
    println!("probe-cli: type `pause`, `resume`, `stats`, or `quit`.");
    let _ = io::stdout().flush();

    run_console_loop(&runtime);
    let report = runtime.shutdown();
    println!(
        "probe-cli: shut down cleanly. observed {} events; {} output batch(es) sent, {} failed, {} dropped.",
        report.observed_events,
        report.output_batches_sent,
        report.output_batches_failed,
        report.output_batches_dropped,
    );
    print_stats(&runtime.status());
    let _ = io::stdout().flush();
}

fn parse_args() -> (PathBuf, bool) {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut config_path = default_data_dir().join("config.json");
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

fn print_usage() {
    println!("probe-cli: InputFlow shared-runtime diagnostic console.");
    println!("usage: probe-cli [--config PATH] [--debug] [--print-default-config] [--help]");
    println!(
        "  --config PATH           config file (default: %LOCALAPPDATA%\\InputFlow\\config.json)"
    );
    println!("  --debug                 enable per-event debug logging");
    println!("  --print-default-config  print a default config template and exit");
}

fn run_console_loop(runtime: &Runtime) {
    let stdin = io::stdin();
    let mut line = String::new();
    loop {
        line.clear();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => match line.trim().to_ascii_lowercase().as_str() {
                "quit" | "exit" | "q" => break,
                "pause" => match runtime.pause() {
                    Ok(report) => println!(
                        "probe-cli: paused; held_events={} inserted={} requested={} output_complete={} last_error={}; interception stopped.",
                        report.held_events,
                        report.inserted_inputs,
                        report.requested_inputs,
                        report.output_complete,
                        report.last_error
                    ),
                    Err(error) => eprintln!("probe-cli: pause failed: {error}"),
                },
                "resume" => match runtime.resume() {
                    Ok(()) => println!("probe-cli: resumed; interception active."),
                    Err(error) => eprintln!("probe-cli: resume failed: {error}"),
                },
                "stats" => print_stats(&runtime.status()),
                "" => {}
                other => println!(
                    "probe-cli: unknown command `{other}` (pause | resume | stats | quit)."
                ),
            },
            Err(error) => {
                eprintln!("probe-cli: stdin read error: {error}");
                break;
            }
        }
    }
}

fn print_stats(status: &RuntimeStatus) {
    let query_start = Instant::now();
    match status.callback_latency_us {
        Some((total, p50, p95, p99, max)) => println!(
            "probe-cli: callback observed duration (us): total={total} p50={} p95={} p99={} max={}",
            fmt_opt(p50),
            fmt_opt(p95),
            fmt_opt(p99),
            fmt_opt(max)
        ),
        None => println!("probe-cli: callback observed duration: no samples."),
    }
    match status.hold_delay_us {
        Some((total, p50, p95, p99, max)) => println!(
            "probe-cli: hold delay (us): total={total} p50={} p95={} p99={} max={}",
            fmt_opt(p50),
            fmt_opt(p95),
            fmt_opt(p99),
            fmt_opt(max)
        ),
        None => println!("probe-cli: hold delay: no samples."),
    }
    println!(
        "probe-cli: status phase={:?} suspended={} capture_active={} rules={} output_sent={} output_failed={} output_dropped={}",
        status.phase,
        status.suspended,
        status.capture_active,
        status.rule_count,
        status.output_batches_sent,
        status.output_batches_failed,
        status.output_batches_dropped
    );
    println!(
        "probe-cli: stats formatting duration (us): {}",
        query_start.elapsed().as_micros()
    );
}

fn fmt_opt(value: Option<u64>) -> String {
    value
        .map(|number| number.to_string())
        .unwrap_or_else(|| "-".to_string())
}

fn print_line(line: &str) {
    let stdout = io::stdout();
    let mut output = stdout.lock();
    let _ = writeln!(output, "{line}");
    let _ = output.flush();
}

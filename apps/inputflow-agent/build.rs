use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let manifest = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap())
        .join("inputflow-agent.manifest");
    println!("cargo:rerun-if-changed={}", manifest.display());

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rustc-link-arg-bin=inputflow-agent=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg-bin=inputflow-agent=/MANIFESTINPUT:{}",
            manifest.display()
        );
        link_application_icon();
    }
}

fn link_application_icon() {
    let icon = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap())
        .join("../../installer/InputFlow.ico");
    println!("cargo:rerun-if-changed={}", icon.display());
    if !icon.is_file() {
        panic!("InputFlow icon is missing: {}", icon.display());
    }

    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let rc_path = out_dir.join("inputflow-agent-icon.rc");
    let res_path = out_dir.join("inputflow-agent-icon.res");
    let icon_rc = icon.display().to_string().replace('\\', "\\\\");
    std::fs::write(&rc_path, format!("1 ICON \"{icon_rc}\"\n"))
        .unwrap_or_else(|error| panic!("could not write {}: {error}", rc_path.display()));

    let rc = find_rc().unwrap_or_else(|| {
        panic!("rc.exe was not found. Install the Windows SDK resource compiler.")
    });
    let status = Command::new(&rc)
        .arg("/nologo")
        .arg("/fo")
        .arg(&res_path)
        .arg(&rc_path)
        .status()
        .unwrap_or_else(|error| panic!("could not run {}: {error}", rc.display()));
    if !status.success() {
        panic!("rc.exe failed with status {status}");
    }
    println!("cargo:rustc-link-arg-bin=inputflow-agent={}", res_path.display());
}

fn find_rc() -> Option<PathBuf> {
    let kits = Path::new(r"C:\Program Files (x86)\Windows Kits\10\bin");
    let mut versions = std::fs::read_dir(kits).ok()?.filter_map(Result::ok).collect::<Vec<_>>();
    versions.sort_by(|left, right| right.file_name().cmp(&left.file_name()));
    versions.into_iter().map(|entry| entry.path().join("x64").join("rc.exe")).find(|rc| rc.is_file())
}

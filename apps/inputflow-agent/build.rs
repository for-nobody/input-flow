use std::path::PathBuf;

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
    }
}

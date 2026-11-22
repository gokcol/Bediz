use std::{env, path::PathBuf, process::Command};

fn main() {
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    let sources = ["src/video/Recorder.swift", "src/video/Player.swift"];
    for source in sources {
        println!("cargo:rerun-if-changed={source}");
    }
    println!("cargo:rerun-if-env-changed=DEVELOPER_DIR");
    println!("cargo:rerun-if-env-changed=SDKROOT");
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo output directory"));
    let arch = match env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("aarch64") => "arm64",
        Ok("x86_64") => "x86_64",
        _ => panic!("Unsupported macOS recording architecture"),
    };
    let sdk = Command::new("xcrun")
        .args(["--sdk", "macosx", "--show-sdk-path"])
        .output()
        .expect("Locate the macOS SDK");
    assert!(sdk.status.success(), "The macOS SDK is required");
    let sdk = String::from_utf8(sdk.stdout).expect("SDK path");
    let object = out.join("bediz_recorder.o");
    let result = Command::new("xcrun")
        .args([
            "--sdk",
            "macosx",
            "swiftc",
            "-swift-version",
            "5",
            "-parse-as-library",
            "-O",
            "-whole-module-optimization",
            "-emit-object",
            "-module-name",
            "BedizRecorder",
            "-target",
        ])
        .arg(format!("{arch}-apple-macosx14.0"))
        .args(["-sdk", sdk.trim(), "-module-cache-path"])
        .arg(out.join("swift-module-cache"))
        .arg("-o")
        .arg(&object)
        .args(sources)
        .status()
        .expect("Run the Swift compiler");
    assert!(result.success(), "Compile the native macOS recorder");
    let result = Command::new("ar")
        .arg("crs")
        .arg(out.join("libbediz_recorder.a"))
        .arg(&object)
        .status()
        .expect("Run ar");
    assert!(result.success(), "Archive the native macOS recorder");
    println!("cargo:rustc-link-search=native={}", out.display());
    println!(
        "cargo:rustc-link-search=native={}/usr/lib/swift",
        sdk.trim()
    );
    println!("cargo:rustc-link-lib=static=bediz_recorder");
    println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
}

use std::process::Command;

fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        link_compiler_rt();
    }
    tauri_build::build()
}

// whisper.cpp's Metal backend uses Objective-C `@available`, which clang lowers to a
// call to `__isPlatformVersionAtLeast` in compiler-rt. rustc links neither compiler-rt
// nor libclang_rt, so the link fails with an undefined symbol unless we add it here.
fn link_compiler_rt() {
    let out = Command::new("xcrun")
        .args(["clang", "-print-runtime-dir"])
        .output()
        .expect("running `xcrun clang -print-runtime-dir` failed — install the Xcode Command Line Tools");
    let dir = String::from_utf8(out.stdout).expect("non-UTF8 runtime dir");
    let dir = dir.trim();
    assert!(!dir.is_empty(), "clang reported no runtime dir");

    println!("cargo:rustc-link-search=native={dir}");
    println!("cargo:rustc-link-lib=static=clang_rt.osx");

    // The microphone permission check looks up AVCaptureDevice by name at runtime.
    // Nothing else in the binary pulls AVFoundation in, and without the framework
    // the lookup fails and every mic status reads as "unknown".
    println!("cargo:rustc-link-lib=framework=AVFoundation");
}

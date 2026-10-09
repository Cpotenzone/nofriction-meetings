// Nano Banana Meetings - Build Script
// Configures Swift runtime linking and macOS frameworks for ScreenCaptureKit

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    #[cfg(target_os = "macos")]
    {
        // Link macOS audio and capture frameworks
        println!("cargo:rustc-link-lib=framework=AVFoundation");
        println!("cargo:rustc-link-lib=framework=CoreAudio");
        println!("cargo:rustc-link-lib=framework=AudioToolbox");
        println!("cargo:rustc-link-lib=framework=ScreenCaptureKit");
        println!("cargo:rustc-link-lib=framework=CoreMedia");
        // calendar_client.rs looks up EKEventStore by name through the ObjC
        // runtime. Link EventKit explicitly instead of relying on another
        // framework happening to load it (the binary didn't list it)
        println!("cargo:rustc-link-lib=framework=EventKit");

        // Add Swift library search paths
        // The system Swift libraries are in /usr/lib/swift
        println!("cargo:rustc-link-search=/usr/lib/swift");

        // Add rpath for Swift concurrency library at runtime
        // This tells the dynamic linker where to find libswift_Concurrency.dylib
        println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");

        // Also check Xcode toolchain path (for development)
        if let Ok(developer_dir) = std::process::Command::new("xcode-select")
            .arg("-p")
            .output()
        {
            if let Ok(path) = String::from_utf8(developer_dir.stdout) {
                let toolchain_swift = format!(
                    "{}/Toolchains/XcodeDefault.xctoolchain/usr/lib/swift/macosx",
                    path.trim()
                );
                println!("cargo:rustc-link-search={}", toolchain_swift);
                println!("cargo:rustc-link-arg=-Wl,-rpath,{}", toolchain_swift);
            }
        }
    }

    #[cfg(target_os = "macos")]
    build_swift_bridge();

    tauri_build::build()
}

/// Compile swift/NoFrictionBridge (C ABI via @_cdecl) into a static library
/// for the *target* arch and link it. StoreKit code is included only for the
/// Mac App Store flavor (`--features mas` → -DNF_STOREKIT); the Foundation
/// Models (Apple on-device AI) part is built for both flavors and the
/// framework is weak-linked so the app still launches on macOS < 26.
#[cfg(target_os = "macos")]
fn build_swift_bridge() {
    use std::path::PathBuf;
    use std::process::Command;

    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let src_dir = manifest.join("swift/NoFrictionBridge/Sources/NoFrictionBridge");
    println!("cargo:rerun-if-changed={}", src_dir.display());
    let mut sources: Vec<PathBuf> = std::fs::read_dir(&src_dir)
        .expect("swift bridge sources")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().map_or(false, |x| x == "swift"))
        .collect();
    sources.sort();
    for s in &sources {
        println!("cargo:rerun-if-changed={}", s.display());
    }

    let mas = std::env::var_os("CARGO_FEATURE_MAS").is_some();
    let arch = match std::env::var("CARGO_CFG_TARGET_ARCH").unwrap().as_str() {
        "aarch64" => "arm64".to_string(),
        a => a.to_string(),
    };
    // Keep in sync with bundle.macOS.minimumSystemVersion
    let min_macos = "12.3";
    let target = format!("{}-apple-macos{}", arch, min_macos);
    let sdk = Command::new("xcrun")
        .args(["--sdk", "macosx", "--show-sdk-path"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .expect("xcrun --sdk macosx --show-sdk-path failed (install Xcode)");
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let lib = out_dir.join("libNoFrictionBridge.a");
    let debug = std::env::var("PROFILE").map(|p| p == "debug").unwrap_or(true);

    let mut cmd = Command::new("xcrun");
    cmd.args(["swiftc", "-parse-as-library", "-emit-library", "-static"])
        .args(["-module-name", "NoFrictionBridge"])
        .args(["-target", &target, "-sdk", &sdk])
        .args(["-module-cache-path", &out_dir.join("swift-module-cache").display().to_string()])
        // FoundationModels is weak-linked below; don't let autolink make it strong
        .args(["-Xfrontend", "-disable-autolink-framework", "-Xfrontend", "FoundationModels"])
        .arg("-o")
        .arg(&lib);
    if debug {
        cmd.arg("-Onone");
    } else {
        cmd.args(["-O", "-whole-module-optimization"]);
    }
    if mas {
        cmd.arg("-DNF_STOREKIT");
    }
    cmd.args(&sources);
    let status = cmd.status().expect("failed to run swiftc");
    if !status.success() {
        panic!("Swift bridge failed to compile (swift/NoFrictionBridge)");
    }

    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-lib=static=NoFrictionBridge");
    println!("cargo:rustc-link-lib=framework=Foundation");
    // Notifications.swift: permission request + app-activation observer
    println!("cargo:rustc-link-lib=framework=UserNotifications");
    println!("cargo:rustc-link-lib=framework=AppKit");
    if mas {
        println!("cargo:rustc-link-lib=framework=StoreKit");
    }
    println!("cargo:rustc-link-arg=-Wl,-weak_framework,FoundationModels");
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_MAS");
}

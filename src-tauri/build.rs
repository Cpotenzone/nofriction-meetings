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

    tauri_build::build()
}

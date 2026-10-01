// swift-tools-version:5.9
// noFriction Swift bridge: StoreKit 2 (Mac App Store build) and Apple
// Foundation Models (on-device AI). Exposes a plain C ABI (@_cdecl) that
// src-tauri/src/store.rs calls. build.rs compiles these sources directly
// with swiftc (per target arch; -DNF_STOREKIT for `--features mas`), so this
// manifest exists for editing/type-checking in Xcode:
//   open src-tauri/swift/NoFrictionBridge/Package.swift
import PackageDescription

let package = Package(
    name: "NoFrictionBridge",
    platforms: [.macOS("12.3")],
    products: [
        .library(name: "NoFrictionBridge", type: .static, targets: ["NoFrictionBridge"]),
    ],
    targets: [
        .target(
            name: "NoFrictionBridge",
            path: "Sources/NoFrictionBridge",
            swiftSettings: [.define("NF_STOREKIT")]
        ),
    ]
)

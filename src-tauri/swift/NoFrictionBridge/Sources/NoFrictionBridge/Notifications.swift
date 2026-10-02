// Notification permission + "app became active" observer.
//
// Notifications themselves are posted by tauri-plugin-notification. This
// file only (1) asks macOS for permission at a sensible moment (the first
// recording, not at launch) and (2) tells Rust when the app is activated,
// which is what clicking one of our notifications does — Rust then shows
// and focuses the main window (it may be hidden: closing it only hides it).
// Neither needs an entitlement and both work in the App Sandbox.

import AppKit
import Foundation
import UserNotifications

/// UNUserNotificationCenter throws an Objective-C exception when the process
/// isn't a bundled .app (e.g. `cargo run`/`tauri dev`), so only use it there.
private func nfIsBundledApp() -> Bool {
    Bundle.main.bundleIdentifier != nil && Bundle.main.bundleURL.pathExtension == "app"
}

/// Ask for alert/sound permission. Result: `{"ok":true,"granted":Bool,"asked":Bool}`.
/// macOS shows its prompt only the first time; later calls just report.
@_cdecl("nf_notifications_request")
public func nf_notifications_request(_ ctx: UnsafeMutableRawPointer?, _ cb: NFCallback) {
    let context = NFContext(ptr: ctx, cb: cb)
    guard nfIsBundledApp() else {
        context.send(["ok": true, "granted": false, "asked": false, "reason": "not_bundled"])
        return
    }
    UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound]) { granted, error in
        if let error = error {
            context.send(["ok": false, "error": "\(error.localizedDescription)"])
        } else {
            context.send(["ok": true, "granted": granted, "asked": true])
        }
    }
}

private final class NFActivationObserver: @unchecked Sendable {
    let cb: NFCallback
    var token: NSObjectProtocol?
    init(cb: NFCallback) { self.cb = cb }
}

private var nfActivationObserver: NFActivationObserver?

/// Call `cb(nil, "{}")` on the main thread every time the app becomes
/// active. Registering again replaces the previous observer.
@_cdecl("nf_observe_app_activation")
public func nf_observe_app_activation(_ cb: NFCallback) {
    let register = {
        if let old = nfActivationObserver?.token {
            NotificationCenter.default.removeObserver(old)
        }
        let observer = NFActivationObserver(cb: cb)
        observer.token = NotificationCenter.default.addObserver(
            forName: NSApplication.didBecomeActiveNotification,
            object: nil,
            queue: .main
        ) { _ in
            "{}".withCString { observer.cb(nil, $0) }
        }
        nfActivationObserver = observer
    }
    if Thread.isMainThread {
        register()
    } else {
        DispatchQueue.main.async(execute: register)
    }
}

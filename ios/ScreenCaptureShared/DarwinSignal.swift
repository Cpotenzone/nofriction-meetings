import Foundation

/// Darwin notifications between the app and the broadcast extension: a
/// name only, no payload (the data is in the App Group container).
final class DarwinSignal: @unchecked Sendable {
    private let lock = NSLock()
    private var handler: (() -> Void)?
    private var name: String?

    init() {}

    /// Call `handler` whenever `name` is posted (replaces an earlier observation).
    func observe(_ name: String, _ handler: @escaping () -> Void) {
        stop()
        lock.withLock {
            self.handler = handler
            self.name = name
        }
        CFNotificationCenterAddObserver(CFNotificationCenterGetDarwinNotifyCenter(), Unmanaged.passUnretained(self).toOpaque(),
                                        { _, observer, _, _, _ in
                                            guard let observer else { return }
                                            Unmanaged<DarwinSignal>.fromOpaque(observer).takeUnretainedValue().fire()
                                        }, name as CFString, nil, .deliverImmediately)
    }

    func stop() {
        let current = lock.withLock { () -> String? in
            defer { name = nil; handler = nil }
            return name
        }
        guard let current else { return }
        CFNotificationCenterRemoveObserver(CFNotificationCenterGetDarwinNotifyCenter(), Unmanaged.passUnretained(self).toOpaque(),
                                           CFNotificationName(current as CFString), nil)
    }

    private func fire() {
        let handler = lock.withLock { self.handler }
        handler?()
    }

    deinit { stop() }

    static func post(_ name: String) {
        CFNotificationCenterPostNotification(CFNotificationCenterGetDarwinNotifyCenter(), CFNotificationName(name as CFString),
                                             nil, nil, true)
    }
}

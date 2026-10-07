import Foundation
import WatchKit

// What a watch recording starts with, the Discreet display, and the haptics.
// Pure logic, tested in NoFrictionWatchTests (docs/WATCH_APP.md).

/// The Record flow's choices: what it is, how long, which notebook, and
/// whether the screen is Discreet. Starts that skip the flow (the App
/// Intent) use the remembered ones with no notebook.
struct WatchStartOptions: Equatable, Sendable {
    var kind: RecordingKind
    var limit: RecordingLimit
    var notebook: String?
    var discreet: Bool

    init(kind: RecordingKind = .default, limit: RecordingLimit = .noLimit, notebook: String? = nil, discreet: Bool = false) {
        self.kind = kind
        self.limit = limit
        self.notebook = Notebook.normalize(notebook)
        self.discreet = discreet
    }

    /// The last choices made on this watch (Meeting, no limit, not Discreet until chosen).
    static func remembered(_ defaults: UserDefaults = .standard) -> WatchStartOptions {
        WatchStartOptions(kind: RecordingKindStore.remembered(defaults), limit: RecordingLimitStore.remembered(defaults),
                          discreet: DiscreetSetting.isOn(defaults))
    }

    /// Remember the type, the length and the Discreet choice for next time
    /// (the notebook isn't: it starts at None each time, like the iPhone).
    func remember(_ defaults: UserDefaults = .standard) {
        RecordingKindStore.remember(kind, defaults)
        RecordingLimitStore.remember(limit, defaults)
        DiscreetSetting.set(discreet, defaults)
    }
}

/// Discreet: a low-distraction display for lectures and other places where a
/// lit, red recording screen distracts. It changes only how the watch looks
/// and feels while recording; the system microphone indicator still shows,
/// and the recording notice still applies. Remembered; off by default.
enum DiscreetSetting {
    static let key = "discreetRecording"

    static func isOn(_ defaults: UserDefaults = .standard) -> Bool { defaults.bool(forKey: key) }

    static func set(_ on: Bool, _ defaults: UserDefaults = .standard) { defaults.set(on, forKey: key) }
}

/// What the recording screen shows, decided from the mode and the display
/// state (pure; tested). The views only render it.
struct RecordingPresentation: Equatable {
    /// The faded logo: opacity between `low` and `high`, pulsing slowly
    /// when `pulses` (otherwise it stays at `low`).
    struct Logo: Equatable {
        var low: Double
        var high: Double
        var pulses: Bool
        /// One fade in and out, seconds
        var cycle: TimeInterval

        /// Opacity `time` seconds into the pulse: `low` at 0, `high` half a
        /// cycle later, a smooth (cosine) fade between. Static: `low`.
        func opacity(at time: TimeInterval) -> Double {
            guard pulses, cycle > 0 else { return low }
            let phase = (1 - cos(2 * Double.pi * time / cycle)) / 2
            return low + (high - low) * phase
        }
    }

    /// Red status, big clock, meter, Mark, Pause and Stop
    var showsStandardUI: Bool
    /// Discreet: the only sign of the recording (nil = no logo)
    var logo: Logo?
    /// Discreet: the time left (or elapsed) in small dim grey text
    var showsGlanceTime: Bool
    var glanceOpacity: Double
    /// Discreet: a tap anywhere on the screen marks ★
    var tapAnywhereMarks: Bool

    // Discreet levels: faint, never full brightness
    static let pulseLow = 0.12
    static let pulseHigh = 0.35
    static let pulseCycle: TimeInterval = 3.5
    /// Reduce Motion: a static faded logo, no pulse
    static let reducedMotionOpacity = 0.22
    /// Wrist down / Always On: only the logo, dimmer still, static
    static let alwaysOnOpacity = 0.07
    /// The brief brighten that confirms a mark (still faint)
    static let markFlashOpacity = 0.5
    static let markFlashSeconds: TimeInterval = 0.45
    /// How long the time stays after a tap or a wrist raise
    static let glanceSeconds: TimeInterval = 3
    static let glanceTextOpacity = 0.45

    /// - Parameters:
    ///   - discreet: the recording runs in Discreet mode
    ///   - luminanceReduced: wrist down / Always On (`isLuminanceReduced`)
    ///   - reduceMotion: the system's Reduce Motion setting
    ///   - paused: the recording is paused (the logo stops pulsing, "Paused" shows)
    ///   - glancing: within `glanceSeconds` of a tap or a wrist raise
    ///   - markFlash: within `markFlashSeconds` of a mark
    static func make(discreet: Bool, luminanceReduced: Bool, reduceMotion: Bool, paused: Bool = false,
                     glancing: Bool = false, markFlash: Bool = false) -> RecordingPresentation {
        guard discreet else {
            return RecordingPresentation(showsStandardUI: true, logo: nil, showsGlanceTime: false,
                                         glanceOpacity: 0, tapAnywhereMarks: false)
        }
        if luminanceReduced {
            // Only the logo, dimmer and still; a tap first wakes the screen
            return RecordingPresentation(showsStandardUI: false,
                                         logo: Logo(low: alwaysOnOpacity, high: alwaysOnOpacity, pulses: false, cycle: pulseCycle),
                                         showsGlanceTime: false, glanceOpacity: 0, tapAnywhereMarks: false)
        }
        if paused {
            // Nothing is being recorded: a still logo and the word "Paused"
            return RecordingPresentation(showsStandardUI: false,
                                         logo: Logo(low: reducedMotionOpacity, high: reducedMotionOpacity, pulses: false, cycle: pulseCycle),
                                         showsGlanceTime: true, glanceOpacity: glanceTextOpacity, tapAnywhereMarks: false)
        }
        let logo: Logo
        if markFlash {
            logo = Logo(low: markFlashOpacity, high: markFlashOpacity, pulses: false, cycle: pulseCycle)
        } else if reduceMotion {
            logo = Logo(low: reducedMotionOpacity, high: reducedMotionOpacity, pulses: false, cycle: pulseCycle)
        } else {
            logo = Logo(low: pulseLow, high: pulseHigh, pulses: true, cycle: pulseCycle)
        }
        return RecordingPresentation(showsStandardUI: false, logo: logo, showsGlanceTime: glancing,
                                     glanceOpacity: glancing ? glanceTextOpacity : 0, tapAnywhereMarks: true)
    }
}

/// What the watch buzzes for. Discreet uses light taps only.
enum HapticCue: Equatable {
    case start, mark, warning, stop, interrupted, failure, pauseResume

    func pattern(discreet: Bool) -> [WKHapticType] {
        if discreet {
            switch self {
            case .warning, .interrupted: return [.click, .click]
            case .failure: return [.click, .click, .click]
            default: return [.click]
            }
        }
        switch self {
        case .start: return [.start]
        case .mark: return [.success]
        case .warning: return [.notification]
        case .stop: return [.stop]
        case .interrupted: return [.retry]
        case .failure: return [.failure]
        case .pauseResume: return [.click]
        }
    }
}

enum WatchHaptics {
    /// Taps in a pattern are this far apart
    static let spacing: Duration = .milliseconds(350)

    @MainActor
    static func play(_ cue: HapticCue, discreet: Bool) {
        let pattern = cue.pattern(discreet: discreet)
        guard let first = pattern.first else { return }
        WKInterfaceDevice.current().play(first)
        guard pattern.count > 1 else { return }
        Task { @MainActor in
            for type in pattern.dropFirst() {
                try? await Task.sleep(for: spacing)
                WKInterfaceDevice.current().play(type)
            }
        }
    }
}

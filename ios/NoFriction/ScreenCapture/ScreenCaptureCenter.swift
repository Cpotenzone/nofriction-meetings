import Foundation
import Observation
import ReplayKit
import SwiftData
import SwiftUI
import UIKit

/// The app's side of screen capture (docs/SCREEN_CAPTURE_IOS.md).
///
/// - Watches the App Group container for a running broadcast ("Capturing
///   your screen", the screen count) and attaches it to the recording in
///   progress. A broadcast started from Control Center with no recording
///   running starts one (remembered type and notebook, no time limit) once
///   noFriction is on screen; one that ended before that gets a recording of
///   its own.
/// - Tells the extension when the recording pauses, and ends the broadcast
///   when the recording stops; then imports the screens and app audio.
/// - Transcribes app audio on the device when this iPhone isn't recording.
@MainActor
@Observable
final class ScreenCaptureCenter {
    /// The broadcast running right now, if any
    private(set) var live: ScreenCaptureManifest?
    /// Some video was hidden from capture: say so once ("Some apps hide…")
    private(set) var hiddenNoticeDue = false

    var isCapturing: Bool { live != nil }
    var screensKept: Int { live?.framesKept ?? 0 }

    static let hiddenNotice = "Some apps hide their video from screen capture; those parts are skipped."

    @ObservationIgnored let importer: ScreenCaptureImporter
    @ObservationIgnored let links: ScreenCaptureLinks
    @ObservationIgnored private weak var session: RecordingSession?
    @ObservationIgnored private let isPro: @MainActor () -> Bool
    @ObservationIgnored private let started = DarwinSignal()
    @ObservationIgnored private let finished = DarwinSignal()
    @ObservationIgnored private var poller: Task<Void, Never>?
    @ObservationIgnored private var transcriber: Task<Void, Never>?
    @ObservationIgnored private var busy = false
    @ObservationIgnored private var autoStarting = false
    /// Folders seen without a manifest once; removed if still without one next time
    @ObservationIgnored private var unnamed: Set<String> = []
    /// Recordings whose app audio failed to transcribe since the app last came on screen
    @ObservationIgnored private var attempted: Set<UUID> = []
    @ObservationIgnored private weak var picker: RPSystemBroadcastPickerView?
    @ObservationIgnored var shared: UserDefaults? = ScreenCaptureContract.defaults()

    init(importer: ScreenCaptureImporter, session: RecordingSession?, links: ScreenCaptureLinks = .shared,
         isPro: @escaping @MainActor () -> Bool) {
        self.importer = importer
        self.session = session
        self.links = links
        self.isPro = isPro
    }

    /// At launch: listen for the extension, clean up, and start polling.
    func activate() {
        started.observe(ScreenCaptureContract.Signal.started) { Task { @MainActor [weak self] in await self?.refresh() } }
        finished.observe(ScreenCaptureContract.Signal.finished) { Task { @MainActor [weak self] in await self?.refresh() } }
        importer.removeLeftovers(links: links)
        publishAppAudio()
        setPaused(false)
        poller?.cancel()
        poller = Task { [weak self] in
            while !Task.isCancelled {
                await self?.refresh()
                try? await Task.sleep(for: .seconds(1))
            }
        }
    }

    /// The app came on screen.
    func sceneBecameActive() {
        attempted = []
        Task { await refresh() }
    }

    // MARK: The extension's flags

    /// "Transcribe what's playing": Pro and the switch on (read by the extension when a capture starts).
    func publishAppAudio() {
        ScreenAudioPolicy.publish(isPro: isPro(), wantsIt: ScreenCapturePrefs.wantsAppAudio(), to: shared)
    }

    private func setPaused(_ paused: Bool) {
        shared?.set(paused, forKey: ScreenCaptureContract.Key.paused)
        DarwinSignal.post(ScreenCaptureContract.Signal.control)
    }

    // MARK: Recording hooks (RecordingSession)

    func recordingStarted(_ meeting: Meeting) {
        setPaused(false)
        // A capture already running (started from Control Center) joins this recording
        for s in importer.sessions() {
            guard let m = s.manifest, m.isLive(at: importer.env.now()), links.meetingID(for: m.id) == nil else { continue }
            links.link(m.id, to: meeting.id)
        }
    }

    func recordingPaused(_ paused: Bool) { setPaused(paused) }

    /// The recording is stopping: end its broadcast (the extension finishes
    /// its files first, up to a few seconds), then import them.
    func recordingWillStop(_ meeting: Meeting) async {
        setPaused(false)
        let mine = importer.sessions().filter { $0.manifest.map { links.meetingID(for: $0.id) == meeting.id } ?? false }
        if let running = mine.first(where: { $0.manifest?.isLive(at: importer.env.now()) ?? false }), let id = running.manifest?.id {
            shared?.set(id.uuidString, forKey: ScreenCaptureContract.Key.stopRequest)
            DarwinSignal.post(ScreenCaptureContract.Signal.control)
            for _ in 0..<15 {
                if ScreenCaptureManifest.load(from: running.folder)?.endedAt != nil { break }
                try? await Task.sleep(for: .milliseconds(200))
            }
        }
        for s in mine {
            let fresh = ScreenCaptureImporter.Session(folder: s.folder, manifest: ScreenCaptureManifest.load(from: s.folder))
            importer.importFiles(of: fresh, into: meeting)
            if let m = fresh.manifest, !m.isLive(at: importer.env.now()) {
                noteHidden(m)
                importer.remove(fresh)
                links.unlink(m.id)
            }
        }
        live = nil
    }

    // MARK: Reconcile

    /// Read the container: what's running, and what ended and must be imported.
    func refresh() async {
        guard !busy else { return }
        busy = true
        defer { busy = false }
        let now = importer.env.now()
        var running: ScreenCaptureManifest?
        for s in importer.sessions() {
            guard let manifest = s.manifest else {
                if unnamed.contains(s.key) {
                    importer.remove(s)
                    unnamed.remove(s.key)
                } else {
                    unnamed.insert(s.key)
                }
                continue
            }
            unnamed.remove(s.key)
            if manifest.isLive(at: now) {
                running = manifest
                if links.meetingID(for: manifest.id) == nil { attach(manifest) }
            } else {
                finish(s, manifest: manifest)
            }
        }
        live = running
        if let running { noteHidden(running) }
        transcribeIfIdle()
    }

    /// A running broadcast with no recording: the recording in progress, or a new one.
    private func attach(_ manifest: ScreenCaptureManifest) {
        guard let session else { return }
        if session.isActive, let meeting = session.meeting {
            links.link(manifest.id, to: meeting.id)
            return
        }
        // Start one only on screen (the microphone can't start in the
        // background) and once the recording notice was accepted
        guard session.phase == .idle, !autoStarting, importer.env.isForeground(),
              UserDefaults.standard.bool(forKey: "recordingNoticeAccepted") else { return }
        autoStarting = true
        Task {
            await session.start(limit: .noLimit, kind: RecordingKindStore.remembered(),
                                notebook: ScreenCapturePrefs.lastNotebook(), captureScreen: true)
            if let meeting = session.meeting { links.link(manifest.id, to: meeting.id) }
            autoStarting = false
        }
    }

    /// An ended broadcast: everything into its recording, then the folder goes.
    private func finish(_ s: ScreenCaptureImporter.Session, manifest: ScreenCaptureManifest) {
        defer {
            importer.remove(s)
            links.unlink(manifest.id)
        }
        noteHidden(manifest)
        let target: Meeting
        if let id = links.meetingID(for: manifest.id) {
            // Its recording was deleted: nothing comes back
            guard let meeting = importer.meeting(id: id) else { return }
            target = meeting
        } else {
            guard importer.hasContent(s) else { return }
            target = importer.makeRecording(for: manifest) { from, to in
                CalendarService.shared.isAuthorized ? CalendarService.shared.events(from: from, to: to) : []
            }
        }
        // Screens after the recording stopped (a few seconds' grace) aren't part of it
        importer.importFiles(of: s, into: target, keepUntil: target.endedAt?.addingTimeInterval(10))
    }

    private func noteHidden(_ manifest: ScreenCaptureManifest) {
        guard manifest.hiddenSeconds >= HiddenVideoTracker.noticeAfter,
              !UserDefaults.standard.bool(forKey: ScreenCapturePrefs.hiddenNoticeKey) else { return }
        hiddenNoticeDue = true
    }

    func dismissHiddenNotice() {
        UserDefaults.standard.set(true, forKey: ScreenCapturePrefs.hiddenNoticeKey)
        hiddenNoticeDue = false
    }

    // MARK: Transcription

    /// App audio waiting: transcribe it, one recording at a time, never
    /// while this iPhone records (one recognizer at a time).
    func transcribeIfIdle() {
        guard transcriber == nil, session?.isActive != true else { return }
        guard let next = importer.meetingsWithPendingAudio().first(where: { !attempted.contains($0.id) }) else { return }
        let id = next.id
        transcriber = Task { [weak self] in
            guard let self else { return }
            let done = await self.importer.transcribePending(meetingID: id) { [weak self] in self?.session?.isActive != true }
            if !done { self.attempted.insert(id) }
            self.transcriber = nil
        }
    }

    // MARK: Delete Recording

    func purge(_ meeting: Meeting) {
        importer.purge(meeting: meeting, links: links)
    }

    // MARK: Apple's broadcast sheet

    func register(_ view: RPSystemBroadcastPickerView) { picker = view }

    /// Open Apple's screen-broadcast sheet with noFriction preselected
    /// (Start or Stop there is one tap).
    func presentPicker() {
        guard let picker, let button = Self.button(in: picker) else { return }
        button.sendActions(for: .touchUpInside)
    }

    private static func button(in view: UIView) -> UIButton? {
        if let b = view as? UIButton { return b }
        for sub in view.subviews { if let b = button(in: sub) { return b } }
        return nil
    }
}

/// Apple's broadcast picker, hosting noFriction's extension (mic button
/// hidden). Kept tiny and invisible; `ScreenCaptureCenter.presentPicker`
/// presses it.
struct BroadcastPickerHost: UIViewRepresentable {
    let center: ScreenCaptureCenter

    func makeUIView(context: Context) -> RPSystemBroadcastPickerView {
        let view = RPSystemBroadcastPickerView(frame: CGRect(x: 0, y: 0, width: 44, height: 44))
        view.preferredExtension = ScreenCaptureContract.extensionBundleID
        view.showsMicrophoneButton = false
        center.register(view)
        return view
    }

    func updateUIView(_ view: RPSystemBroadcastPickerView, context: Context) {
        center.register(view)
    }
}

import SwiftUI
import VisionKit

/// Settings → Sync (docs/SYNC.md): a row in Settings, and the screen behind it.
struct SyncSection: View {
    @Environment(Store.self) private var store
    @Environment(SyncCenter.self) private var sync
    @State private var paywallFor: ProFeature?

    var body: some View {
        Section {
            if store.isPro {
                NavigationLink {
                    SyncView()
                } label: {
                    LabeledContent("Sync with your Mac", value: sync.isPaired ? summary : "Off")
                }
                .accessibilityIdentifier("sync-row")
            } else {
                Button {
                    paywallFor = .sync
                } label: {
                    LabeledContent("Sync with your Mac", value: "Pro")
                }
                .accessibilityIdentifier("sync-row")
            }
        } header: {
            Text("Sync")
        } footer: {
            Text("Keep the same recordings, transcripts, notes, marks and links on this iPhone and your Mac. They go directly over your Wi-Fi, encrypted, never through a server.")
        }
        .sheet(item: $paywallFor) { PaywallView(feature: $0) }
    }

    private var summary: String {
        if let at = sync.lastSyncedAt { return at.formatted(.relative(presentation: .named)) }
        return "Paired"
    }
}

struct SyncView: View {
    @Environment(SyncCenter.self) private var sync
    @Environment(Store.self) private var store
    @State private var scanning = false
    @State private var error: String?
    @State private var forgetting: SyncMacState?
    @State private var paywallFor: ProFeature?

    var body: some View {
        Form {
            Section {
                ForEach(sync.macs, id: \.macID) { mac in
                    VStack(alignment: .leading, spacing: 4) {
                        Text(mac.name).font(.body.weight(.medium))
                        Text(line(for: mac)).font(.footnote).foregroundStyle(.secondary)
                    }
                    .swipeActions {
                        Button("Forget", role: .destructive) { forgetting = mac }
                    }
                    .contextMenu {
                        Button("Forget", systemImage: "trash", role: .destructive) { forgetting = mac }
                    }
                }
                if sync.isPaired {
                    Button {
                        Task { await sync.syncNow() }
                    } label: {
                        HStack {
                            Text("Sync now")
                            Spacer()
                            if sync.isSyncing { ProgressView() }
                        }
                    }
                    .disabled(sync.isSyncing)
                    .accessibilityIdentifier("sync-now")
                }
            } header: {
                Text(sync.isPaired ? "Paired Mac" : "Not paired")
            } footer: {
                Text(statusText)
            }

            Section {
                Button("Pair with your Mac", systemImage: "qrcode.viewfinder") { start { scanning = true } }
                    .accessibilityIdentifier("sync-pair")
                Button("Paste pairing link", systemImage: "doc.on.clipboard") {
                    start { pasteLink() }
                }
                .accessibilityIdentifier("sync-paste")
            } footer: {
                Text("On your Mac, open noFriction → Settings → Sync → Pair a device, then scan the code. Or copy the pairing link there and paste it here.")
            }

            Section("How it works") {
                Text("Your iPhone and Mac talk directly on the same Wi-Fi, encrypted and pinned to the Mac you paired. noFriction receives nothing. iOS doesn't let apps sync in the background, so this iPhone syncs when you open noFriction, when a recording stops, and when you tap Sync now. Keep noFriction open on your Mac.")
                Text("Recordings, transcripts, notes, marks, links and topics sync. Delete and Strike from the record apply on both devices. Photos, screens, audio, chats and review guides stay on the device that made them.")
            }
            .font(.footnote)
            .foregroundStyle(.secondary)
        }
        .scrollContentBackground(.hidden)
        .background(Theme.background)
        .navigationTitle("Sync")
        .sheet(isPresented: $scanning) {
            PairingScanner { code in
                scanning = false
                Task { await pair(code) }
            }
        }
        .sheet(item: $paywallFor) { PaywallView(feature: $0) }
        .alert("Couldn't sync", isPresented: .init(get: { error != nil }, set: { if !$0 { error = nil } })) {
            Button("OK", role: .cancel) {}
        } message: {
            Text(error ?? "")
        }
        .confirmationDialog("Forget \(forgetting?.name ?? "this Mac")?", isPresented: .init(get: { forgetting != nil }, set: { if !$0 { forgetting = nil } }), titleVisibility: .visible) {
            Button("Forget", role: .destructive) {
                if let m = forgetting { sync.forget(m.macID) }
                forgetting = nil
            }
        } message: {
            Text("This iPhone stops syncing with it. Recordings already here stay. To sync again, pair again.")
        }
    }

    private func start(_ action: () -> Void) {
        guard store.isPro else { paywallFor = .sync; return }
        action()
    }

    private var statusText: String {
        switch sync.status {
        case .syncing(let name): return "Syncing with \(name)…"
        case .failed(let message): return message
        case .idle:
            if let at = sync.lastSyncedAt { return "Last synced \(at.formatted(.relative(presentation: .named)))." }
            return sync.isPaired ? "Not synced yet." : "Pair with your Mac to keep both in step."
        }
    }

    private func line(for mac: SyncMacState) -> String {
        if let e = mac.lastError { return e }
        if let at = mac.lastSyncedAt { return "Last synced \(at.formatted(.relative(presentation: .named)))" }
        return "Not synced yet"
    }

    private func pasteLink() {
        guard let text = UIPasteboard.general.string, !text.isEmpty else {
            error = "Copy the pairing link on your Mac first (Settings → Sync → Pair a device → Copy pairing link)."
            return
        }
        Task { await pair(text) }
    }

    private func pair(_ code: String) async {
        do { try await sync.pair(code) } catch {
            self.error = (error as? LocalizedError)?.errorDescription ?? error.localizedDescription
        }
    }
}

/// Scans the Mac's pairing QR code (VisionKit). Falls back to a message
/// where the camera scanner isn't available (Simulator, older devices).
struct PairingScanner: View {
    let onCode: (String) -> Void
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            Group {
                if DataScannerViewController.isSupported && DataScannerViewController.isAvailable {
                    QRScannerRepresentable(onCode: onCode).ignoresSafeArea()
                } else {
                    ContentUnavailableView("Can't scan here", systemImage: "camera",
                                           description: Text("Use Paste pairing link instead: copy the link on your Mac and paste it on this iPhone."))
                }
            }
            .navigationTitle("Scan the code on your Mac")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } } }
        }
    }
}

private struct QRScannerRepresentable: UIViewControllerRepresentable {
    let onCode: (String) -> Void

    func makeUIViewController(context: Context) -> DataScannerViewController {
        let vc = DataScannerViewController(recognizedDataTypes: [.barcode(symbologies: [.qr])], qualityLevel: .balanced,
                                           isHighlightingEnabled: true)
        vc.delegate = context.coordinator
        try? vc.startScanning()
        return vc
    }

    func updateUIViewController(_ vc: DataScannerViewController, context: Context) {}

    func makeCoordinator() -> Coordinator { Coordinator(onCode: onCode) }

    final class Coordinator: NSObject, DataScannerViewControllerDelegate {
        let onCode: (String) -> Void
        private var done = false
        init(onCode: @escaping (String) -> Void) { self.onCode = onCode }

        func dataScanner(_ scanner: DataScannerViewController, didAdd items: [RecognizedItem], allItems: [RecognizedItem]) {
            for item in items {
                if case .barcode(let b) = item, let s = b.payloadStringValue, s.hasPrefix("nfsync:"), !done {
                    done = true
                    scanner.stopScanning()
                    onCode(s)
                }
            }
        }
    }
}

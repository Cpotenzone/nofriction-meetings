import XCTest
@testable import noFriction

// docs/PRO.md, iOS side: the paywall names the feature that opened it, every
// feature belongs to a listed group, and the Free line keeps what must stay
// free. Keys match the Mac (src/lib/pro.ts).

final class ProFeatureTests: XCTestCase {
    func testPaywallTitleNamesTheFeature() {
        XCTAssertEqual(ProCopy.headline(.sync), "Sync is part of noFriction Pro")
        XCTAssertEqual(ProCopy.headline(.transcribePlaying), "Transcribe what's playing is part of noFriction Pro")
        XCTAssertEqual(ProCopy.headline(.notes), "Notes are part of noFriction Pro")
        XCTAssertEqual(ProCopy.headline(nil), "noFriction Pro")
        for f in ProFeature.allCases {
            XCTAssertTrue(f.headline.hasSuffix("part of noFriction Pro"), f.rawValue)
        }
    }

    func testEveryFeatureIsListed() {
        let listed = Set(ProGroup.allCases)
        for f in ProFeature.allCases { XCTAssertTrue(listed.contains(f.group), f.rawValue) }
        XCTAssertEqual(ProFeature.sync.group, .sync)
        XCTAssertEqual(ProFeature.transcribePlaying.group, .playing)
        XCTAssertEqual(ProFeature.followUp.group, .study)
    }

    func testFreeLineKeepsTheEssentials() {
        for w in ["recording", "microphone transcription", "search", "Delete and Strike", "Apple Watch"] {
            XCTAssertTrue(ProCopy.free.contains(w), w)
        }
        for w in ["Sync", "Obsidian", "Chat", "what's playing"] {
            XCTAssertFalse(ProCopy.free.contains(w), w)
        }
    }

    func testCopyIsSentenceCaseWithoutEmoji() {
        let all = [ProCopy.value, ProCopy.free] + ProGroup.allCases.flatMap { [$0.title, $0.detail] }
        for s in all {
            XCTAssertFalse(s.unicodeScalars.contains { $0.properties.isEmojiPresentation }, s)
            XCTAssertNotEqual(s, s.uppercased(), s)
        }
        let proper: Set<String> = ["Obsidian", "Mac", "iPhone"]
        for g in ProGroup.allCases {
            for w in g.title.split(separator: " ").dropFirst() where !proper.contains(String(w)) {
                XCTAssertEqual(String(w), w.lowercased(), g.title)
            }
        }
    }

    /// The same keys as the Mac paywall (src/lib/pro.ts → PRO_FEATURE_KEYS).
    func testKeysMatchTheMac() throws {
        let repo = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        let ts = try String(contentsOf: repo.appending(path: "src/lib/pro.ts"), encoding: .utf8)
        for f in ProFeature.allCases {
            XCTAssertTrue(ts.contains("\"\(f.rawValue)\""), "\(f.rawValue) missing from src/lib/pro.ts")
            XCTAssertTrue(ts.contains(f.headline), "headline differs from the Mac: \(f.headline)")
        }
    }
}

import XCTest
@testable import noFriction

// MARK: - Transcript filter

final class TranscriptFilterTests: XCTestCase {
    private func isDropped(_ text: String, nearSilence: Bool = false, previous: String? = nil) -> Bool {
        if case .drop = TranscriptFilter.clean(text, nearSilence: nearSilence, previous: previous) { return true }
        return false
    }

    private func kept(_ text: String, nearSilence: Bool = false, words: [WordTiming] = []) -> (text: String, words: [WordTiming], substantive: Bool)? {
        if case .keep(let t, let w, let s) = TranscriptFilter.clean(text, words: words, nearSilence: nearSilence) { return (t, w, s) }
        return nil
    }

    // Real junk from a Mac recording left running after the meeting
    func testDropsRealByeByeLoops() {
        XCTAssertTrue(isDropped("Bye-bye. Bye-bye."))
        XCTAssertTrue(isDropped("Bye-bye. Bye-bye. Bye-bye. Bye-bye. Bye-bye. Bye-bye. Bye-by"))
        XCTAssertTrue(isDropped("bye bye bye bye"))
        XCTAssertTrue(isDropped("Bye. Bye. Bye."))
    }

    func testSingleByeByeDroppedOnlyOnNearSilence() {
        XCTAssertTrue(isDropped("Bye-bye.", nearSilence: true))
        let k = kept("Bye-bye.")
        XCTAssertEqual(k?.text, "Bye-bye.")
        XCTAssertEqual(k?.substantive, false, "kept, but not evidence the meeting is still on")
    }

    func testDuplicateFillerSegmentDropped() {
        XCTAssertTrue(isDropped("Bye-bye.", previous: "Bye-bye."))
        XCTAssertFalse(isDropped("Bye-bye.", previous: "See you Thursday."))
    }

    func testDropsFillerPhraseLoopsAndFillerOnSilence() {
        XCTAssertTrue(isDropped("Thank you. Thank you. Thank you."))
        XCTAssertTrue(isDropped("you you you you you"))
        XCTAssertTrue(isDropped("Thank you.", nearSilence: true))
        XCTAssertTrue(isDropped("Okay.", nearSilence: true))
        XCTAssertTrue(isDropped("   "))
    }

    func testKeepsRealSentences() {
        XCTAssertEqual(kept("Bye for now, talk Thursday.")?.text, "Bye for now, talk Thursday.")
        XCTAssertEqual(kept("Bye for now, talk Thursday.", nearSilence: true)?.text, "Bye for now, talk Thursday.")
        XCTAssertEqual(kept("No no no, that's wrong.")?.text, "No no no, that's wrong.")
        XCTAssertEqual(kept("No no no, that's wrong.")?.substantive, true)
        XCTAssertEqual(kept("Thanks, bye everyone!")?.text, "Thanks, bye everyone!")
        XCTAssertEqual(kept("Okay.")?.text, "Okay.")
    }

    func testGenuineRepeatsAndClosingsSurviveClearAudio() {
        XCTAssertEqual(kept("Yeah, yeah, yeah.")?.text, "Yeah, yeah, yeah.")
        XCTAssertEqual(kept("Hello? Hello? Hello? Hi")?.text, "Hello? Hello? Hello? Hi")
        XCTAssertEqual(kept("See you next time.")?.text, "See you next time.")
        // Still filler: dropped on near silence or as a duplicate
        XCTAssertTrue(isDropped("Yeah, yeah, yeah.", nearSilence: true))
        XCTAssertTrue(isDropped("See you next time.", nearSilence: true))
        XCTAssertTrue(isDropped("See you next time.", previous: "See you next time."))
        // A 4+ loop is still a loop
        XCTAssertTrue(isDropped("yeah yeah yeah yeah yeah"))
        // Real bye-bye junk unaffected
        XCTAssertTrue(isDropped("Bye. Bye. Bye."))
    }

    func testCollapsesLoopInsideRealSegment() {
        XCTAssertEqual(kept("So the plan is bye bye bye bye bye")?.text, "So the plan is bye")
        XCTAssertEqual(kept("We're done here. Bye-bye. Bye-bye. Bye-bye.")?.text, "We're done here. Bye-bye.")
        XCTAssertEqual(kept("Let's ship it. Thank you thank you thank you thank")?.text, "Let's ship it. Thank you")
    }

    func testCollapseKeepsTrailingPunctuation() {
        XCTAssertEqual(kept("no no no no, that's wrong")?.text, "no, that's wrong")
        XCTAssertEqual(kept("No no no no no. Next item")?.text, "No. Next item")
        XCTAssertEqual(kept("Wait wait wait wait? Okay")?.text, "Wait? Okay")
        // The kept word already has its own punctuation: nothing doubled
        XCTAssertEqual(kept("We're all done here. Bye. Bye. Bye. Bye.")?.text, "We're all done here. Bye.")
        // Word timings after the insertion shift by the carried punctuation
        let text = "no no no no, fine"
        let words = [
            WordTiming(location: 0, length: 2, start: 0, end: 0.2),
            WordTiming(location: 3, length: 2, start: 0.3, end: 0.5),
            WordTiming(location: 6, length: 2, start: 0.6, end: 0.8),
            WordTiming(location: 9, length: 2, start: 0.9, end: 1.1),
            WordTiming(location: 13, length: 4, start: 1.3, end: 1.6),
        ]
        let k = kept(text, words: words)
        XCTAssertEqual(k?.text, "no, fine")
        XCTAssertEqual(k?.words, [words[0], WordTiming(location: 4, length: 4, start: 1.3, end: 1.6)])
    }

    func testCollapseKeepsWordTimingsAligned() {
        // "Ship it bye bye bye bye" — word offsets in UTF-16
        let text = "Ship it bye bye bye bye"
        let words = [
            WordTiming(location: 0, length: 4, start: 0, end: 0.4),
            WordTiming(location: 5, length: 2, start: 0.4, end: 0.6),
            WordTiming(location: 8, length: 3, start: 1, end: 1.2),
            WordTiming(location: 12, length: 3, start: 1.3, end: 1.5),
            WordTiming(location: 16, length: 3, start: 1.6, end: 1.8),
            WordTiming(location: 20, length: 3, start: 1.9, end: 2.1),
        ]
        let k = kept(text, words: words)
        XCTAssertEqual(k?.text, "Ship it bye")
        XCTAssertEqual(k?.words, Array(words.prefix(3)))
    }

    func testTokensSplitHyphensAndFixTruncatedTail() {
        XCTAssertEqual(TranscriptFilter.tokens("Bye-bye. Bye-by").map(\.norm), ["bye", "bye", "bye", "bye"])
        XCTAssertEqual(TranscriptFilter.tokens("Talk to you by").map(\.norm), ["talk", "to", "you", "by"])
        XCTAssertEqual(TranscriptFilter.tokens("It\u{2019}s fine.").map(\.norm), ["it's", "fine"])
    }

    func testLoopsNeedFourSingleWordRepeats() {
        XCTAssertTrue(TranscriptFilter.loops(["no", "no", "no", "that's", "wrong"]).isEmpty)
        XCTAssertEqual(TranscriptFilter.loops(["a", "b", "b", "b", "b"]), [.init(start: 1, unit: 1, end: 5)])
    }
}

// MARK: - Meeting-end detection

final class MeetingEndDetectorTests: XCTestCase {
    private final class TestClock {
        var now = Date(timeIntervalSince1970: 1_790_000_000)
        func advance(_ seconds: TimeInterval) { now = now.addingTimeInterval(seconds) }
    }

    private var clock = TestClock()

    override func setUp() {
        clock = TestClock()
    }

    private func detector(minutes: Double = 3, scheduledEnd: Date? = nil, enabled: Bool = true) -> MeetingEndDetector {
        var config = MeetingEndDetector.Config()
        config.silenceMinutes = minutes
        config.enabled = enabled
        let clock = self.clock
        return MeetingEndDetector(config: config, scheduledEnd: scheduledEnd, clock: { clock.now })
    }

    /// Advance second by second, feeding `level` each second, until an action other than .none
    private func run(_ d: inout MeetingEndDetector, for seconds: Int, level: Float = 0, speechEvery: Int? = nil) -> (MeetingEndDetector.Action, Int)? {
        for s in 1...seconds {
            clock.advance(1)
            d.noteLevel(level)
            if let every = speechEvery, s % every == 0 { _ = d.noteSpeech() }
            let action = d.tick()
            if action != .none { return (action, s) }
        }
        return nil
    }

    func testSilenceAndQuietAudioTriggersAfterWindow() {
        var d = detector(minutes: 3)
        let hit = run(&d, for: 400)
        XCTAssertEqual(hit?.1, 180)
        XCTAssertEqual(hit?.0, .beginCountdown(.silence, deadline: clock.now.addingTimeInterval(30)))
    }

    func testNoSpeechButLoudAudioWaitsTwiceAsLong() {
        var d = detector(minutes: 3)
        let hit = run(&d, for: 600, level: 0.6)
        XCTAssertEqual(hit?.1, 360)
        if case .beginCountdown(.silence, _)? = hit?.0 {} else { XCTFail("expected silence countdown, got \(String(describing: hit))") }
    }

    func testRegularSpeechNeverTriggers() {
        var d = detector(minutes: 3)
        XCTAssertNil(run(&d, for: 1800, level: 0.6, speechEvery: 45))
    }

    func testQuietAudioAloneDoesNotTriggerWhileSpeechIsTranscribed() {
        var d = detector(minutes: 3)
        // Soft far-field voice: meter under threshold, transcript still flowing
        XCTAssertNil(run(&d, for: 1200, level: 0.05, speechEvery: 60))
    }

    func testCalendarEndPlusGraceAndSixtySecondsQuiet() {
        // Event ends 5 min in; people keep talking until 8 min, then stop
        var d = detector(minutes: 10, scheduledEnd: clock.now.addingTimeInterval(300))
        XCTAssertNil(run(&d, for: 480, level: 0.6, speechEvery: 20))
        let hit = run(&d, for: 200, level: 0.6)
        XCTAssertEqual(hit?.1, 60)
        if case .beginCountdown(.calendarEnded, _)? = hit?.0 {} else { XCTFail("expected calendar countdown, got \(String(describing: hit))") }
    }

    func testCalendarSignalWaitsForGracePeriod() {
        // Event ends at 1 min; quiet from the start — grace keeps it until 3 min
        var d = detector(minutes: 30, scheduledEnd: clock.now.addingTimeInterval(60))
        let hit = run(&d, for: 400)
        XCTAssertEqual(hit?.1, 180)
        if case .beginCountdown(.calendarEnded, _)? = hit?.0 {} else { XCTFail() }
    }

    func testCountdownStopsWhenNobodyResponds() {
        var d = detector(minutes: 1)
        XCTAssertEqual(run(&d, for: 100)?.1, 60)
        XCTAssertTrue(d.isCountingDown)
        let hit = run(&d, for: 60)
        XCTAssertEqual(hit?.0, .stop(.silence))
        XCTAssertEqual(hit?.1, 30)
        XCTAssertEqual(d.phase, .ended(.silence))
        XCTAssertNil(run(&d, for: 100), "nothing after the stop")
    }

    func testSpeechCancelsCountdown() {
        var d = detector(minutes: 1)
        _ = run(&d, for: 100)
        clock.advance(10)
        XCTAssertEqual(d.noteSpeech(), .cancelCountdown)
        XCTAssertEqual(d.phase, .listening)
        // Silence clock restarted from the speech
        XCTAssertEqual(run(&d, for: 100)?.1, 60)
    }

    func testKeepRecordingSnoozesTenMinutes() {
        var d = detector(minutes: 1)
        _ = run(&d, for: 100)
        XCTAssertEqual(d.keepRecording(), .cancelCountdown)
        let hit = run(&d, for: 1000)
        XCTAssertEqual(hit?.1, 600, "re-prompts only after the snooze, if still silent")
        if case .beginCountdown? = hit?.0 {} else { XCTFail() }
    }

    func testStopNowStopsImmediately() {
        var d = detector(minutes: 1)
        _ = run(&d, for: 100)
        XCTAssertEqual(d.stopNow(), .stop(.silence))
        XCTAssertTrue(d.endDetected)
    }

    func testDisabledNeverTriggersAndCancelsRunningCountdown() {
        var off = detector(minutes: 1, enabled: false)
        XCTAssertNil(run(&off, for: 1000))

        var d = detector(minutes: 1)
        _ = run(&d, for: 100)
        d.config.enabled = false
        clock.advance(1)
        XCTAssertEqual(d.tick(), .cancelCountdown)
    }

    func testResumeAfterPauseRestartsSilenceClock() {
        var d = detector(minutes: 1)
        clock.advance(50)
        _ = d.resetIdle()
        XCTAssertEqual(run(&d, for: 100)?.1, 60)
    }

    func testConfigLoadsFromDefaults() throws {
        let defaults = try XCTUnwrap(UserDefaults(suiteName: "MeetingEndDetectorTests"))
        defaults.removePersistentDomain(forName: "MeetingEndDetectorTests")
        XCTAssertEqual(MeetingEndDetector.Config.load(defaults).enabled, true, "default on")
        XCTAssertEqual(MeetingEndDetector.Config.load(defaults).silenceMinutes, 3)
        defaults.set(false, forKey: MeetingEndDetector.Config.enabledKey)
        defaults.set(7, forKey: MeetingEndDetector.Config.minutesKey)
        XCTAssertEqual(MeetingEndDetector.Config.load(defaults).enabled, false)
        // The silence time is a decision, not a setting: a stored value from an older build is ignored
        XCTAssertEqual(MeetingEndDetector.Config.load(defaults).silenceMinutes, 3)
        defaults.removePersistentDomain(forName: "MeetingEndDetectorTests")
    }

    func testLevelHistoryPeak() {
        var h = LevelHistory()
        let t = clock.now
        h.append(0.1, at: t)
        h.append(0.5, at: t.addingTimeInterval(2))
        h.append(0.2, at: t.addingTimeInterval(4))
        XCTAssertEqual(h.peak(from: t, to: t.addingTimeInterval(1)), 0.1)
        XCTAssertEqual(h.peak(from: t, to: t.addingTimeInterval(5)), 0.5)
        XCTAssertNil(h.peak(from: t.addingTimeInterval(10), to: t.addingTimeInterval(20)))
    }
}

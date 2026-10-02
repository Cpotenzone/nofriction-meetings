import AVFoundation
import Foundation

/// Overwrites time ranges of a recording with digital silence, in place
/// (docs/REDACTION.md, purge item 3). The AAC file is decoded to PCM in
/// chunks, the samples in range are set to zero, and the result is
/// re-encoded with the same format to a temp file that atomically replaces
/// the original. The old file is deleted; nothing hides the audio, it is gone.
enum AudioSilencer {
    enum Failure: LocalizedError {
        case unreadable(String)
        case writeFailed(String)
        case lengthChanged

        var errorDescription: String? {
            switch self {
            case .unreadable(let why): return "Couldn't read the recording to silence it (\(why))."
            case .writeFailed(let why): return "Couldn't rewrite the recording (\(why))."
            case .lengthChanged: return "The rewritten recording came out a different length, so it was discarded."
            }
        }
    }

    /// Merge, clamp and sort ranges (seconds).
    static func normalized(_ ranges: [ClosedRange<Double>]) -> [ClosedRange<Double>] {
        let sorted = ranges.map { max(0, $0.lowerBound)...max(0, $0.upperBound) }.sorted { $0.lowerBound < $1.lowerBound }
        var out: [ClosedRange<Double>] = []
        for r in sorted {
            if let last = out.last, r.lowerBound <= last.upperBound {
                out[out.count - 1] = last.lowerBound...max(last.upperBound, r.upperBound)
            } else {
                out.append(r)
            }
        }
        return out
    }

    /// Silence `ranges` (seconds) in the file at `url`. Throws, leaving the
    /// original untouched, if anything fails.
    static func silence(_ url: URL, ranges: [ClosedRange<Double>]) throws {
        let ranges = normalized(ranges)
        guard !ranges.isEmpty else { return }

        let input: AVAudioFile
        do { input = try AVAudioFile(forReading: url) } catch { throw Failure.unreadable(error.localizedDescription) }
        let format = input.processingFormat
        let rate = format.sampleRate
        let totalFrames = input.length
        let frameRanges: [Range<AVAudioFramePosition>] = ranges.compactMap {
            let a = AVAudioFramePosition(($0.lowerBound * rate).rounded(.down))
            let b = min(totalFrames, AVAudioFramePosition(($0.upperBound * rate).rounded(.up)))
            return a < b ? a..<b : nil
        }
        guard !frameRanges.isEmpty else { return }

        var settings = input.fileFormat.settings
        settings[AVFormatIDKey] = kAudioFormatMPEG4AAC
        settings[AVSampleRateKey] = rate
        settings[AVNumberOfChannelsKey] = format.channelCount
        if settings[AVEncoderBitRateKey] == nil { settings[AVEncoderBitRateKey] = 64_000 }

        let temp = url.deletingLastPathComponent()
            .appending(path: ".silencing-\(UUID().uuidString).\(url.pathExtension.isEmpty ? "m4a" : url.pathExtension)")
        defer { try? FileManager.default.removeItem(at: temp) }

        do {
            // Scoped so the writer is closed (and the file finalized) before the swap
            do {
                let output = try AVAudioFile(forWriting: temp, settings: settings,
                                             commonFormat: format.commonFormat, interleaved: format.isInterleaved)
                let chunk: AVAudioFrameCount = 32_768
                guard let buffer = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: chunk) else {
                    throw Failure.writeFailed("no buffer")
                }
                while input.framePosition < totalFrames {
                    let position = input.framePosition
                    try input.read(into: buffer, frameCount: chunk)
                    if buffer.frameLength == 0 { break }
                    let chunkRange = position..<(position + AVAudioFramePosition(buffer.frameLength))
                    for r in frameRanges where r.overlaps(chunkRange) {
                        let from = Int(max(r.lowerBound, chunkRange.lowerBound) - position)
                        let to = Int(min(r.upperBound, chunkRange.upperBound) - position)
                        zero(buffer, from: from, to: to)
                    }
                    try output.write(from: buffer)
                }
            }
            let check = try AVAudioFile(forReading: temp)
            // AAC works in 1024-frame packets; allow one packet of slack
            guard abs(check.length - totalFrames) <= 2048 else { throw Failure.lengthChanged }
            _ = try FileManager.default.replaceItemAt(url, withItemAt: temp)
        } catch let f as Failure {
            throw f
        } catch {
            throw Failure.writeFailed(error.localizedDescription)
        }
    }

    private static func zero(_ buffer: AVAudioPCMBuffer, from: Int, to: Int) {
        guard to > from else { return }
        let channels = Int(buffer.format.channelCount)
        let interleaved = buffer.format.isInterleaved
        let stride = interleaved ? channels : 1
        let count = (to - from)
        if let f = buffer.floatChannelData {
            for c in 0..<(interleaved ? 1 : channels) {
                let p = f[c] + from * stride
                p.update(repeating: 0, count: count * stride)
            }
        } else if let i = buffer.int16ChannelData {
            for c in 0..<(interleaved ? 1 : channels) {
                let p = i[c] + from * stride
                p.update(repeating: 0, count: count * stride)
            }
        } else if let i = buffer.int32ChannelData {
            for c in 0..<(interleaved ? 1 : channels) {
                let p = i[c] + from * stride
                p.update(repeating: 0, count: count * stride)
            }
        }
    }
}

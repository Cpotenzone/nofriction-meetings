import Foundation

// Deciding which screens to keep (docs/SCREEN_CAPTURE_IOS.md). Pure value
// code over a coarse brightness grid, so the extension never holds a frame:
// it reads the luma plane once, keeps a few thousand bytes, and lets the
// pixel buffer go. Unit-tested on small synthetic grids.

/// A frame reduced to a grid of cells, each the average brightness (0…255)
/// of its block of pixels.
struct LumaGrid: Equatable, Sendable {
    let columns: Int
    let rows: Int
    var cells: [UInt8]

    /// Cells along the longest side
    static let longSide = 64
    /// Read every `step`-th pixel in each direction inside a cell
    static let step = 4

    init(columns: Int, rows: Int, cells: [UInt8]) {
        precondition(cells.count == columns * rows, "cells must fill the grid")
        self.columns = columns
        self.rows = rows
        self.cells = cells
    }

    /// Grid size for a frame: `longSide` cells along its longest side, the
    /// other side in proportion (at least 1, at most one cell per pixel).
    static func size(width: Int, height: Int, longSide: Int = longSide) -> (columns: Int, rows: Int) {
        guard width > 0, height > 0, longSide > 0 else { return (0, 0) }
        if width >= height {
            let c = min(longSide, width)
            let r = min(height, max(1, Int((Double(height) * Double(c) / Double(width)).rounded())))
            return (c, r)
        }
        let r = min(longSide, height)
        let c = min(width, max(1, Int((Double(width) * Double(r) / Double(height)).rounded())))
        return (c, r)
    }

    /// From an 8-bit luma plane (plane 0 of a 4:2:0 YpCbCr pixel buffer).
    static func fromLuma(_ base: UnsafePointer<UInt8>, width: Int, height: Int, bytesPerRow: Int,
                         longSide: Int = longSide, step: Int = step) -> LumaGrid {
        sample(width: width, height: height, longSide: longSide, step: step) { x, y in
            Int(base[y * bytesPerRow + x])
        }
    }

    /// From 32-bit BGRA pixels (integer BT.601 weights).
    static func fromBGRA(_ base: UnsafePointer<UInt8>, width: Int, height: Int, bytesPerRow: Int,
                         longSide: Int = longSide, step: Int = step) -> LumaGrid {
        sample(width: width, height: height, longSide: longSide, step: step) { x, y in
            let p = y * bytesPerRow + x * 4
            return (29 * Int(base[p]) + 150 * Int(base[p + 1]) + 77 * Int(base[p + 2])) >> 8
        }
    }

    @inline(__always)
    private static func sample(width: Int, height: Int, longSide: Int, step: Int, read: (Int, Int) -> Int) -> LumaGrid {
        let (columns, rows) = size(width: width, height: height, longSide: longSide)
        guard columns > 0, rows > 0 else { return LumaGrid(columns: 0, rows: 0, cells: []) }
        let step = max(1, step)
        var cells = [UInt8](repeating: 0, count: columns * rows)
        for cy in 0..<rows {
            let y0 = cy * height / rows
            let y1 = max(y0 + 1, (cy + 1) * height / rows)
            for cx in 0..<columns {
                let x0 = cx * width / columns
                let x1 = max(x0 + 1, (cx + 1) * width / columns)
                var sum = 0
                var count = 0
                var y = y0
                while y < y1 {
                    var x = x0
                    while x < x1 {
                        sum += read(x, y)
                        count += 1
                        x += step
                    }
                    y += step
                }
                cells[cy * columns + cx] = UInt8(clamping: count > 0 ? sum / count : 0)
            }
        }
        return LumaGrid(columns: columns, rows: rows, cells: cells)
    }

    /// Fraction of cells whose brightness moved by at least `delta` (0…1).
    /// Grids of different sizes (the screen rotated) count as fully changed.
    func changedFraction(from other: LumaGrid, delta: Int) -> Double {
        guard columns == other.columns, rows == other.rows, !cells.isEmpty else { return 1 }
        var changed = 0
        for i in 0..<cells.count where abs(Int(cells[i]) - Int(other.cells[i])) >= delta {
            changed += 1
        }
        return Double(changed) / Double(cells.count)
    }
}

/// Video an app hides from screen capture (DRM) arrives as black frames.
enum HiddenFrame {
    /// Brightness at or under which a cell counts as black. Covers both
    /// full-range (black = 0) and video-range (black = 16) luma.
    static let blackLevel: UInt8 = 24
    /// Share of black cells for the frame to count as hidden. A few lit
    /// cells (subtitles, a progress bar) still count.
    static let blackShare = 0.98

    static func isNearBlack(_ grid: LumaGrid, level: UInt8 = blackLevel, share: Double = blackShare) -> Bool {
        guard !grid.cells.isEmpty else { return false }
        var dark = 0
        for c in grid.cells where c <= level { dark += 1 }
        return Double(dark) / Double(grid.cells.count) >= share
    }
}

/// Keep a screen only when it changed, at most one per second.
struct ScreenChangeDetector: Sendable {
    struct Config: Equatable, Sendable {
        /// Never two screens closer than this (seconds)
        var minInterval: TimeInterval = 1
        /// A small change (a line of text, a counter) waits this long after the last screen
        var calmInterval: TimeInterval = 5
        /// A cell changed when its brightness moved at least this much
        var cellDelta = 12
        /// Under this share of changed cells the screen is the same (a blinking cursor)
        var smallChange = 0.004
        /// From this share on it is a new screen (a slide, a page, a scene cut)
        var bigChange = 0.2
    }

    enum Decision: Equatable, Sendable {
        /// Save this frame
        case keep
        /// Same as the last screen kept
        case unchanged
        /// Changed, but too soon after the last screen; a later frame is kept if it stays changed
        case tooSoon
        /// Near-black: video hidden from capture (or a black screen); never saved
        case hidden
    }

    var config = Config()
    private(set) var lastKept: LumaGrid?
    private(set) var lastKeptAt: TimeInterval?

    init(config: Config = Config()) { self.config = config }

    /// `time`: the frame's timestamp in seconds (any monotonic clock).
    mutating func consider(_ grid: LumaGrid, at time: TimeInterval) -> Decision {
        if HiddenFrame.isNearBlack(grid) { return .hidden }
        guard let last = lastKept, let lastAt = lastKeptAt, time >= lastAt else {
            return keep(grid, at: time)
        }
        let changed = grid.changedFraction(from: last, delta: config.cellDelta)
        if changed < config.smallChange { return .unchanged }
        let elapsed = time - lastAt
        if elapsed < config.minInterval { return .tooSoon }
        if changed < config.bigChange && elapsed < config.calmInterval { return .tooSoon }
        return keep(grid, at: time)
    }

    private mutating func keep(_ grid: LumaGrid, at time: TimeInterval) -> Decision {
        lastKept = grid
        lastKeptAt = time
        return .keep
    }
}

/// How long hidden (near-black) video has run, for the one-time notice.
struct HiddenVideoTracker: Equatable, Sendable {
    /// A gap longer than this between two hidden frames doesn't count as hidden time
    static let maxGap: TimeInterval = 2
    /// Hidden this long in total: tell the user once
    static let noticeAfter: TimeInterval = 3

    private(set) var seconds: TimeInterval = 0
    private var lastHiddenAt: TimeInterval?

    mutating func note(hidden: Bool, at time: TimeInterval) {
        guard hidden else { lastHiddenAt = nil; return }
        if let last = lastHiddenAt, time > last, time - last <= Self.maxGap { seconds += time - last }
        lastHiddenAt = time
    }

    var shouldNotify: Bool { seconds >= Self.noticeAfter }
}

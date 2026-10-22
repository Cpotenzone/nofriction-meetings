import Foundation

/// Export a recording as a package (docs/EXPORT.md): a folder, shared as a
/// .zip, with the recording as Markdown and as a printable web page, the
/// photos and screens the user picked, the transcript, and the review guide
/// and flashcards if they were made. Same layout as the Mac
/// (src-tauri/src/export_package/build.rs).
///
/// Pure: plain data in, text and files out. Nothing here is stored in the
/// app, so Delete and Strike have nothing new to purge; text comes from
/// `RedactionText`, so stricken words export as the marker.
enum ExportPackage {
    struct Picture: Equatable {
        var fileURL: URL
        /// Seconds from the recording's start
        var time: TimeInterval
        var isPhoto: Bool
        var caption: String?
    }

    struct Highlight: Equatable, Identifiable, Sendable {
        var id = UUID()
        var time: TimeInterval
        var label: String
        /// Words copied from the transcript (checked by `ExportAI.parseHighlights`)
        var quote: String?

        static func == (a: Self, b: Self) -> Bool { a.time == b.time && a.label == b.label && a.quote == b.quote }
    }

    struct Mark: Equatable {
        var time: TimeInterval
        var symbol: String
        var label: String
        var note: String?
    }

    struct Link: Equatable {
        var url: String
        var title: String?
        var time: TimeInterval?
    }

    struct Line: Equatable {
        var time: TimeInterval
        var text: String
    }

    struct Email: Equatable {
        var subject: String
        var body: String
    }

    struct Input {
        var title: String
        var kind: RecordingKind = .meeting
        /// "Tuesday 21 October 2026 at 14:00"
        var when: String = ""
        var duration: String?
        var notebook: String?
        var people: [String] = []
        /// The AI notes (Markdown)
        var notes: String?
        var highlights: [Highlight] = []
        var pictures: [Picture] = []
        var marks: [Mark] = []
        var links: [Link] = []
        var email: Email?
        var guideMarkdown: String?
        var flashcardsCSV: String?
        var transcript: [Line] = []
        /// Any part was written by AI: the footer says to check it
        var hasAI = false
    }

    enum FileData: Equatable {
        case text(String)
        case copy(URL)
    }

    static let transcriptFile = "transcript.txt"
    static let flashcardsFile = "flashcards.csv"
    static let screensDir = "screens"

    // MARK: Names

    /// "Biology 101: Cells" → "Biology 101 Cells" (same rule as the Mac).
    static func fileStem(_ title: String) -> String {
        let mapped = String(title.map { $0.isLetter || $0.isNumber || $0 == " " || $0 == "-" || $0 == "_" ? $0 : " " })
        let s = String(mapped.split(separator: " ").joined(separator: " ").prefix(60))
        return s.isEmpty ? "Recording" : s
    }

    /// "2026-10-21 Weekly sync"
    static func folderName(date: Date, title: String) -> String {
        let f = DateFormatter()
        f.calendar = Calendar(identifier: .gregorian)
        f.locale = Locale(identifier: "en_US_POSIX")
        f.dateFormat = "yyyy-MM-dd"
        return "\(f.string(from: date)) \(fileStem(title))"
    }

    /// "screens/03-photo-12-07.jpg": numbered in package order, then the time.
    static func imageName(_ index: Int, _ p: Picture) -> String {
        let ext = p.fileURL.pathExtension.lowercased()
        let safe = ["jpg", "jpeg", "png", "heic"].contains(ext) ? ext : "jpg"
        let kind = p.isPhoto ? "photo" : "screen"
        return String(format: "%@/%02d-%@-%@.%@", screensDir, index + 1, kind, p.time.clock.replacingOccurrences(of: ":", with: "-"), safe)
    }

    static func guideName(_ kind: RecordingKind) -> String { "\(kind.guideTitle).md" }

    static func picturesNoun(_ pictures: [Picture]) -> String {
        let photos = pictures.filter(\.isPhoto).count
        if photos == 0 { return "Screens" }
        if photos == pictures.count { return "Photos" }
        return "Screens and photos"
    }

    private static func footer(_ i: Input) -> String {
        i.hasAI
            ? "Made with noFriction. Parts were written by AI from the transcript and can be wrong; check them against the recording."
            : "Made with noFriction."
    }

    private static func metaLine(_ i: Input) -> String {
        ([i.kind.label] + [i.when].filter { !$0.isEmpty } + [i.duration].compactMap { $0 }).joined(separator: " · ")
    }

    private static func trimmed(_ s: String?) -> String? {
        guard let t = s?.trimmingCharacters(in: .whitespacesAndNewlines), !t.isEmpty else { return nil }
        return t
    }

    // MARK: Markdown

    static func markdown(_ i: Input) -> String {
        let esc = StudyExport.mdEscape
        var out = "# \(esc(i.title))\n\n\(esc(metaLine(i)))\n\n"
        if let n = trimmed(i.notebook) { out += "Notebook: \(esc(n))\n\n" }
        if !i.people.isEmpty { out += "People: \(esc(i.people.joined(separator: ", ")))\n\n" }

        if !i.highlights.isEmpty {
            out += "## Highlights\n\n"
            for h in i.highlights {
                var line = "- **\(h.time.clock)** \(esc(h.label.trimmingCharacters(in: .whitespaces)))"
                if let q = trimmed(h.quote) { line += ": “\(esc(q))”" }
                out += line + "\n"
            }
            out += "\n"
        }

        if let notes = trimmed(i.notes) {
            out += "## Notes\n\n"
            // The notes are Markdown already; headings move down a level
            for line in notes.components(separatedBy: .newlines) {
                let t = mdInert(line.trimmingCharacters(in: .whitespaces))
                out += (t.hasPrefix("#") ? "#" + t : t) + "\n"
            }
            out += "\n"
        }

        if !i.pictures.isEmpty {
            out += "## \(picturesNoun(i.pictures))\n\n"
            for (n, p) in i.pictures.enumerated() {
                let caption = trimmed(p.caption)
                let alt = caption.map(esc) ?? "\(p.isPhoto ? "photo" : "screen") at \(p.time.clock)"
                out += "![\(alt)](\(imageName(n, p)))\n\n"
                out += caption.map { "*\(p.time.clock)* · \(esc($0))\n\n" } ?? "*\(p.time.clock)*\n\n"
            }
        }

        if !i.marks.isEmpty {
            out += "## Marked moments\n\n"
            for m in i.marks {
                var line = "- \(m.time.clock) \(m.symbol) \(m.label)"
                if let n = trimmed(m.note) { line += ": \(esc(n))" }
                out += line + "\n"
            }
            out += "\n"
        }

        if !i.links.isEmpty {
            out += "## Links\n\n"
            for l in i.links {
                let label = trimmed(l.title).map(esc) ?? esc(l.url)
                let when = l.time.map { " (\($0.clock))" } ?? ""
                out += "- [\(label)](<\(l.url.replacingOccurrences(of: ">", with: "%3E"))>)\(when)\n"
            }
            out += "\n"
        }

        if let e = i.email, trimmed(e.body) != nil {
            out += "## Follow-up email\n\n**Subject:** \(esc(e.subject.trimmingCharacters(in: .whitespaces)))\n\n"
            for para in e.body.trimmingCharacters(in: .whitespacesAndNewlines).components(separatedBy: "\n\n") {
                out += para.components(separatedBy: "\n").map(esc).joined(separator: "  \n") + "\n\n"
            }
        }

        var also: [String] = []
        if i.guideMarkdown != nil { also.append("[\(i.kind.guideTitle)](<\(guideName(i.kind))>)") }
        if i.flashcardsCSV != nil { also.append("[Flashcards](\(flashcardsFile)) (CSV, for flashcard apps)") }
        if !i.transcript.isEmpty { also.append("[Transcript](\(transcriptFile))") }
        if !also.isEmpty { out += "## Also in this package\n\n" + also.map { "- \($0)\n" }.joined() + "\n" }

        out += "_\(footer(i))_\n"
        return out
    }

    /// AI-written Markdown kept as Markdown, but inert: no raw HTML and no
    /// images (a viewer would load them from the internet when the file
    /// opens). Headings, lists, bold and links stay.
    static func mdInert(_ line: String) -> String {
        line.replacingOccurrences(of: "<", with: "\\<")
            .replacingOccurrences(of: ">", with: "\\>")
            .replacingOccurrences(of: "![", with: "\\![")
    }

    // MARK: HTML

    static func htmlEscape(_ s: String) -> String {
        var out = ""
        for c in s {
            switch c {
            case "&": out += "&amp;"
            case "<": out += "&lt;"
            case ">": out += "&gt;"
            case "\"": out += "&quot;"
            case "'": out += "&#39;"
            default: out.append(c)
            }
        }
        return out
    }

    /// `**bold**` → <strong>, on already-escaped text.
    private static func inlineBold(_ escaped: String) -> String {
        var out = ""
        var rest = Substring(escaped)
        while let a = rest.range(of: "**") {
            let after = rest[a.upperBound...]
            guard let b = after.range(of: "**"), b.lowerBound > after.startIndex else { break }
            out += rest[..<a.lowerBound] + "<strong>" + after[..<b.lowerBound] + "</strong>"
            rest = after[b.upperBound...]
        }
        return out + rest
    }

    private static func boldHeading(_ line: String) -> String? {
        var s = Substring(line)
        if s.hasSuffix(":") { s = s.dropLast() }
        guard s.hasPrefix("**"), s.hasSuffix("**"), s.count > 4 else { return nil }
        var inner = s.dropFirst(2).dropLast(2)
        if inner.hasSuffix(":") { inner = inner.dropLast() }
        let t = inner.trimmingCharacters(in: .whitespaces)
        return t.isEmpty || t.contains("**") ? nil : t
    }

    /// The AI notes as HTML: headings, bullets, paragraphs and bold.
    /// Every character is escaped first.
    static func simpleMarkdownHTML(_ md: String) -> String {
        var out = ""
        var inList = false
        for raw in md.components(separatedBy: .newlines) {
            let line = raw.trimmingCharacters(in: .whitespaces)
            let bullet: String? = ["- ", "* ", "• "].first { line.hasPrefix($0) }.map { String(line.dropFirst($0.count)) }
            if bullet == nil && inList { out += "</ul>\n"; inList = false }
            if line.isEmpty { continue }
            if let b = bullet {
                if !inList { out += "<ul>\n"; inList = true }
                out += "<li>\(inlineBold(htmlEscape(b)))</li>\n"
            } else if line.hasPrefix("#") {
                out += "<h3>\(htmlEscape(line.drop { $0 == "#" }.trimmingCharacters(in: .whitespaces)))</h3>\n"
            } else if let h = boldHeading(line) {
                out += "<h3>\(htmlEscape(h))</h3>\n"
            } else {
                out += "<p>\(inlineBold(htmlEscape(line)))</p>\n"
            }
        }
        if inList { out += "</ul>\n" }
        return out
    }

    private static let style = """
        :root { color-scheme: light; }
        body { font: 15px/1.55 -apple-system, BlinkMacSystemFont, "Helvetica Neue", Arial, sans-serif; color: #1c1c1e; background: #fff; max-width: 760px; margin: 40px auto; padding: 0 24px; }
        h1 { font-size: 28px; line-height: 1.2; margin: 0 0 6px; }
        h2 { font-size: 19px; margin: 32px 0 10px; padding-bottom: 4px; border-bottom: 1px solid #e5e5ea; }
        h3 { font-size: 16px; margin: 18px 0 6px; }
        .meta { color: #636366; margin: 0 0 4px; }
        .time { font-variant-numeric: tabular-nums; color: #636366; margin-right: 6px; }
        ul { padding-left: 22px; }
        li { margin: 3px 0; }
        blockquote { margin: 4px 0 0; color: #3a3a3c; font-style: italic; }
        figure { margin: 18px 0 26px; break-inside: avoid; page-break-inside: avoid; }
        figure img { display: block; max-width: 100%; height: auto; border: 1px solid #e5e5ea; border-radius: 6px; }
        figcaption { margin-top: 6px; font-size: 14px; color: #3a3a3c; }
        .email { white-space: pre-wrap; background: #f5f5f7; border-radius: 8px; padding: 12px 14px; }
        .footer { margin-top: 40px; font-size: 13px; color: #8e8e93; }
        a { color: #0a5fd8; }
        @media print { body { margin: 0 auto; } h2 { break-after: avoid; } }
        """

    /// A self-contained web page (no scripts, no outside requests; images by
    /// relative path) to read and to print to PDF.
    static func html(_ i: Input) -> String {
        let e = htmlEscape
        var out = "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n"
        out += "<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n"
        out += "<title>\(e(i.title))</title>\n<style>\(style)</style>\n</head>\n<body>\n"
        out += "<h1>\(e(i.title))</h1>\n<p class=\"meta\">\(e(metaLine(i)))</p>\n"
        if let n = trimmed(i.notebook) { out += "<p class=\"meta\">Notebook: \(e(n))</p>\n" }
        if !i.people.isEmpty { out += "<p class=\"meta\">People: \(e(i.people.joined(separator: ", ")))</p>\n" }

        if !i.highlights.isEmpty {
            out += "<h2>Highlights</h2>\n<ul>\n"
            for h in i.highlights {
                out += "<li><span class=\"time\">\(h.time.clock)</span>\(e(h.label.trimmingCharacters(in: .whitespaces)))"
                if let q = trimmed(h.quote) { out += "<blockquote>“\(e(q))”</blockquote>" }
                out += "</li>\n"
            }
            out += "</ul>\n"
        }
        if let notes = trimmed(i.notes) { out += "<h2>Notes</h2>\n" + simpleMarkdownHTML(notes) }

        if !i.pictures.isEmpty {
            out += "<h2>\(picturesNoun(i.pictures))</h2>\n"
            for (n, p) in i.pictures.enumerated() {
                let caption = trimmed(p.caption)
                let alt = caption.map(e) ?? "\(p.isPhoto ? "photo" : "screen") at \(p.time.clock)"
                out += "<figure><img src=\"\(e(imageName(n, p)))\" alt=\"\(alt)\"><figcaption><span class=\"time\">\(p.time.clock)</span>\(caption.map(e) ?? "")</figcaption></figure>\n"
            }
        }
        if !i.marks.isEmpty {
            out += "<h2>Marked moments</h2>\n<ul>\n"
            for m in i.marks {
                out += "<li><span class=\"time\">\(m.time.clock)</span>\(e(m.symbol)) \(e(m.label))"
                if let n = trimmed(m.note) { out += ": \(e(n))" }
                out += "</li>\n"
            }
            out += "</ul>\n"
        }
        if !i.links.isEmpty {
            out += "<h2>Links</h2>\n<ul>\n"
            for l in i.links {
                let label = trimmed(l.title) ?? l.url
                let when = l.time.map { " <span class=\"time\">(\($0.clock))</span>" } ?? ""
                if l.url.hasPrefix("https://") || l.url.hasPrefix("http://") {
                    out += "<li><a href=\"\(e(l.url))\">\(e(label))</a>\(when)</li>\n"
                } else {
                    out += "<li>\(e(label))\(when)</li>\n"
                }
            }
            out += "</ul>\n"
        }
        if let m = i.email, let body = trimmed(m.body) {
            out += "<h2>Follow-up email</h2>\n<p><strong>Subject:</strong> \(e(m.subject.trimmingCharacters(in: .whitespaces)))</p>\n"
            out += "<div class=\"email\">\(e(body))</div>\n"
        }
        var also: [String] = []
        if i.guideMarkdown != nil { also.append("<a href=\"\(e(guideName(i.kind)))\">\(e(i.kind.guideTitle))</a>") }
        if i.flashcardsCSV != nil { also.append("<a href=\"\(flashcardsFile)\">Flashcards</a> (CSV, for flashcard apps)") }
        if !i.transcript.isEmpty { also.append("<a href=\"\(transcriptFile)\">Transcript</a>") }
        if !also.isEmpty { out += "<h2>Also in this package</h2>\n<ul>\n" + also.map { "<li>\($0)</li>\n" }.joined() + "</ul>\n" }
        out += "<p class=\"footer\">\(e(footer(i)))</p>\n</body>\n</html>\n"
        return out
    }

    // MARK: Transcript

    static func transcriptText(_ i: Input) -> String {
        var out = "\(i.title)\n\(i.when)\n\n"
        for l in i.transcript {
            let text = l.text.replacingOccurrences(of: "\n", with: " ").trimmingCharacters(in: .whitespaces)
            out += "[\(l.time.clock)] \(text)\n"
        }
        return out
    }

    // MARK: The package

    /// The files, relative to the package folder, in the order written.
    static func files(_ i: Input) -> [(String, FileData)] {
        let stem = fileStem(i.title)
        var out: [(String, FileData)] = [("\(stem).md", .text(markdown(i))), ("\(stem).html", .text(html(i)))]
        for (n, p) in i.pictures.enumerated() { out.append((imageName(n, p), .copy(p.fileURL))) }
        if !i.transcript.isEmpty { out.append((transcriptFile, .text(transcriptText(i)))) }
        if let g = i.guideMarkdown { out.append((guideName(i.kind), .text(g))) }
        if let c = i.flashcardsCSV { out.append((flashcardsFile, .text(c))) }
        return out
    }

    enum Failure: LocalizedError {
        case badPath(String)
        case zip
        var errorDescription: String? {
            switch self {
            case .badPath(let p): "Bad path in the package: \(p)"
            case .zip: "Couldn't make the zip file."
            }
        }
    }

    /// Write the package as a new folder `name` inside `parent` (replacing an
    /// earlier one with the same name: `parent` is the app's own temporary
    /// export folder). Returns the folder.
    @discardableResult
    static func writeFolder(_ i: Input, name: String, in parent: URL) throws -> URL {
        let fm = FileManager.default
        let dir = parent.appending(path: name, directoryHint: .isDirectory)
        try? fm.removeItem(at: dir)
        try fm.createDirectory(at: dir, withIntermediateDirectories: true)
        for (rel, data) in files(i) {
            guard !rel.isEmpty, !rel.hasPrefix("/"), !rel.split(separator: "/", omittingEmptySubsequences: false).contains(where: { $0 == ".." || $0.isEmpty }) else {
                throw Failure.badPath(rel)
            }
            let url = dir.appending(path: rel)
            try fm.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
            switch data {
            case .text(let t): try Data(t.utf8).write(to: url)
            case .copy(let src): try fm.copyItem(at: src, to: url)
            }
        }
        return dir
    }

    /// Zip a folder with the system's own archiver (the same one Files uses
    /// for "Compress"): the zip's root is the folder. Returns `<folder>.zip`
    /// next to it.
    static func zip(folder: URL) throws -> URL {
        let dest = folder.deletingLastPathComponent().appending(path: folder.lastPathComponent + ".zip")
        try? FileManager.default.removeItem(at: dest)
        var coordError: NSError?
        var copyError: Error?
        NSFileCoordinator().coordinate(readingItemAt: folder, options: .forUploading, error: &coordError) { tmp in
            do { try FileManager.default.copyItem(at: tmp, to: dest) } catch { copyError = error }
        }
        if let e = coordError ?? (copyError as NSError?) { throw e }
        guard FileManager.default.fileExists(atPath: dest.path) else { throw Failure.zip }
        return dest
    }

    /// Where packages are made before sharing: the app's temporary folder,
    /// emptied each time a new package is made.
    static var workFolder: URL { FileManager.default.temporaryDirectory.appending(path: "Export", directoryHint: .isDirectory) }

    static func clearWorkFolder() {
        try? FileManager.default.removeItem(at: workFolder)
    }
}

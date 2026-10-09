import SwiftUI

/// The notes renderer, the same for every recording type (meeting notes,
/// lecture notes, personal notes): `##` headings become headings, `-` / `*`
/// lines become bullets, and inline `**bold**` / `_italic_` is kept. Plain
/// `AttributedString(markdown:)` with inline-only syntax leaves `##` and `-`
/// in the text, which is what lecture notes looked like before.
enum NotesMarkdown {
    enum Block: Equatable {
        case heading(String, level: Int)
        case bullet(String, indent: Int)
        case paragraph(String)
    }

    /// Lines → blocks. Consecutive plain lines join into one paragraph
    /// (soft wrap); a blank line ends it.
    static func blocks(_ markdown: String) -> [Block] {
        var out: [Block] = []
        var paragraph: [String] = []
        func flush() {
            if !paragraph.isEmpty {
                out.append(.paragraph(paragraph.joined(separator: " ")))
                paragraph = []
            }
        }
        for raw in markdown.split(separator: "\n", omittingEmptySubsequences: false) {
            let line = String(raw)
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            if trimmed.isEmpty { flush(); continue }
            if let (text, level) = heading(trimmed) {
                flush()
                out.append(.heading(text, level: level))
            } else if let text = bullet(trimmed) {
                flush()
                let indent = line.prefix { $0 == " " || $0 == "\t" }.count
                out.append(.bullet(text, indent: indent >= 2 ? 1 : 0))
            } else {
                paragraph.append(trimmed)
            }
        }
        flush()
        return out
    }

    private static func heading(_ line: String) -> (String, Int)? {
        // A line that is only bold ("**Summary**", "**Action items:**") is a
        // heading too; models write it as often as "## Summary". Joined into
        // the next line, it read "Summary How cells turn…".
        for mark in ["**", "__"] where line.hasPrefix(mark) && line.hasSuffix(mark) && line.count > mark.count * 2 {
            let inner = line.dropFirst(mark.count).dropLast(mark.count)
            guard !inner.contains(mark) else { break }
            var text = inner.trimmingCharacters(in: .whitespaces)
            if text.hasSuffix(":") { text = String(text.dropLast()).trimmingCharacters(in: .whitespaces) }
            return text.isEmpty ? nil : (text, 2)
        }
        guard line.hasPrefix("#") else { return nil }
        let hashes = line.prefix { $0 == "#" }.count
        guard hashes <= 6 else { return nil }
        let rest = line.dropFirst(hashes)
        guard rest.first == " " || rest.isEmpty else { return nil }
        let text = rest.trimmingCharacters(in: .whitespaces)
        return text.isEmpty ? nil : (text, hashes)
    }

    private static func bullet(_ line: String) -> String? {
        for marker in ["- ", "* ", "• "] where line.hasPrefix(marker) {
            return String(line.dropFirst(marker.count)).trimmingCharacters(in: .whitespaces)
        }
        // "1. item" numbered lists
        if let dot = line.firstIndex(of: "."), line[..<dot].allSatisfy(\.isNumber), !line[..<dot].isEmpty,
           line.index(after: dot) < line.endIndex, line[line.index(after: dot)] == " " {
            return String(line[line.index(dot, offsetBy: 2)...])
        }
        return nil
    }

    /// Inline Markdown (bold, italic, code, links) for one block's text.
    static func inline(_ s: String) -> AttributedString {
        (try? AttributedString(markdown: s, options: .init(interpretedSyntax: .inlineOnlyPreservingWhitespace)))
            ?? AttributedString(s)
    }
}

struct NotesMarkdownView: View {
    let markdown: String

    var body: some View {
        let blocks = NotesMarkdown.blocks(markdown)
        VStack(alignment: .leading, spacing: 6) {
            ForEach(Array(blocks.enumerated()), id: \.offset) { i, block in
                switch block {
                case .heading(let text, let level):
                    Text(NotesMarkdown.inline(text))
                        .font(level <= 2 ? .headline : .subheadline.weight(.semibold))
                        .padding(.top, i == 0 ? 0 : 6)
                        .accessibilityAddTraits(.isHeader)
                case .bullet(let text, let indent):
                    HStack(alignment: .firstTextBaseline, spacing: 6) {
                        Text("•").foregroundStyle(.secondary)
                        Text(NotesMarkdown.inline(text))
                    }
                    .padding(.leading, CGFloat(indent) * 14)
                case .paragraph(let text):
                    Text(NotesMarkdown.inline(text))
                }
            }
        }
    }
}

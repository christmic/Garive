import SwiftUI

enum MobileResponseBlock: Equatable {
    case prose(String)
    case code(language: String?, text: String)
}

func parseMobileResponseBlocks(_ value: String) -> [MobileResponseBlock] {
    var result: [MobileResponseBlock] = []
    var buffer: [String] = []
    var language: String?
    var inCode = false

    func trimmedProse(_ lines: [String]) -> [String] {
        var lines = lines
        while lines.first?.isEmpty == true { lines.removeFirst() }
        while lines.last?.isEmpty == true { lines.removeLast() }
        return lines
    }

    for line in value.replacingOccurrences(of: "\r\n", with: "\n").components(separatedBy: "\n") {
        let marker = line.trimmingCharacters(in: .whitespaces)
        if !inCode && line.drop(while: { $0.isWhitespace }).hasPrefix("```") {
            let prose = trimmedProse(buffer)
            if !prose.isEmpty { result.append(.prose(prose.joined(separator: "\n"))) }
            buffer.removeAll(keepingCapacity: true)
            let suffix = String(marker.dropFirst(3)).trimmingCharacters(in: .whitespaces)
            language = suffix.isEmpty ? nil : String(suffix.prefix(32))
            inCode = true
        } else if inCode && marker == "```" {
            result.append(.code(language: language, text: buffer.joined(separator: "\n")))
            buffer.removeAll(keepingCapacity: true)
            language = nil
            inCode = false
        } else {
            buffer.append(line)
        }
    }
    if inCode {
        result.append(.code(language: language, text: buffer.joined(separator: "\n")))
    } else {
        let prose = trimmedProse(buffer)
        if !prose.isEmpty { result.append(.prose(prose.joined(separator: "\n"))) }
    }
    return result
}

#if GARIVE_SHARED_AVAILABLE
struct MobileResponseText: View {
    let text: String

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            ForEach(Array(parseMobileResponseBlocks(text).enumerated()), id: \.offset) { _, block in
                switch block {
                case let .prose(value):
                    Text(value).textSelection(.enabled)
                case let .code(language, value):
                    VStack(alignment: .leading, spacing: 6) {
                        if let language {
                            Text(language).font(.caption2.weight(.semibold)).foregroundStyle(.secondary)
                        }
                        ScrollView(.horizontal, showsIndicators: true) {
                            Text(value)
                                .font(.system(.body, design: .monospaced))
                                .fixedSize(horizontal: true, vertical: true)
                                .textSelection(.enabled)
                                .accessibilityLabel("Agent code block")
                        }
                    }
                    .padding(12)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(GarivePalette.raised, in: RoundedRectangle(cornerRadius: 14))
                    .overlay(RoundedRectangle(cornerRadius: 14).stroke(.secondary.opacity(0.24)))
                }
            }
        }
    }
}
#endif

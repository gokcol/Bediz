import Foundation

/// Caption times are milliseconds, relative to a clip in a project or to the whole exported timeline.
struct VideoSubtitleCue: Codable, Equatable, Sendable {
    var start: Int64
    var end: Int64
    var text: String

    func clipped(to range: Range<Int64>) -> Self? {
        let first = max(start, range.lowerBound), last = min(end, range.upperBound)
        guard first < last else { return nil }
        return Self(start: first - range.lowerBound, end: last - range.lowerBound, text: text)
    }

    static func valid(_ cues: [Self], length: Int64) -> Bool {
        guard cues.count <= 50_000 else { return false }
        var end: Int64 = 0
        for cue in cues {
            guard cue.start >= end, cue.end > cue.start, cue.end <= length,
                  !cue.text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
                  cue.text.utf8.count <= 2_048 else { return false }
            end = cue.end
        }
        return true
    }

    static func serialize(_ cues: [Self]) -> String {
        func stamp(_ time: Int64) -> String { VideoTime.format(time).replacingOccurrences(of: ".", with: ",") }
        return cues.enumerated().map { index, cue in
            "\(index + 1)\n\(stamp(cue.start)) --> \(stamp(cue.end))\n\(cue.text)\n"
        }.joined(separator: "\n")
    }

    static func parse(_ input: String, length: Int64) -> [Self]? {
        guard input.utf8.count <= 16 * 1_024 * 1_024 else { return nil }
        let content = input.replacingOccurrences(of: "\r\n", with: "\n").replacingOccurrences(of: "\r", with: "\n")
            .trimmingCharacters(in: .whitespacesAndNewlines)
        if content.isEmpty { return [] }
        var cues: [Self] = []
        for block in content.components(separatedBy: "\n\n") where !block.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            let lines = block.split(separator: "\n")
            guard lines.count >= 3, Int(lines[0].trimmingCharacters(in: .whitespaces)) != nil else { return nil }
            let stamps = lines[1].components(separatedBy: "-->").map { $0.replacingOccurrences(of: ",", with: ".") }
            guard stamps.count == 2, let start = VideoTime.parse(stamps[0]), let end = VideoTime.parse(stamps[1]) else { return nil }
            cues.append(Self(start: start, end: end, text: lines.dropFirst(2).joined(separator: "\n")))
        }
        return valid(cues, length: length) ? cues : nil
    }

    static func active(in cues: [Self], at time: Int64) -> Self? {
        var low = 0, high = cues.count
        while low < high {
            let middle = (low + high) / 2
            if cues[middle].end <= time { low = middle + 1 }
            else { high = middle }
        }
        guard cues.indices.contains(low), cues[low].start <= time else { return nil }
        return cues[low]
    }
}

import Foundation

enum VideoTime {
    static func format(_ milliseconds: Int64) -> String {
        let value = max(0, milliseconds)
        return String(format: "%02lld:%02lld:%02lld.%03lld",
                      value / 3_600_000, value / 60_000 % 60, value / 1_000 % 60, value % 1_000)
    }

    static func parse(_ input: String) -> Int64? {
        let parts = input.trimmingCharacters(in: .whitespacesAndNewlines).split(separator: ":", omittingEmptySubsequences: false)
        guard (1...3).contains(parts.count),
              let last = parts.last else { return nil }
        let seconds = last.split(separator: ".", omittingEmptySubsequences: false)
        guard (1...2).contains(seconds.count), let whole = digits(seconds[0]), whole <= maximum / 1_000,
              parts.count == 1 || whole < 60 else { return nil }
        var milliseconds: Int64 = 0
        if seconds.count == 2 {
            let fraction = seconds[1]
            guard (1...3).contains(fraction.count), let value = digits(fraction) else { return nil }
            milliseconds = value * [100, 10, 1][fraction.count - 1]
        }
        var total = whole
        var multiplier: Int64 = 60
        for (index, part) in parts.dropLast().reversed().enumerated() {
            guard let value = digits(part), index != 0 || parts.count == 2 || value < 60,
                  value <= maximum / multiplier else { return nil }
            total += value * multiplier
            multiplier *= 60
        }
        guard total <= maximum / 1_000 else { return nil }
        let result = total * 1_000 + milliseconds
        return result <= maximum ? result : nil
    }

    static let maximum: Int64 = 100 * 3_600_000

    private static func digits(_ text: Substring) -> Int64? {
        guard !text.isEmpty, text.utf8.allSatisfy({ (48...57).contains($0) }) else { return nil }
        return Int64(text)
    }
}

enum VideoTransition: String, CaseIterable, Codable {
    case cut = "Cut"
    case dissolve = "Hold & dissolve"
    case dip = "Dip to black"
    case slide = "Slide in"
    case wipe = "Wipe across"
}

enum VideoCardStyle: String, CaseIterable, Codable {
    case title = "Title"
    case brief = "Information"
    case chapter = "Chapter"
    case photo = "Photo"
    case end = "End credits"
    case blank = "Blank"

    var defaults: (String, String) {
        switch self {
        case .title: ("Your title", "Add a subtitle")
        case .brief: ("What to know", "Add your key points")
        case .chapter: ("Next chapter", "")
        case .photo: ("Photo story", "Add a caption")
        case .end: ("Thanks for watching", "Add credits or contact information")
        case .blank: ("", "")
        }
    }
}

enum VideoArtworkKind: String, CaseIterable, Codable {
    case text = "Text"
    case callout = "Callout"
    case arrow = "Arrow"
    case outline = "Rectangle"
    case ellipse = "Ellipse"
    case highlight = "Highlight"
    case spotlight = "Cursor spotlight"
    case step = "Step marker"
    case image = "Image or Bediz artwork"
    case frameEffects = "Frame effects"

    var usesText: Bool { self == .text || self == .callout || self == .step }
}

enum VideoInk: String, CaseIterable, Codable {
    case white = "White"
    case blue = "Blue"
    case yellow = "Yellow"
    case coral = "Coral"
    case mint = "Mint"
    case black = "Black"
}

struct VideoFrameReference: Codable, Equatable {
    let picture: Data
    // Effects already keep their original annotations in the render data.
    let objects: Data?

    var isValid: Bool {
        !picture.isEmpty && picture.count <= 32 * 1_024 * 1_024
            && (objects.map { !$0.isEmpty && $0.count <= 32 * 1_024 * 1_024 } ?? true)
    }
}

struct VideoSpotlightPoint: Codable, Equatable {
    var x: Double
    var y: Double

    var isValid: Bool { (0...1).contains(x) && (0...1).contains(y) }
}

struct VideoArtwork: Codable, Equatable {
    var kind: VideoArtworkKind
    var text = ""
    var image: Data?
    var ink: VideoInk = .white
    var x: Double = 0.1
    var y: Double = 0.15
    var width: Double = 0.35
    var spotlightEnd: VideoSpotlightPoint?
    var frameReference: VideoFrameReference?

    var editableFrameObjects: Data? { frameReference?.objects ?? (kind == .frameEffects ? image : nil) }

    var isValid: Bool {
        [x, y, width].allSatisfy { $0.isFinite && (0...1).contains($0) }
            && width >= 0.04 && text.utf8.count <= 32_768
            && (image?.count ?? 0) <= 32 * 1_024 * 1_024
            && ((kind != .image && kind != .frameEffects) || image != nil)
            && (spotlightEnd?.isValid ?? true)
            && (frameReference.map { $0.isValid && editableFrameObjects != nil && (kind == .image || kind == .frameEffects) } ?? true)
    }
}

struct VideoCard: Codable, Equatable {
    var style: VideoCardStyle
    var title: String
    var body: String
    var accent: VideoInk = .blue
    var image: Data?

    init(style: VideoCardStyle) {
        self.style = style
        (title, body) = style.defaults
    }

    var isValid: Bool {
        title.utf8.count <= 32_768 && body.utf8.count <= 32_768 && (image?.count ?? 0) <= 32 * 1_024 * 1_024
    }
}

struct VideoOverlay: Codable, Equatable {
    var id = UUID()
    var start: Int64
    var end: Int64
    var art: VideoArtwork
    var motion: VideoSpotlightTiming?
}

struct VideoSpotlightTiming: Codable, Equatable {
    var offset: Int64
    var length: Int64

    func contains(_ duration: Int64) -> Bool {
        offset >= 0 && offset < length && length <= VideoTime.maximum && duration <= length - offset
    }
}

struct VideoAudioFades: Codable, Equatable {
    var fadeIn: Int64
    var fadeOut: Int64
    var length: Int64
    var offset: Int64 = 0

    func contains(_ duration: Int64) -> Bool {
        fadeIn >= 0 && fadeOut >= 0 && length <= VideoTime.maximum
            && offset >= 0 && offset < length && duration <= length - offset
            && fadeIn <= length && fadeOut <= length - fadeIn
    }
}

struct VideoEditClip: Codable, Equatable, Identifiable {
    var id = UUID()
    var sourceStart: Int64 = 0
    var length: Int64
    var card: VideoCard?
    var freeze: Data?
    var audioFades: VideoAudioFades?
    var transition: VideoTransition = .cut
    var transitionLength: Int64 = 450
    var overlays: [VideoOverlay] = []
    var subtitles: [VideoSubtitleCue]?

    var isStill: Bool { card != nil || freeze != nil }

    var caption: String {
        if freeze != nil { return "Freeze frame" }
        guard let card else { return "Video" }
        let title = card.title.trimmingCharacters(in: .whitespacesAndNewlines)
        return title.isEmpty ? card.style.rawValue : title
    }

    func slice(_ span: Range<Int64>) -> Self {
        var clip = self
        if span.lowerBound != 0 {
            clip.id = UUID()
            if !isStill { clip.sourceStart += span.lowerBound }
            clip.audioFades?.offset += span.lowerBound
            clip.transition = .cut
        }
        clip.length = span.upperBound - span.lowerBound
        clip.subtitles = subtitles?.compactMap { $0.clipped(to: span) }
        clip.overlays = overlays.compactMap { overlay in
            let start = max(overlay.start, span.lowerBound)
            let end = min(overlay.end, span.upperBound)
            guard start < end else { return nil }
            var result = overlay
            result.motion?.offset += start - overlay.start
            result.start = start - span.lowerBound
            result.end = end - span.lowerBound
            return result
        }
        return clip
    }

    mutating func resizeStill(to duration: Int64) {
        length = duration
        subtitles = subtitles?.compactMap { $0.clipped(to: 0..<duration) }
        overlays = overlays.compactMap { layer in
            guard layer.start < duration else { return nil }
            var clipped = layer
            clipped.end = min(clipped.end, duration)
            return clipped
        }
    }
}

struct VideoEditProject: Codable, Equatable {
    private(set) var version = 1
    var source: URL
    private(set) var clips: [VideoEditClip]

    init(source: URL, duration: Int64) {
        self.source = source
        self.clips = [.init(length: duration)]
    }

    private enum CodingKeys: String, CodingKey { case version, source, clips, frameReferences }

    init(from decoder: Decoder) throws {
        let values = try decoder.container(keyedBy: CodingKeys.self)
        version = try values.decode(Int.self, forKey: .version)
        source = try values.decode(URL.self, forKey: .source)
        clips = try values.decode([VideoEditClip].self, forKey: .clips)
        if let references = try values.decodeIfPresent([String: VideoFrameReference].self, forKey: .frameReferences) {
            for clip in clips.indices {
                for layer in clips[clip].overlays.indices {
                    let key = clips[clip].overlays[layer].id.uuidString
                    if let reference = references[key] {
                        clips[clip].overlays[layer].art.frameReference = reference
                    }
                }
            }
        }
    }

    func encode(to encoder: Encoder) throws {
        var values = encoder.container(keyedBy: CodingKeys.self)
        try values.encode(version, forKey: .version)
        try values.encode(source, forKey: .source)
        var references: [String: VideoFrameReference] = [:]
        let savedClips = clips.map { clip in
            var saved = clip
            // A split overlay shares the reference across its timeline fragments.
            for layer in saved.overlays.indices {
                if let reference = saved.overlays[layer].art.frameReference {
                    references[saved.overlays[layer].id.uuidString] = reference
                    saved.overlays[layer].art.frameReference = nil
                }
            }
            return saved
        }
        try values.encode(savedClips, forKey: .clips)
        if !references.isEmpty { try values.encode(references, forKey: .frameReferences) }
    }

    var length: Int64 { clips.reduce(0) { $0 + $1.length } }

    var hasSubtitles: Bool { clips.contains { !($0.subtitles ?? []).isEmpty } }

    var subtitles: [VideoSubtitleCue] {
        var combined: [VideoSubtitleCue] = []
        for (offset, clip) in entries() {
            for cue in clip.subtitles ?? [] {
                let start = offset + cue.start, end = offset + cue.end
                if let last = combined.indices.last, combined[last].end == start, combined[last].text == cue.text {
                    combined[last].end = end
                } else {
                    combined.append(.init(start: start, end: end, text: cue.text))
                }
            }
        }
        return combined
    }

    mutating func replaceSubtitles(_ cues: [VideoSubtitleCue]) -> Bool {
        guard VideoSubtitleCue.valid(cues, length: length) else { return false }
        var offset: Int64 = 0
        var first = 0
        for index in clips.indices {
            let end = offset + clips[index].length
            while first < cues.count && cues[first].end <= offset { first += 1 }
            let local = cues[first...].prefix { $0.start < end }.compactMap { $0.clipped(to: offset..<end) }
            clips[index].subtitles = local.isEmpty ? nil : local
            offset = end
        }
        return true
    }

    func entries() -> [(offset: Int64, clip: VideoEditClip)] {
        var offset: Int64 = 0
        return clips.map { clip in
            defer { offset += clip.length }
            return (offset, clip)
        }
    }

    func location(at time: Int64) -> (index: Int, offset: Int64)? {
        let time = max(time, 0)
        var offset: Int64 = 0
        for (index, clip) in clips.enumerated() {
            if time < offset + clip.length || index == clips.count - 1 { return (index, offset) }
            offset += clip.length
        }
        return nil
    }

    func portion(_ span: Range<Int64>) -> [VideoEditClip] {
        entries().compactMap { offset, clip in
            let start = max(span.lowerBound, offset)
            let end = min(span.upperBound, offset + clip.length)
            return start < end ? clip.slice((start - offset)..<(end - offset)) : nil
        }
    }

    mutating func split(at time: Int64) -> Bool {
        guard clips.count < 10_000, time > 0, time < length,
              let (index, offset) = location(at: time), offset != time else { return false }
        let clip = clips[index]
        let point = time - offset
        clips.replaceSubrange(index...index, with: [clip.slice(0..<point), clip.slice(point..<clip.length)])
        return true
    }

    mutating func remove(_ span: Range<Int64>) -> Bool {
        guard span.lowerBound >= 0, span.upperBound <= length, !span.isEmpty else { return false }
        clips = portion(0..<span.lowerBound) + portion(span.upperBound..<length)
        return true
    }

    mutating func insert(_ inserted: [VideoEditClip], at time: Int64) -> Bool {
        guard !inserted.isEmpty, time >= 0, time <= length,
              length + inserted.reduce(0, { $0 + $1.length }) <= VideoTime.maximum else { return false }
        let leading = portion(0..<time)
        let trailing = portion(time..<length)
        guard leading.count + inserted.count + trailing.count <= 10_000 else { return false }
        var overlayIDs: [UUID: UUID] = [:]
        clips = leading + inserted.map { item in
            var clip = item
            clip.id = UUID()
            for index in clip.overlays.indices {
                let original = clip.overlays[index].id
                let replacement = overlayIDs[original, default: UUID()]
                overlayIDs[original] = replacement
                clip.overlays[index].id = replacement
            }
            return clip
        } + trailing
        return true
    }

    mutating func setClip(_ index: Int, _ updated: VideoEditClip) -> Bool {
        guard clips.indices.contains(index), clips[index].id == updated.id,
              updated.length > 0, length - clips[index].length + updated.length <= VideoTime.maximum else { return false }
        clips[index] = updated
        return true
    }

    mutating func addOverlay(_ art: VideoArtwork, over span: Range<Int64>) -> Bool {
        guard span.lowerBound >= 0, span.upperBound <= length, !span.isEmpty, art.isValid else { return false }
        let entries = entries()
        let affected = entries.enumerated().compactMap { index, entry -> (Int, Int64, Int64)? in
            let start = max(span.lowerBound, entry.offset)
            let end = min(span.upperBound, entry.offset + entry.clip.length)
            return start < end ? (index, start - entry.offset, end - entry.offset) : nil
        }
        guard affected.allSatisfy({ clips[$0.0].overlays.count < 2_000 }) else { return false }
        let id = UUID()
        for (index, start, end) in affected {
            let clipTime = entries[index].offset
            let motion = art.kind == .spotlight && art.spotlightEnd != nil
                ? VideoSpotlightTiming(offset: clipTime + start - span.lowerBound, length: span.upperBound - span.lowerBound)
                : nil
            clips[index].overlays.append(.init(id: id, start: start, end: end, art: art, motion: motion))
        }
        return true
    }

    func overlayBounds(_ id: UUID) -> Range<Int64>? {
        let fragments = entries().compactMap { entry in
            entry.clip.overlays.first(where: { $0.id == id }).map {
                (entry.offset + $0.start)..<(entry.offset + $0.end)
            }
        }
        guard let first = fragments.first, let last = fragments.last else { return nil }
        return first.lowerBound..<last.upperBound
    }

    func overlayDuration(_ id: UUID) -> Int64 {
        clips.reduce(0) { total, clip in
            guard let layer = clip.overlays.first(where: { $0.id == id }) else { return total }
            return total + layer.end - layer.start
        }
    }

    mutating func replaceOverlay(_ id: UUID, art: VideoArtwork) -> Bool {
        guard art.isValid else { return false }
        var found = false
        for clip in clips.indices {
            if let layer = clips[clip].overlays.firstIndex(where: { $0.id == id }) {
                clips[clip].overlays[layer].art = art
                found = true
            }
        }
        return found
    }

    mutating func editOverlay(_ id: UUID, art: VideoArtwork, over span: Range<Int64>) -> Bool {
        var edited = self
        guard edited.removeOverlay(id), edited.addOverlay(art, over: span) else { return false }
        self = edited
        return true
    }

    mutating func removeOverlay(_ id: UUID) -> Bool {
        var removed = false
        for index in clips.indices {
            if let layer = clips[index].overlays.firstIndex(where: { $0.id == id }) {
                clips[index].overlays.remove(at: layer)
                removed = true
            }
        }
        return removed
    }

    func validate(sourceDuration: Int64) throws {
        guard version == 1, source.isFileURL, (1...10_000).contains(clips.count),
              Set(clips.map(\.id)).count == clips.count else { throw VideoEditError.invalidProject }
        var total: Int64 = 0
        for clip in clips {
            guard clip.length > 0, clip.length <= VideoTime.maximum - total,
                  clip.sourceStart >= 0, (1...5_000).contains(clip.transitionLength),
                  clip.overlays.count <= 2_000,
                  Set(clip.overlays.map(\.id)).count == clip.overlays.count,
                  VideoSubtitleCue.valid(clip.subtitles ?? [], length: clip.length),
                  clip.overlays.allSatisfy({
                      $0.start >= 0 && $0.start < $0.end && $0.end <= clip.length && $0.art.isValid
                          && ($0.motion?.contains($0.end - $0.start) ?? true)
                  }),
                  clip.audioFades.map({ !clip.isStill && $0.contains(clip.length) }) ?? true
            else { throw VideoEditError.invalidProject }
            if let card = clip.card {
                guard card.isValid, clip.freeze == nil else { throw VideoEditError.invalidProject }
            } else if let freeze = clip.freeze {
                guard !freeze.isEmpty, freeze.count <= 32 * 1_024 * 1_024 else { throw VideoEditError.invalidProject }
            } else {
                guard clip.sourceStart <= sourceDuration, clip.length <= sourceDuration - clip.sourceStart
                else { throw VideoEditError.invalidProject }
            }
            total += clip.length
        }
    }
}

enum VideoEditError: LocalizedError {
    case invalidProject, noVideo, noExport, artwork, render

    var errorDescription: String? {
        switch self {
        case .invalidProject: "The video project is incomplete or refers to a different recording."
        case .noVideo: "The source has no readable video track."
        case .noExport: "Bediz couldn’t export this video as MP4."
        case .artwork: "Choose an image or a Bediz project that can be opened as artwork."
        case .render: "Bediz couldn’t render a video frame."
        }
    }
}

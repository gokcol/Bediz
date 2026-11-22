@preconcurrency import AVFoundation
import Speech

@available(macOS 26, *)
enum VideoSpeech {
    struct Language {
        let locale: Locale
        let usesDictation: Bool

        var name: String {
            Locale.current.localizedString(forIdentifier: locale.identifier) ?? locale.identifier
        }

        func module() -> any SpeechModule {
            if usesDictation { return DictationTranscriber(locale: locale, preset: .timeIndexedLongDictation) }
            return SpeechTranscriber(locale: locale, transcriptionOptions: [], reportingOptions: [], attributeOptions: [.audioTimeRange])
        }
    }

    enum Failure: LocalizedError {
        case noAudio, noSpeech, unavailable, invalidResult

        var errorDescription: String? {
            switch self {
            case .noAudio: "The timeline has no audio to transcribe."
            case .noSpeech: "Bediz couldn’t find spoken words in this audio."
            case .unavailable: "On-device speech recognition is unavailable for this language on this Mac."
            case .invalidResult: "This transcript has too many subtitle cues. Try a shorter video."
            }
        }
    }

    static func languages() async -> [Language] {
        let primary = SpeechTranscriber.isAvailable ? await SpeechTranscriber.supportedLocales : []
        let dictation = await DictationTranscriber.supportedLocales
        let primaryIDs = Set(primary.map { $0.identifier(.bcp47) })
        let choices = primary.map { Language(locale: $0, usesDictation: false) }
            + dictation.filter { !primaryIDs.contains($0.identifier(.bcp47)) }.map { Language(locale: $0, usesDictation: true) }
        return choices.sorted { first, second in
            let a = first.locale.identifier(.bcp47), b = second.locale.identifier(.bcp47)
            if a == "en-US" { return b != a }
            if b == "en-US" { return false }
            return first.name.localizedStandardCompare(second.name) == .orderedAscending
        }
    }

    static func isInstalled(_ language: Language) async -> Bool {
        await AssetInventory.status(forModules: [language.module()]) == .installed
    }

    static func download(_ language: Language) async throws {
        let module = language.module()
        if let request = try await AssetInventory.assetInstallationRequest(supporting: [module]) {
            try await request.downloadAndInstall()
        }
        try Task.checkCancellation()
        guard await AssetInventory.status(forModules: [module]) == .installed else { throw Failure.unavailable }
    }

    static func audio(from render: VideoEditRender, length: Int64, to url: URL) async throws {
        let tracks = try await render.asset.loadTracks(withMediaType: .audio)
        guard !tracks.isEmpty else { throw Failure.noAudio }
        guard let export = AVAssetExportSession(asset: render.asset, presetName: AVAssetExportPresetAppleM4A)
        else { throw Failure.noAudio }
        export.audioMix = render.audioMix
        export.timeRange = CMTimeRange(start: .zero, duration: videoTime(length))
        try await export.export(to: url, as: .m4a)
    }

    static func transcribe(_ url: URL, language: Language, length: Int64) async throws -> [VideoSubtitleCue] {
        let file = try AVAudioFile(forReading: url)
        let module = language.module()
        guard await AssetInventory.status(forModules: [module]) == .installed else { throw Failure.unavailable }
        let analyzer = SpeechAnalyzer(modules: [module])
        let cues: [VideoSubtitleCue]
        if let speech = module as? SpeechTranscriber {
            cues = try await analyze(speech.results, text: { $0.text }, file: file, analyzer: analyzer, length: length)
        } else if let dictation = module as? DictationTranscriber {
            cues = try await analyze(dictation.results, text: { $0.text }, file: file, analyzer: analyzer, length: length)
        } else {
            throw Failure.unavailable
        }
        try Task.checkCancellation()
        let sorted = cues.sorted { $0.start < $1.start }
        var normalized: [VideoSubtitleCue] = []
        for var cue in sorted {
            cue.start = max(normalized.last?.end ?? 0, cue.start)
            if cue.end > cue.start { normalized.append(cue) }
        }
        guard !normalized.isEmpty else { throw Failure.noSpeech }
        guard VideoSubtitleCue.valid(normalized, length: length) else { throw Failure.invalidResult }
        return normalized
    }

    private static func analyze<Results: AsyncSequence & Sendable>(
        _ sequence: Results, text: @escaping @Sendable (Results.Element) -> AttributedString,
        file: AVAudioFile, analyzer: SpeechAnalyzer, length: Int64
    ) async throws -> [VideoSubtitleCue] where Results.Element: SpeechModuleResult & Sendable {
        try await withTaskCancellationHandler {
            async let cues: [VideoSubtitleCue] = collect(sequence, text: text, length: length)
            do {
                if let end = try await analyzer.analyzeSequence(from: file) {
                    try await analyzer.finalizeAndFinish(through: end)
                } else {
                    await analyzer.cancelAndFinishNow()
                }
                return try await cues
            } catch {
                await analyzer.cancelAndFinishNow()
                throw error
            }
        } onCancel: {
            Task { await analyzer.cancelAndFinishNow() }
        }
    }

    private static func collect<Results: AsyncSequence>(
        _ sequence: Results, text: (Results.Element) -> AttributedString, length: Int64
    ) async throws -> [VideoSubtitleCue] where Results.Element: SpeechModuleResult {
        var cues: [VideoSubtitleCue] = []
        for try await result in sequence where result.isFinal {
            try Task.checkCancellation()
            let content = text(result)
            var words: [(text: String, timing: CMTimeRange)] = []
            var prefix = ""
            for run in content.runs {
                let fragment = String(content[run.range].characters)
                if let timing = run.audioTimeRange {
                    words.append((prefix + fragment, timing))
                    prefix = ""
                } else if let index = words.indices.last {
                    words[index].text += fragment
                } else {
                    prefix += fragment
                }
            }
            if words.isEmpty {
                if let line = cue(String(content.characters), range: result.range, length: length) { cues.append(line) }
            } else {
                var line = ""
                var start: CMTime?, end: CMTime = .zero
                for (word, timing) in words {
                    if let first = start, (line.count + word.count > 76 || (timing.end - first).seconds > 6),
                       let part = cue(line, range: CMTimeRange(start: first, end: end), length: length) {
                        cues.append(part)
                        line = ""
                        start = nil
                    }
                    if start == nil { start = timing.start }
                    end = timing.end
                    line += word
                }
                if let start, let part = cue(line, range: CMTimeRange(start: start, end: end), length: length) { cues.append(part) }
            }
        }
        return cues
    }

    private static func cue(_ text: String, range: CMTimeRange, length: Int64) -> VideoSubtitleCue? {
        let line = text.split(whereSeparator: \.isWhitespace).joined(separator: " ")
        guard !line.isEmpty, range.start.seconds.isFinite, range.end.seconds.isFinite else { return nil }
        let start = Int64(max(0, min(Double(length), range.start.seconds * 1_000)).rounded())
        let end = Int64(max(0, min(Double(length), range.end.seconds * 1_000)).rounded())
        return start < end ? .init(start: start, end: end, text: String(line.prefix(500))) : nil
    }
}

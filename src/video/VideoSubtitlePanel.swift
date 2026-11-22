import AppKit
import UniformTypeIdentifiers

@available(macOS 26, *)
@MainActor
final class VideoSubtitlePanel: NSObject, NSTextViewDelegate {
    private let window = NSPanel()
    private let language = NSPopUpButton(frame: .zero, pullsDown: false)
    private let editor = NSTextView()
    private let status = VideoEditUI.label("Checking installed languages…", quiet: true)
    private let spinner = NSProgressIndicator()
    private lazy var downloadButton = VideoEditUI.button("Download language", "arrow.down.circle", self, #selector(download), text: true)
    private lazy var generateButton = VideoEditUI.button("Generate subtitles", "captions.bubble", self, #selector(generate), text: true)
    private lazy var copyButton = VideoEditUI.button("Copy transcript", "doc.on.doc", self, #selector(copyTranscript))
    private lazy var saveButton = VideoEditUI.button("Save subtitles as SRT", "square.and.arrow.down", self, #selector(saveSRT))
    private lazy var applyButton = VideoEditUI.button("Add to Timeline", "checkmark", self, #selector(apply), text: true, primary: true)
    private let length: Int64
    private let initial: [VideoSubtitleCue]
    private let prepare: (URL) async throws -> Void
    private var languages: [VideoSpeech.Language] = []
    private var installed = false
    private var operation: Task<Void, Never>?
    private var result: [VideoSubtitleCue]?

    init(length: Int64, existing: [VideoSubtitleCue], prepare: @escaping (URL) async throws -> Void) {
        self.length = length
        initial = existing
        self.prepare = prepare
        super.init()
        let cancel = VideoEditUI.button("Close", "xmark", self, #selector(close), text: true)
        cancel.keyEquivalent = "\u{1b}"
        if !existing.isEmpty {
            applyButton.title = "Update Timeline"
            applyButton.setAccessibilityLabel("Update Timeline")
        }
        window.styleMask = [.titled]
        window.title = "Auto Subtitles"
        let root = NSView()
        root.appearance = NSAppearance(named: .darkAqua)
        window.contentView = root
        language.setAccessibilityLabel("Spoken language")
        language.target = self
        language.action = #selector(checkLanguage)
        language.widthAnchor.constraint(equalToConstant: 178).isActive = true
        status.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        editor.isRichText = false
        editor.allowsUndo = true
        editor.font = .monospacedSystemFont(ofSize: 12, weight: .regular)
        editor.textColor = .textColor
        editor.textContainerInset = NSSize(width: 10, height: 8)
        editor.isVerticallyResizable = true
        editor.autoresizingMask = [.width]
        editor.textContainer?.widthTracksTextView = true
        editor.setAccessibilityLabel("Editable subtitle text and timestamps")
        editor.isAutomaticQuoteSubstitutionEnabled = false
        editor.isAutomaticDashSubstitutionEnabled = false
        editor.isAutomaticTextReplacementEnabled = false
        editor.string = VideoSubtitleCue.serialize(existing)
        editor.delegate = self
        let scroll = NSScrollView()
        scroll.borderType = .bezelBorder
        scroll.hasVerticalScroller = true
        scroll.documentView = editor
        scroll.heightAnchor.constraint(equalToConstant: 245).isActive = true
        spinner.style = .spinning
        spinner.controlSize = .small
        spinner.isDisplayedWhenStopped = false
        let explanation = VideoEditUI.label("Speech is processed on this Mac. Review the text and times before adding subtitles.", quiet: true)
        let content = VideoEditUI.column([
            explanation,
            VideoEditUI.row([language, downloadButton, VideoEditUI.gap(), generateButton]),
            scroll,
            VideoEditUI.row([spinner, status, VideoEditUI.gap(), copyButton, saveButton, cancel, applyButton]),
        ], spacing: 10)
        content.translatesAutoresizingMaskIntoConstraints = false
        root.addSubview(content)
        NSLayoutConstraint.activate([
            content.leadingAnchor.constraint(equalTo: root.leadingAnchor, constant: 16),
            content.trailingAnchor.constraint(equalTo: root.trailingAnchor, constant: -16),
            content.topAnchor.constraint(equalTo: root.topAnchor, constant: 14),
            content.bottomAnchor.constraint(equalTo: root.bottomAnchor, constant: -14),
        ])
        window.setContentSize(.init(width: 620, height: content.fittingSize.height + 28))
        updateControls()
    }

    func show(in parent: NSWindow, completion: @escaping ([VideoSubtitleCue]?) -> Void) {
        parent.beginSheet(window) { [self] _ in completion(result) }
        run {
            self.languages = await VideoSpeech.languages()
            try Task.checkCancellation()
            self.language.addItems(withTitles: self.languages.map(\.name))
            try await self.refreshLanguage()
        }
    }

    private var selectedLanguage: VideoSpeech.Language? {
        let index = language.indexOfSelectedItem
        return languages.indices.contains(index) ? languages[index] : nil
    }

    private var cues: [VideoSubtitleCue]? { VideoSubtitleCue.parse(editor.string, length: length) }

    private func showStatus(_ message: String) {
        status.stringValue = message
        status.toolTip = message
    }

    private func updateControls() {
        let ready = operation == nil
        let parsed = ready ? cues : nil
        let hasText = !(parsed ?? []).isEmpty
        language.isEnabled = ready && !languages.isEmpty
        editor.isEditable = ready
        downloadButton.isEnabled = ready && selectedLanguage != nil && !installed
        generateButton.isEnabled = ready && installed
        copyButton.isEnabled = hasText
        saveButton.isEnabled = hasText
        if let parsed {
            applyButton.isEnabled = parsed != initial
            applyButton.title = initial.isEmpty ? "Add to Timeline" : (parsed.isEmpty ? "Remove Subtitles" : "Update Timeline")
            applyButton.setAccessibilityLabel(applyButton.title)
        } else {
            applyButton.isEnabled = false
        }
    }

    private func run(_ action: @escaping () async throws -> Void) {
        guard operation == nil else { return }
        spinner.startAnimation(nil)
        operation = Task {
            defer {
                self.operation = nil
                self.spinner.stopAnimation(nil)
                self.updateControls()
            }
            do { try await action() }
            catch {
                if !Task.isCancelled { self.showStatus(error.localizedDescription) }
            }
        }
        updateControls()
    }

    @objc private func checkLanguage() { run { try await self.refreshLanguage() } }

    private func refreshLanguage() async throws {
        guard let selectedLanguage else {
            showStatus("On-device speech recognition is unavailable on this Mac.")
            return
        }
        installed = await VideoSpeech.isInstalled(selectedLanguage)
        try Task.checkCancellation()
        showStatus(installed ? "\(selectedLanguage.name) is ready." : "Download \(selectedLanguage.name) to generate subtitles.")
    }

    @objc private func download() {
        guard let selectedLanguage else { return }
        showStatus("Downloading \(selectedLanguage.name)…")
        run {
            try await VideoSpeech.download(selectedLanguage)
            try await self.refreshLanguage()
        }
    }

    @objc private func generate() {
        guard let selectedLanguage, installed else { return }
        showStatus("Preparing timeline audio…")
        run {
            let directory = FileManager.default.temporaryDirectory
                .appendingPathComponent("Bediz-Transcription-\(UUID())", isDirectory: true)
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: false,
                                                    attributes: [.posixPermissions: 0o700])
            defer { try? FileManager.default.removeItem(at: directory) }
            let audio = directory.appendingPathComponent("audio.m4a")
            try await self.prepare(audio)
            try Task.checkCancellation()
            self.showStatus("Recognizing speech…")
            let cues = try await VideoSpeech.transcribe(audio, language: selectedLanguage, length: self.length)
            self.editor.string = VideoSubtitleCue.serialize(cues)
            self.editor.breakUndoCoalescing()
            self.showStatus("\(cues.count) subtitle cues")
        }
    }

    func textDidChange(_ notification: Notification) {
        showStatus(cues == nil ? "Check the text and times. Cues must be in order and fit the timeline." : "")
        updateControls()
    }

    @objc private func copyTranscript() {
        guard let cues, !cues.isEmpty else { return }
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(cues.map(\.text).joined(separator: "\n"), forType: .string)
        showStatus("Transcript copied.")
    }

    @objc private func saveSRT() {
        guard let cues, !cues.isEmpty else { return }
        let save = NSSavePanel()
        if let srt = UTType(filenameExtension: "srt") { save.allowedContentTypes = [srt] }
        save.nameFieldStringValue = "Subtitles.srt"
        save.beginSheetModal(for: window) { [weak self] response in
            guard response == .OK, let url = save.url else { return }
            do {
                try VideoSubtitleCue.serialize(cues).write(to: url, atomically: true, encoding: .utf8)
                self?.showStatus("Saved \(url.lastPathComponent)")
            } catch { self?.showStatus(error.localizedDescription) }
        }
    }

    @objc private func apply() {
        guard let cues else { return }
        result = cues
        close()
    }

    @objc private func close() {
        operation?.cancel()
        if window.firstResponder === editor { editor.breakUndoCoalescing() }
        window.sheetParent?.endSheet(window)
    }
}

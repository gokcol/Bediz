import AppKit

@MainActor
final class VideoExportPanel: NSObject, NSTextFieldDelegate {
    private let sheet = NSPanel()
    private let source: CGSize
    private let hasAudio: Bool
    private let preset = VideoEditUI.popup(VideoExportPreset.allCases, selected: .source, label: "Size preset")
    private let width = NSTextField()
    private let height = NSTextField()
    private let framing = VideoEditUI.popup(VideoFraming.allCases, selected: .fit, label: "Framing")
    private let container = VideoEditUI.popup(VideoContainer.allCases, selected: .mp4, label: "File format")
    private let encoder = NSPopUpButton(frame: .zero, pullsDown: false)
    private let quality = VideoEditUI.popup(VideoExportQuality.allCases, selected: .balanced, label: "Video quality")
    private let sound = NSPopUpButton(frame: .zero, pullsDown: false)
    private let rate = NSPopUpButton(frame: .zero, pullsDown: false)
    private let feedback = VideoEditUI.label("", quiet: true)
    private let framingHint = VideoEditUI.label("", quiet: true)
    private let subtitles = NSButton(checkboxWithTitle: "Burn subtitles into the video", target: nil, action: nil)
    private let audioRates = [96, 128, 192, 256, 320]
    private var result: VideoExportOptions?

    init(source: CGSize, hasAudio: Bool, hasSubtitles: Bool, previous: VideoExportOptions?) {
        self.source = source
        self.hasAudio = hasAudio
        super.init()
        subtitles.state = previous?.includeSubtitles == false ? .off : .on
        subtitles.isHidden = !hasSubtitles

        for control in [preset, framing, container, encoder, sound] {
            control.target = self
            control.action = #selector(changed(_:))
        }
        for (field, name) in [(width, "Output width"), (height, "Output height")] {
            field.setAccessibilityLabel(name)
            field.font = .monospacedDigitSystemFont(ofSize: 13, weight: .medium)
            field.alignment = .center
            field.delegate = self
            field.widthAnchor.constraint(equalToConstant: 74).isActive = true
        }
        encoder.setAccessibilityLabel("Video codec")
        sound.setAccessibilityLabel("Audio codec")
        rate.setAccessibilityLabel("AAC bitrate")
        rate.addItems(withTitles: audioRates.map { "\($0) kbps" })
        rate.selectItem(at: 2)
        let size = previous?.size ?? source
        width.stringValue = String(Int(size.width))
        height.stringValue = String(Int(size.height))
        if let previous {
            framing.selectItem(withTitle: previous.framing.rawValue)
            container.selectItem(withTitle: previous.container.rawValue)
            quality.selectItem(withTitle: previous.quality.rawValue)
            if let index = audioRates.firstIndex(of: previous.aacKbps) { rate.selectItem(at: index) }
            let profile = VideoExportPreset.allCases.first { $0.size(source: source) == size } ?? .custom
            preset.selectItem(withTitle: profile.rawValue)
        }
        updateChoices(video: previous?.video, audio: previous?.sound)
        updateFramingHint()

        sheet.styleMask = [.titled]
        sheet.title = "Export video"
        let root = NSView()
        root.appearance = NSAppearance(named: .darkAqua)
        root.wantsLayer = true
        root.layer?.backgroundColor = NSColor(srgbRed: 0.11, green: 0.14, blue: 0.20, alpha: 1).cgColor
        sheet.contentView = root

        let pixels = VideoEditUI.row([width, VideoEditUI.label("×", quiet: true), height, VideoEditUI.label("px", quiet: true)])
        let rules = NSBox()
        rules.boxType = .separator
        let cancel = VideoEditUI.button("Cancel", "xmark", self, #selector(cancelPanel), text: true)
        cancel.keyEquivalent = "\u{1b}"
        let confirm = VideoEditUI.button("Choose File…", "square.and.arrow.up", self, #selector(confirmPanel), text: true, primary: true)
        confirm.keyEquivalent = "\r"
        var rows: [NSView] = [
            row("Size preset", preset),
            row("Dimensions", pixels),
            row("Framing", framing),
            row("", framingHint),
            rules,
            row("File format", container),
            row("Video codec", encoder, quality),
            row("Audio codec", sound, rate),
        ]
        if hasSubtitles { rows.append(row("Subtitles", subtitles)) }
        rows.append(VideoEditUI.row([feedback, VideoEditUI.gap(), cancel, confirm]))
        let content = VideoEditUI.column(rows, spacing: 10)
        content.translatesAutoresizingMaskIntoConstraints = false
        root.addSubview(content)
        NSLayoutConstraint.activate([
            content.widthAnchor.constraint(equalToConstant: 456),
            content.leadingAnchor.constraint(equalTo: root.leadingAnchor, constant: 18),
            content.trailingAnchor.constraint(equalTo: root.trailingAnchor, constant: -18),
            content.topAnchor.constraint(equalTo: root.topAnchor, constant: 16),
            content.bottomAnchor.constraint(equalTo: root.bottomAnchor, constant: -16),
        ])
        sheet.setContentSize(NSSize(width: 492, height: content.fittingSize.height + 32))
    }

    func show(in window: NSWindow, completion: @escaping (VideoExportOptions?) -> Void) {
        window.beginSheet(sheet) { [self] _ in completion(result) }
    }

    private func row(_ label: String, _ control: NSView, _ secondary: NSView? = nil) -> NSStackView {
        let name = VideoEditUI.label(label)
        name.widthAnchor.constraint(equalToConstant: 90).isActive = true
        var controls = [name as NSView, control]
        if let secondary { controls.append(secondary) }
        controls.append(VideoEditUI.gap())
        return VideoEditUI.row(controls, spacing: 10)
    }

    private var format: VideoContainer { VideoContainer(rawValue: container.titleOfSelectedItem ?? "") ?? .mp4 }
    private var videoCodec: VideoEncoder { VideoEncoder(rawValue: encoder.titleOfSelectedItem ?? "") ?? .h264 }
    private var audioCodec: SoundEncoder { SoundEncoder(rawValue: sound.titleOfSelectedItem ?? "") ?? .none }
    private var frame: VideoFraming { VideoFraming(rawValue: framing.titleOfSelectedItem ?? "") ?? .fit }

    private func updateChoices(video: VideoEncoder?, audio: SoundEncoder?) {
        let selectedVideo = video ?? videoCodec
        let selectedAudio = audio ?? (sound.numberOfItems == 0 ? .aac : audioCodec)
        let validVideo = VideoEncoder.allCases.filter { $0.fits(format) }
        let validSound = hasAudio ? SoundEncoder.allCases.filter { $0.fits(format) } : [.none]
        encoder.removeAllItems()
        encoder.addItems(withTitles: validVideo.map(\.rawValue))
        encoder.selectItem(withTitle: validVideo.contains(selectedVideo) ? selectedVideo.rawValue : VideoEncoder.h264.rawValue)
        sound.removeAllItems()
        sound.addItems(withTitles: validSound.map(\.rawValue))
        sound.selectItem(withTitle: validSound.contains(selectedAudio) ? selectedAudio.rawValue : validSound[0].rawValue)
        sound.isEnabled = hasAudio
        updateBitrates()
    }

    private func updateBitrates() {
        quality.isEnabled = videoCodec != .proRes
        quality.setAccessibilityLabel("Video quality")
        rate.isEnabled = audioCodec == .aac
    }

    private func updateFramingHint() {
        framingHint.stringValue = frame == .fit ? "Adds bars when the aspect ratios differ." : "Crops the edges when the aspect ratios differ."
    }

    @objc private func changed(_ sender: NSPopUpButton) {
        feedback.stringValue = ""
        if sender === preset,
           let selected = preset.titleOfSelectedItem.flatMap(VideoExportPreset.init(rawValue:)),
           let size = selected.size(source: source) {
            width.stringValue = String(Int(size.width))
            height.stringValue = String(Int(size.height))
        }
        if sender === container { updateChoices(video: nil, audio: nil) }
        updateFramingHint()
        updateBitrates()
    }

    func controlTextDidChange(_ notification: Notification) {
        preset.selectItem(withTitle: VideoExportPreset.custom.rawValue)
        feedback.stringValue = ""
    }

    @objc private func confirmPanel() {
        guard let w = Int(width.stringValue), let h = Int(height.stringValue),
              VideoExportOptions.valid(w), VideoExportOptions.valid(h) else {
            feedback.stringValue = "Use even dimensions from 16 to 4096 px."
            return
        }
        let bitrate = audioRates[max(0, rate.indexOfSelectedItem)]
        let selected = VideoExportOptions(
            size: CGSize(width: w, height: h), container: format, video: videoCodec, sound: audioCodec, framing: frame,
            quality: VideoExportQuality(rawValue: self.quality.titleOfSelectedItem ?? "") ?? .balanced, aacKbps: bitrate,
            includeSubtitles: subtitles.state == .on
        )
        guard selected.isValid else { return }
        result = selected
        cancelPanel()
    }

    @objc private func cancelPanel() { sheet.sheetParent?.endSheet(sheet) }
}

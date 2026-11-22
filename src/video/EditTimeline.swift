import AppKit
import AVFoundation
import UniformTypeIdentifiers

typealias BedizOpenFrame = @convention(c) (UInt64, UnsafePointer<UInt8>, Int, Int64, Int64, UnsafePointer<UInt8>?, Int) -> Bool

@MainActor
private final class EditClipStrip: NSView {
    var segments: [VideoEditClip] = [] { didSet { needsDisplay = true } }
    var selection: Range<Int64>? { didSet { needsDisplay = true } }
    var time: Int64 = 0 { didSet { if oldValue != time { needsDisplay = true } } }
    var seek: ((Int64) -> Void)?
    private var duration: Int64 { segments.reduce(0) { $0 + $1.length } }

    override func draw(_ dirtyRect: NSRect) {
        let track = bounds.insetBy(dx: 3, dy: 4)
        NSColor.black.withAlphaComponent(0.32).setFill()
        NSBezierPath(roundedRect: track, xRadius: 6, yRadius: 6).fill()
        let total = duration
        guard total > 0 else { return }
        func x(_ time: Int64) -> CGFloat { track.minX + track.width * CGFloat(time) / CGFloat(total) }
        var cursor: Int64 = 0
        for clip in segments {
            let end = cursor + clip.length
            let left = x(cursor), right = x(end)
            let rect = CGRect(x: left, y: track.minY + 1, width: max(0, right - left - 1), height: track.height - 2)
            let isCurrent = time >= cursor && (time < end || end == total)
            let color: NSColor
            if clip.freeze != nil { color = NSColor(srgbRed: 0.14, green: 0.40, blue: 0.46, alpha: 1) }
            else if clip.card != nil { color = NSColor(srgbRed: 0.38, green: 0.24, blue: 0.56, alpha: 1) }
            else { color = NSColor(srgbRed: 0.21, green: 0.37, blue: 0.65, alpha: 1) }
            (isCurrent ? color.highlight(withLevel: 0.16) ?? color : color).setFill()
            NSBezierPath(roundedRect: rect, xRadius: min(4, rect.width / 2), yRadius: 4).fill()
            if rect.width >= 31 {
                let paragraph = NSMutableParagraphStyle()
                paragraph.lineBreakMode = .byTruncatingTail
                (clip.caption as NSString).draw(in: rect.insetBy(dx: 6, dy: 4), withAttributes: [
                    .font: NSFont.systemFont(ofSize: 11, weight: .semibold), .foregroundColor: NSColor.white,
                    .paragraphStyle: paragraph,
                ])
            }
            if clip.transition != .cut && cursor > 0 {
                NSColor.white.withAlphaComponent(0.60).setFill()
                NSBezierPath(ovalIn: CGRect(x: left - 3, y: track.midY - 3, width: 6, height: 6)).fill()
            }
            if !clip.overlays.isEmpty {
                VideoEditUI.accent.highlight(withLevel: 0.44)?.setFill()
                for item in clip.overlays {
                    CGRect(x: x(cursor + item.start), y: track.minY, width: x(item.end) - x(item.start), height: 2).fill()
                }
            }
            cursor = end
        }
        if let selection {
            let lower = x(selection.lowerBound), upper = x(selection.upperBound)
            NSColor(srgbRed: 0.96, green: 0.77, blue: 0.35, alpha: 0.22).setFill()
            CGRect(x: lower, y: track.minY, width: upper - lower, height: track.height).fill()
            NSColor(srgbRed: 1, green: 0.82, blue: 0.45, alpha: 1).setFill()
            for edge in [lower, upper] {
                CGRect(x: edge - 0.75, y: track.minY, width: 1.5, height: track.height).fill()
            }
        }
        NSColor.white.setFill()
        CGRect(x: min(track.maxX, max(track.minX, x(time))) - 0.75, y: bounds.minY + 1,
               width: 1.5, height: bounds.height - 2).fill()
    }

    override func mouseDown(with event: NSEvent) { movePlayhead(event) }
    override func mouseDragged(with event: NSEvent) { movePlayhead(event) }

    private func movePlayhead(_ event: NSEvent) {
        let point = convert(event.locationInWindow, from: nil)
        let fraction = min(1, max(0, (point.x - 3) / max(1, bounds.width - 6)))
        seek?(Int64((fraction * CGFloat(duration)).rounded()))
    }
}

@MainActor
final class VideoTimelineEditor: NSObject, NSTextFieldDelegate {
    private struct FrameSession {
        let id: UInt64
        let time: Int64
        let picture: Data
        let layer: UUID?
    }
    let view = NSView()
    var artworkRenderer: BedizArtworkRenderer?
    var onSeek: ((Int64) -> Void)?
    var onPreview: ((AVPlayerItem?, Int64) -> Void)?
    var onSource: ((URL, String?) -> Void)?
    var onStill: ((Int64, CGFloat) async -> CGImage?)?
    var openReference: BedizOpenFrame?
    var frameRenderer: BedizFrameRenderer?
    var onReferenceVisibility: ((Bool) -> Void)?

    private let strip = EditClipStrip()
    private let position = VideoEditUI.timeField("Playhead time", value: 0)
    private let markIn = VideoEditUI.timeField("Range start", value: 0)
    private let markOut = VideoEditUI.timeField("Range end", value: 0)
    private let notice = VideoEditUI.label("", quiet: true)
    private let clips = NSPopUpButton(frame: .zero, pullsDown: false)
    private let annotations = NSPopUpButton(frame: .zero, pullsDown: false)
    private lazy var undoButton = VideoEditUI.button("Undo", "arrow.uturn.backward", self, #selector(undoEdit))
    private lazy var redoButton = VideoEditUI.button("Redo", "arrow.uturn.forward", self, #selector(redoEdit))
    private lazy var pasteButton = VideoEditUI.button("Paste range at playhead", "doc.on.clipboard", self, #selector(pasteRange))
    private lazy var settingsButton = VideoEditUI.button("Edit clip or page", "slider.horizontal.3", self, #selector(clipSettings))
    private lazy var freezeButton = VideoEditUI.button("Freeze frame…", "snowflake", self, #selector(freezeFrame))
    private lazy var annotateFrameButton = VideoEditUI.button("Annotate This Frame", "pencil.and.outline", self, #selector(annotateFrame), text: true)
    private lazy var subtitleButton = VideoEditUI.button("Auto Subtitles…", "captions.bubble", self, #selector(autoSubtitles))
    private lazy var artButton = VideoEditUI.button("Add annotation", "pencil.tip.crop.circle.badge.plus", self, #selector(addArt), text: true)
    private lazy var editArtButton = VideoEditUI.button("Edit annotation", "pencil", self, #selector(editArt))
    private lazy var deleteArtButton = VideoEditUI.button("Remove annotation", "minus.circle", self, #selector(deleteArt))
    private lazy var exportButton = VideoEditUI.button("Export video…", "square.and.arrow.up", self, #selector(exportMovie), text: true, primary: true)

    private(set) var project: VideoEditProject?
    private var media: VideoEditMedia?
    private var file: URL?
    private var saved: VideoEditProject?
    private var undoStack: [VideoEditProject] = []
    private var redoStack: [VideoEditProject] = []
    private var clipboard: [VideoEditClip] = []
    private var playhead: Int64 = 0
    private var displayedClip: Int?
    private var refreshTask: Task<Void, Never>?
    private var loadTask: Task<Void, Never>?
    private var designer: VideoDesignPanel?
    private var startingDesign = false
    private var exporting = false
    private var lastExport: VideoExportOptions?
    private var nextReference: UInt64 = 0
    private var reference: FrameSession?
    private(set) var isPreviewCurrent = true

    var pendingChanges: Bool { project != saved }
    var length: Int64? { project?.length }
    var hasFrameReference: Bool { reference != nil || openingReference }
    private var openingReference = false

    override init() {
        super.init()
        view.appearance = NSAppearance(named: .darkAqua)
        view.wantsLayer = true
        view.layer?.backgroundColor = NSColor(srgbRed: 0.12, green: 0.15, blue: 0.22, alpha: 1).cgColor
        let heading = NSTextField(labelWithString: "Timeline")
        heading.font = .systemFont(ofSize: 14, weight: .semibold)
        let projectMenu = menuButton("Video project", "folder", choices: [
            ("Open Video Project…", #selector(openProjectDialog)),
            ("Save Video Project…", #selector(saveProjectDialog)),
        ])
        let pages = menuButton("Add page", "rectangle.badge.plus", text: true, choices: [
            ("At beginning…", #selector(pageAtStart)),
            ("At playhead…", #selector(pageAtPlayhead)),
            ("After selected clip…", #selector(pageAfterClip)),
            ("At end…", #selector(pageAtEnd)),
        ])
        strip.heightAnchor.constraint(equalToConstant: 33).isActive = true
        strip.setAccessibilityLabel("Clip timeline")
        strip.seek = { [weak self] time in self?.seek(to: time) }
        notice.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        let header = VideoEditUI.row([heading, notice, VideoEditUI.gap(), annotateFrameButton, subtitleButton, undoButton, redoButton, projectMenu, exportButton])
        for field in [position, markIn, markOut] { field.delegate = self }
        position.target = self
        position.action = #selector(goToTime)
        clips.setAccessibilityLabel("Select clip")
        clips.target = self
        clips.action = #selector(jumpToClip)
        annotations.setAccessibilityLabel("Annotations in selected clip")
        annotations.target = self
        annotations.action = #selector(selectAnnotation)
        for popup in [clips, annotations] {
            popup.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
            popup.widthAnchor.constraint(greaterThanOrEqualToConstant: 106).isActive = true
            let width = popup.widthAnchor.constraint(equalToConstant: 140)
            width.priority = NSLayoutConstraint.Priority(rawValue: NSLayoutConstraint.Priority.windowSizeStayPut.rawValue - 1)
            width.isActive = true
        }
        let timeRow = VideoEditUI.row([
            VideoEditUI.label("Playhead"), position,
            VideoEditUI.button("Go to time", "scope", self, #selector(goToTime)),
            VideoEditUI.button("Back 1 ms", "minus", self, #selector(backOne)),
            VideoEditUI.button("Forward 1 ms", "plus", self, #selector(forwardOne)),
            VideoEditUI.button("Split at playhead", "scissors", self, #selector(splitClip), text: true),
            freezeButton,
            VideoEditUI.gap(), VideoEditUI.label("Range", quiet: true),
            VideoEditUI.button("Mark range start", "arrow.right.to.line", self, #selector(markStart)), markIn,
            VideoEditUI.button("Mark range end", "arrow.left.to.line", self, #selector(markEnd)), markOut,
        ], spacing: 5)
        let rangeRow = VideoEditUI.row([
            VideoEditUI.button("Cut range", "scissors", self, #selector(cutRange)),
            VideoEditUI.button("Copy range", "doc.on.doc", self, #selector(copyRange)),
            pasteButton,
            VideoEditUI.button("Delete range", "trash", self, #selector(deleteRange)),
            VideoEditUI.gap(), clips, settingsButton, pages,
            artButton, annotations, editArtButton, deleteArtButton,
        ], spacing: 5)
        let content = VideoEditUI.column([header, strip, timeRow, rangeRow], spacing: 6)
        content.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(content)
        NSLayoutConstraint.activate([
            content.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 14),
            content.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -14),
            content.topAnchor.constraint(equalTo: view.topAnchor, constant: 7),
            content.bottomAnchor.constraint(equalTo: view.bottomAnchor, constant: -8),
        ])
        updateButtons()
    }

    private func menuButton(_ title: String, _ icon: String, text: Bool = false,
                            choices: [(String, Selector)]) -> NSButton {
        let button = VideoEditUI.button(title, icon, self, #selector(showMenu(_:)), text: text)
        let menu = NSMenu()
        for (title, action) in choices {
            let item = NSMenuItem(title: title, action: action, keyEquivalent: "")
            item.target = self
            menu.addItem(item)
        }
        button.menu = menu
        return button
    }

    @objc private func showMenu(_ sender: NSButton) {
        sender.menu?.popUp(positioning: nil, at: NSPoint(x: 0, y: sender.bounds.maxY + 3), in: sender)
    }

    func reset() {
        loadTask?.cancel()
        refreshTask?.cancel()
        isPreviewCurrent = true
        project = nil
        saved = nil
        media = nil
        file = nil
        lastExport = nil
        clipboard = []
        undoStack = []
        redoStack = []
        playhead = 0
        strip.time = 0
        position.stringValue = VideoTime.format(0)
        updateProjectUI()
    }

    func start(source: URL, time: Int64) {
        guard project == nil else { return }
        load(url: source, projectURL: nil, at: time)
    }

    func loadProject(url: URL) {
        guard !hasFrameReference else { return }
        load(url: url, projectURL: url, at: 0)
    }

    private func load(url: URL, projectURL: URL?, at time: Int64) {
        loadTask?.cancel()
        notice.stringValue = "Loading media…"
        loadTask = Task {
            do {
                let restored: VideoEditProject?
                if let projectURL {
                    let size = try projectURL.resourceValues(forKeys: [.fileSizeKey]).fileSize ?? 0
                    guard size > 0, size <= 128 * 1_024 * 1_024 else { throw VideoEditError.invalidProject }
                    restored = try JSONDecoder().decode(VideoEditProject.self, from: Data(contentsOf: projectURL))
                } else {
                    restored = nil
                }
                let source = restored?.source ?? url
                let media = try await VideoEditMedia.load(source)
                let loaded = restored ?? VideoEditProject(source: source, duration: media.duration)
                try loaded.validate(sourceDuration: media.duration)
                try Task.checkCancellation()
                refreshTask?.cancel()
                self.media = media
                project = loaded
                saved = loaded
                file = projectURL
                lastExport = nil
                undoStack = []
                redoStack = []
                clipboard = []
                setTime(time)
                markIn.stringValue = VideoTime.format(0)
                markOut.stringValue = VideoTime.format(min(loaded.length, time))
                updateProjectUI()
                onSource?(source, projectURL?.deletingPathExtension().lastPathComponent)
                if restored != nil { rebuildPreview() }
                else { notice.stringValue = VideoTime.format(loaded.length) }
            } catch is CancellationError {
                return
            } catch {
                if !Task.isCancelled { show(error.localizedDescription) }
            }
        }
    }

    func allowClose() -> Bool {
        if view.window?.attachedSheet != nil { return false }
        if hasFrameReference {
            show("Send or cancel the frame annotations before closing the video.")
            return false
        }
        if exporting {
            show("Wait for the current video export to finish.")
            return false
        }
        guard pendingChanges else { return true }
        let alert = NSAlert()
        alert.messageText = "Save this video project?"
        alert.informativeText = "Save your timeline to reopen its clips, pages, transitions, and annotations."
        alert.addButton(withTitle: "Save")
        alert.addButton(withTitle: "Cancel").keyEquivalent = "\u{1b}"
        alert.addButton(withTitle: "Discard").keyEquivalent = ""
        switch alert.runModal() {
        case .alertFirstButtonReturn: return saveProject()
        case .alertThirdButtonReturn: return true
        default: return false
        }
    }

    func updatePlaybackTime(_ time: Int64) {
        guard project != nil else { return }
        setTime(time)
    }

    private func setTime(_ time: Int64) {
        playhead = min(max(0, time), project?.length ?? 0)
        strip.time = playhead
        if position.currentEditor() == nil { position.stringValue = VideoTime.format(playhead) }
        if let index = project?.location(at: playhead)?.index, index != displayedClip {
            displayedClip = index
            if index < clips.numberOfItems { clips.selectItem(at: index) }
            updateLayers()
        }
    }

    private func seek(to time: Int64) {
        setTime(time)
        position.stringValue = VideoTime.format(playhead)
        onSeek?(playhead)
    }

    @objc private func goToTime() {
        guard let time = VideoTime.parse(position.stringValue) else { return show("Enter a time such as 00:00:03.250.") }
        seek(to: time)
    }
    @objc private func backOne() { seek(to: playhead - 1) }
    @objc private func forwardOne() { seek(to: playhead + 1) }
    @objc private func markStart() {
        markIn.stringValue = VideoTime.format(playhead)
        updateRange()
    }
    @objc private func markEnd() {
        markOut.stringValue = VideoTime.format(playhead)
        updateRange()
    }

    func controlTextDidChange(_ obj: Notification) { updateRange() }

    private var range: Range<Int64>? {
        guard let project, let start = VideoTime.parse(markIn.stringValue),
              let end = VideoTime.parse(markOut.stringValue), start < end, end <= project.length else { return nil }
        return start..<end
    }

    private func updateRange() { strip.selection = range }

    @objc private func splitClip() {
        guard var next = project, next.split(at: playhead) else {
            return show("Move the playhead inside a clip to split it.")
        }
        commit(next, at: playhead)
    }

    @objc private func freezeFrame() {
        guard let project, project.length > 0, isPreviewCurrent,
              let window = view.window, window.attachedSheet == nil, !startingDesign else { return }
        let time = playhead
        onSeek?(time)
        let duration = VideoEditUI.timeField("Freeze frame duration", value: 3_000)
        let field = VideoEditUI.row([VideoEditUI.label("Duration"), duration])
        field.setFrameSize(field.fittingSize)
        let alert = NSAlert()
        alert.messageText = "Freeze frame"
        alert.informativeText = "Hold the picture at \(VideoTime.format(time)) before the video continues."
        alert.accessoryView = field
        alert.addButton(withTitle: "Insert")
        alert.addButton(withTitle: "Cancel").keyEquivalent = "\u{1b}"
        alert.beginSheetModal(for: window) { [weak self] response in
            guard let self, response == .alertFirstButtonReturn else { return }
            guard let length = VideoTime.parse(duration.stringValue), length > 0, length <= VideoTime.maximum - project.length else {
                return self.show("Enter a freeze duration that fits the timeline.")
            }
            self.startingDesign = true
            Task { [self] in
                defer { self.startingDesign = false }
                let picture = await self.onStill?(time, 4096)
                guard self.project == project else { return }
                guard let picture, let image = VideoDesign.png(picture), image.count <= 32 * 1_024 * 1_024 else {
                    return self.show("This frame couldn’t be saved as a still.")
                }
                var updated = project
                guard updated.insert([.init(length: length, freeze: image)], at: time) else { return }
                self.commit(updated, at: time)
            }
        }
    }

    @objc private func annotateFrame() {
        guard let project, project.length > 0 else { return }
        openFrame(at: min(playhead, project.length - 1), replacing: nil)
    }

    private func openFrame(at time: Int64, replacing layer: VideoOverlay?) {
        guard let project, let media, let openReference, project.length > 0, isPreviewCurrent,
              let window = view.window, window.attachedSheet == nil, !startingDesign, !hasFrameReference else { return }
        let objects = layer?.art.editableFrameObjects
        let duration = layer.map { project.overlayDuration($0.id) } ?? (project.length - time)
        guard duration > 0, layer == nil || objects != nil else { return }
        openingReference = true
        onSeek?(time)
        updateButtons()
        show("Opening frame in Bediz…")
        Task { [self] in
            defer {
                openingReference = false
                updateButtons()
            }
            let picture: Data?
            if let saved = layer?.art.frameReference?.picture {
                picture = saved
            } else {
                let frame: CGImage?
                if let layer {
                    // Earlier effect layers retained their objects. Exclude that layer from the reference.
                    var clean = project
                    _ = clean.removeOverlay(layer.id)
                    do {
                        let rendered = try await VideoEditRender.build(project: clean, media: media, frameRenderer: frameRenderer)
                        frame = try await rendered.still(at: time)
                    } catch {
                        return show(error.localizedDescription)
                    }
                } else {
                    frame = await onStill?(time, 4096)
                }
                picture = frame.flatMap { VideoDesign.resize($0, to: media.size) }.flatMap(VideoDesign.png)
            }
            guard self.project == project, view.window === window, window.attachedSheet == nil, !startingDesign else { return }
            guard let picture, !picture.isEmpty, picture.count <= 32 * 1_024 * 1_024 else {
                return show("This frame couldn’t be opened in the editor.")
            }
            nextReference &+= 1
            let id = nextReference
            let accepted = picture.withUnsafeBytes { buffer in
                guard let bytes = buffer.bindMemory(to: UInt8.self).baseAddress else { return false }
                if let objects {
                    return objects.withUnsafeBytes { data in
                        openReference(id, bytes, buffer.count, time, duration, data.bindMemory(to: UInt8.self).baseAddress, data.count)
                    }
                }
                return openReference(id, bytes, buffer.count, time, duration, nil, 0)
            }
            guard accepted else { return show("The image editor couldn’t open this video frame.") }
            reference = FrameSession(id: id, time: time, picture: picture, layer: layer?.id)
            onReferenceVisibility?(true)
        }
    }

    func receiveReference(id: UInt64, bytes: Data?, duration: Int64, usesEffects: Bool, objects: Data?) -> Bool {
        guard let reference, reference.id == id, var next = project else { return false }
        if let bytes {
            let kind: VideoArtworkKind = usesEffects ? .frameEffects : .image
            let saved = VideoFrameReference(picture: reference.picture, objects: objects)
            let art = VideoArtwork(kind: kind, text: "Frame annotations", image: bytes, x: 0, y: 0, width: 1, frameReference: saved)
            if let layer = reference.layer {
                guard next.replaceOverlay(layer, art: art) else { return false }
            } else {
                guard duration > 0, duration <= next.length - reference.time,
                      next.addOverlay(art, over: reference.time..<(reference.time + duration)) else { return false }
            }
            commit(next, at: reference.time)
            if let layer = reference.layer, let index = displayedClip,
               let position = next.clips[index].overlays.firstIndex(where: { $0.id == layer }) {
                annotations.selectItem(at: position)
            }
        }
        self.reference = nil
        updateButtons()
        show(bytes == nil ? VideoTime.format(next.length)
                          : reference.layer == nil ? "Frame annotations added." : "Frame annotations updated.")
        onReferenceVisibility?(false)
        return true
    }

    @objc private func cutRange() { removeRange(copy: true) }
    @objc private func deleteRange() { removeRange(copy: false) }

    private func removeRange(copy: Bool) {
        guard var next = project, let range else { return show("Set a range start and end first.") }
        if copy { clipboard = next.portion(range) }
        guard next.remove(range) else { return }
        markIn.stringValue = VideoTime.format(range.lowerBound)
        markOut.stringValue = markIn.stringValue
        commit(next, at: range.lowerBound)
    }

    @objc private func copyRange() {
        guard let project, let range else { return show("Set a range start and end first.") }
        clipboard = project.portion(range)
        updateButtons()
        show("Range copied. Paste it at any time.")
    }

    @objc private func pasteRange() {
        guard var next = project, next.insert(clipboard, at: playhead) else { return show("No range is available to paste.") }
        commit(next, at: playhead)
    }

    private func commit(_ next: VideoEditProject, at time: Int64) {
        guard let project, project != next else { return }
        undoStack.append(project)
        if undoStack.count > 32 { undoStack.removeFirst() }
        redoStack.removeAll()
        display(next, at: time)
    }

    private func display(_ next: VideoEditProject, at time: Int64) {
        onSeek?(playhead)
        project = next
        setTime(time)
        updateProjectUI()
        rebuildPreview()
    }

    @objc private func undoEdit() {
        guard let project, let old = undoStack.popLast() else { return }
        redoStack.append(project)
        display(old, at: playhead)
    }

    @objc private func redoEdit() {
        guard let project, let next = redoStack.popLast() else { return }
        undoStack.append(project)
        display(next, at: playhead)
    }

    private func rebuildPreview() {
        refreshTask?.cancel()
        guard let project, let media else { return }
        isPreviewCurrent = false
        updateButtons()
        guard project.length > 0 else {
            onPreview?(nil, 0)
            return show("Timeline empty. Paste a range or add a page.")
        }
        show("Updating preview…")
        refreshTask = Task {
            do {
                let render = try await VideoEditRender.build(project: project, media: media, frameRenderer: frameRenderer)
                try Task.checkCancellation()
                isPreviewCurrent = true
                onPreview?(render.playerItem(), playhead)
                updateButtons()
                show(VideoTime.format(project.length))
            } catch is CancellationError {
                return
            } catch {
                if !Task.isCancelled { show(error.localizedDescription) }
            }
        }
    }

    private func updateProjectUI() {
        let items = project?.clips ?? []
        strip.segments = items
        clips.removeAllItems()
        for (offset, clip) in items.enumerated() {
            clips.addItem(withTitle: "\(offset + 1)  \(clip.caption)")
        }
        displayedClip = project?.location(at: playhead)?.index
        if let displayedClip { clips.selectItem(at: displayedClip) }
        clips.isEnabled = !items.isEmpty
        updateRange()
        updateLayers()
        updateButtons()
        if project == nil { notice.stringValue = "" }
    }

    private func updateLayers() {
        annotations.removeAllItems()
        if let displayedClip, let clip = project?.clips[displayedClip] {
            for (index, item) in clip.overlays.enumerated() {
                let text = item.art.text.trimmingCharacters(in: .whitespacesAndNewlines)
                let name = text.isEmpty ? item.art.kind.rawValue : String(text.prefix(48))
                annotations.addItem(withTitle: "\(index + 1)  \(name)")
            }
        }
        if annotations.numberOfItems == 0 { annotations.addItem(withTitle: "No annotations") }
        selectAnnotation()
    }

    @objc private func selectAnnotation() {
        let enabled = displayedClip.flatMap { project?.clips[$0].overlays.isEmpty } == false
        annotations.isEnabled = enabled
        editArtButton.isEnabled = enabled
        deleteArtButton.isEnabled = enabled
    }

    private func updateButtons() {
        undoButton.isEnabled = !undoStack.isEmpty
        redoButton.isEnabled = !redoStack.isEmpty
        pasteButton.isEnabled = !clipboard.isEmpty
        settingsButton.isEnabled = displayedClip != nil
        freezeButton.isEnabled = isPreviewCurrent && (project?.length ?? 0) > 0
        annotateFrameButton.isEnabled = openReference != nil && !hasFrameReference && isPreviewCurrent && (project?.length ?? 0) > 0
        artButton.isEnabled = (project?.length ?? 0) > 0
        subtitleButton.isEnabled = !hasFrameReference && (project?.length ?? 0) > 0
        exportButton.isEnabled = !exporting && (project?.length ?? 0) > 0
    }

    @objc private func jumpToClip() {
        guard let entries = project?.entries(), entries.indices.contains(clips.indexOfSelectedItem) else { return }
        seek(to: entries[clips.indexOfSelectedItem].offset)
    }

    @objc private func pageAtStart() { addPage(at: 0, style: .title) }
    @objc private func pageAtPlayhead() { addPage(at: playhead, style: .brief) }
    @objc private func pageAtEnd() { addPage(at: project?.length ?? 0, style: .end) }
    @objc private func pageAfterClip() {
        guard let project, let index = displayedClip else { return }
        let entry = project.entries()[index]
        addPage(at: entry.offset + entry.clip.length, style: .chapter)
    }

    private func addPage(at time: Int64, style: VideoCardStyle) {
        presentDesign(.page(.init(style: style), length: 3_000)) { [weak self] design in
            guard let self, var next = self.project, case .page(let page, let length) = design,
                  next.insert([.init(length: length, card: page)], at: time) else { return "This page could not be inserted." }
            self.commit(next, at: time)
            return nil
        }
    }

    @objc private func addArt() {
        guard let project, project.length > 0 else { return show("Add a video clip or page first.") }
        let start = min(playhead, project.length - 1)
        presentDesign(.overlay(.init(kind: .text), start: start, length: min(3_000, project.length - start))) { [weak self] design in
            guard let self, var next = self.project, case .overlay(let art, let offset, let length) = design,
                  next.addOverlay(art, over: offset..<(offset + length)) else { return "Keep the annotation within the timeline." }
            self.commit(next, at: offset)
            return nil
        }
    }

    private func selectedAnnotation() -> VideoOverlay? {
        guard let project, let index = displayedClip else { return nil }
        let entry = project.entries()[index]
        let chosen = annotations.indexOfSelectedItem
        guard entry.clip.overlays.indices.contains(chosen) else { return nil }
        return entry.clip.overlays[chosen]
    }

    @objc private func editArt() {
        guard let item = selectedAnnotation(), let span = project?.overlayBounds(item.id) else { return }
        if item.art.editableFrameObjects != nil {
            openFrame(at: span.lowerBound, replacing: item)
            return
        }
        presentDesign(.overlay(item.art, start: span.lowerBound, length: span.upperBound - span.lowerBound)) { [weak self] design in
            guard let self, var next = self.project, case .overlay(let art, let start, let length) = design,
                  next.editOverlay(item.id, art: art, over: start..<(start + length))
            else { return "Keep the annotation within the timeline." }
            self.commit(next, at: start)
            return nil
        }
    }

    @objc private func deleteArt() {
        guard var next = project, let item = selectedAnnotation(), next.removeOverlay(item.id) else { return }
        commit(next, at: playhead)
    }

    private func presentDesign(_ design: VideoDesignPanel.Design, apply: @escaping (VideoDesignPanel.Design) -> String?) {
        guard let media, let window = view.window, window.attachedSheet == nil,
              designer == nil, !startingDesign else { return }
        startingDesign = true
        onSeek?(playhead)
        Task { [self] in
            let still: CGImage?
            if case .overlay(_, let start, _) = design { still = await onStill?(start, 900) } else { still = nil }
            startingDesign = false
            guard view.window === window, window.attachedSheet == nil, project != nil else { return }
            let panel = VideoDesignPanel(design: design, canvas: media.size, still: still, artworkRenderer: artworkRenderer,
                                         endpointStill: { [weak self] time in await self?.onStill?(time, 900) }, apply: apply)
            designer = panel
            panel.show(in: window) { [weak self] in self?.designer = nil }
        }
    }

    @objc private func clipSettings() {
        guard let project, let index = displayedClip, let window = view.window else { return }
        let entry = project.entries()[index]
        let alert = NSAlert()
        alert.messageText = entry.clip.freeze != nil ? "Freeze frame" : (entry.clip.isStill ? "Page settings" : "Video clip")
        alert.informativeText = VideoTime.format(entry.offset) + "  to  " + VideoTime.format(entry.offset + entry.clip.length)
        let timing = VideoEditUI.timeField("Transition duration", value: entry.clip.transitionLength)
        let transition = VideoEditUI.popup(VideoTransition.allCases, selected: entry.clip.transition, label: "Transition into clip")
        transition.isEnabled = index > 0
        timing.isEnabled = index > 0
        let transitionRow = VideoEditUI.row([VideoEditUI.label("Transition in"), transition, timing])
        let hold = VideoEditUI.timeField("Freeze frame duration", value: entry.clip.length)
        let clip = entry.clip
        let fade = clip.audioFades
        let entryFade = min(clip.length, max(0, (fade?.fadeIn ?? 0) - (fade?.offset ?? 0)))
        let exitFade = fade.map { min(clip.length, max(0, clip.length - ($0.length - $0.fadeOut - $0.offset))) } ?? 0
        let fadeIn = VideoEditUI.timeField("Audio fade in", value: entryFade)
        let fadeOut = VideoEditUI.timeField("Audio fade out", value: exitFade)
        let hasAudio = !clip.isStill && !(media?.audio.isEmpty ?? true)
        let accessory: NSView
        if clip.freeze != nil {
            accessory = VideoEditUI.column([transitionRow, VideoEditUI.row([VideoEditUI.label("Duration"), hold])])
        } else if hasAudio {
            accessory = VideoEditUI.column([transitionRow, VideoEditUI.row([
                VideoEditUI.label("Fade in"), fadeIn, VideoEditUI.label("Fade out"), fadeOut,
            ])])
        } else if !clip.isStill {
            accessory = VideoEditUI.column([transitionRow, VideoEditUI.label("This recording has no audio.", quiet: true)])
        } else {
            accessory = transitionRow
        }
        accessory.setFrameSize(accessory.fittingSize)
        alert.accessoryView = accessory
        alert.addButton(withTitle: "Apply")
        alert.addButton(withTitle: "Cancel")
        if entry.clip.card != nil { alert.addButton(withTitle: "Edit Page…") }
        alert.beginSheetModal(for: window) { [weak self] response in
            guard let self else { return }
            switch response {
            case .alertFirstButtonReturn:
                guard let value = VideoTime.parse(timing.stringValue), (1...5_000).contains(value),
                      let selected = transition.titleOfSelectedItem.flatMap(VideoTransition.init(rawValue:)) else {
                    return self.show("Enter a transition duration between 0.001 and 5 seconds.")
                }
                var updated = entry.clip
                updated.transition = selected
                updated.transitionLength = value
                if updated.freeze != nil {
                    guard let duration = VideoTime.parse(hold.stringValue), duration > 0 else {
                        return self.show("Enter a duration greater than zero.")
                    }
                    updated.resizeStill(to: duration)
                } else if hasAudio {
                    guard let rise = VideoTime.parse(fadeIn.stringValue), let fall = VideoTime.parse(fadeOut.stringValue),
                          rise <= updated.length, fall <= updated.length - rise else {
                        return self.show("The combined audio fades must fit within the clip.")
                    }
                    if rise != entryFade || fall != exitFade {
                        updated.audioFades = rise + fall > 0 ? VideoAudioFades(fadeIn: rise, fadeOut: fall, length: updated.length) : nil
                    }
                }
                guard var next = self.project, next.setClip(index, updated) else { return }
                self.commit(next, at: entry.offset)
            case .alertThirdButtonReturn:
                guard let card = entry.clip.card else { return }
                self.presentDesign(.page(card, length: entry.clip.length)) { [weak self] design in
                    guard let self, var next = self.project, case .page(let page, let duration) = design else { return nil }
                    var updated = entry.clip
                    updated.card = page
                    updated.resizeStill(to: duration)
                    guard next.setClip(index, updated) else { return "This duration is too long." }
                    self.commit(next, at: entry.offset)
                    return nil
                }
            default: break
            }
        }
    }

    @objc private func openProjectDialog() {
        guard allowClose() else { return }
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [Self.projectType]
        panel.allowsMultipleSelection = false
        if panel.runModal() == .OK, let url = panel.url { loadProject(url: url) }
    }

    @objc private func saveProjectDialog() { _ = saveProject() }

    private static var projectType: UTType { UTType(filenameExtension: "bedizvideo") ?? .data }

    private func saveProject() -> Bool {
        guard let project, !project.clips.isEmpty else {
            show("Add a clip or page before saving the project.")
            return false
        }
        let url: URL
        if let file {
            url = file
        } else {
            let panel = NSSavePanel()
            panel.allowedContentTypes = [Self.projectType]
            panel.nameFieldStringValue = project.source.deletingPathExtension().lastPathComponent + ".bedizvideo"
            guard panel.runModal() == .OK, let destination = panel.url else { return false }
            url = destination
        }
        do {
            guard url.pathExtension.lowercased() == "bedizvideo" else { throw VideoEditError.invalidProject }
            let data = try JSONEncoder().encode(project)
            guard data.count <= 128 * 1_024 * 1_024 else { throw VideoEditError.invalidProject }
            try data.write(to: url, options: .atomic)
            file = url
            saved = project
            onSource?(project.source, url.deletingPathExtension().lastPathComponent)
            show("Saved \(url.lastPathComponent)")
            return true
        } catch {
            show(error.localizedDescription)
            return false
        }
    }

    @objc private func autoSubtitles() {
        guard let project, let media, let window = view.window, window.attachedSheet == nil,
              !hasFrameReference, project.length > 0 else { return }
        guard #available(macOS 26, *) else {
            return show("Auto Subtitles requires macOS 26 or later.")
        }
        let frameRenderer = self.frameRenderer
        let panel = VideoSubtitlePanel(length: project.length, existing: project.subtitles) { url in
            let render = try await VideoEditRender.build(project: project, media: media, frameRenderer: frameRenderer)
            try await VideoSpeech.audio(from: render, length: project.length, to: url)
        }
        panel.show(in: window) { [weak self] cues in
            guard let self, let cues, self.project == project else { return }
            var updated = project
            if updated.replaceSubtitles(cues) { self.commit(updated, at: self.playhead) }
        }
    }

    @objc private func exportMovie() {
        guard let media, let window = view.window, window.attachedSheet == nil, !exporting else { return }
        let settings = VideoExportPanel(source: media.size, hasAudio: !media.audio.isEmpty,
                                        hasSubtitles: project?.hasSubtitles == true, previous: lastExport)
        settings.show(in: window) { [weak self] options in
            guard let options else { return }
            self?.chooseExportDestination(options)
        }
    }

    private func chooseExportDestination(_ options: VideoExportOptions) {
        guard let project, let media else { return }
        let dialog = NSSavePanel()
        let suffix = options.container.suffix
        dialog.allowedContentTypes = [options.container.contentType]
        dialog.nameFieldStringValue = (file ?? project.source).deletingPathExtension().lastPathComponent + " — Edited." + suffix
        guard dialog.runModal() == .OK, let destination = dialog.url else { return }
        guard destination.pathExtension.lowercased() == suffix else {
            return show("Use the .\(suffix) extension for this export.")
        }
        guard destination.resolvingSymlinksInPath().standardizedFileURL != project.source.resolvingSymlinksInPath().standardizedFileURL
        else { return show("Choose a different name to keep the source recording.") }
        exporting = true
        lastExport = options
        updateButtons()
        show("Exporting \(options.container.rawValue)…")
        Task {
            let fm = FileManager.default
            let staging = destination.deletingLastPathComponent().appendingPathComponent(".bediz-\(UUID()).\(suffix)")
            defer {
                try? fm.removeItem(at: staging)
                exporting = false
                updateButtons()
            }
            do {
                let render = try await VideoEditRender.build(project: project, media: media, frameRenderer: frameRenderer, export: options)
                try await render.export(to: staging, options: options)
                if fm.fileExists(atPath: destination.path) {
                    _ = try fm.replaceItemAt(destination, withItemAt: staging)
                } else {
                    try fm.moveItem(at: staging, to: destination)
                }
                show("Exported \(destination.lastPathComponent)")
            } catch {
                show(error.localizedDescription)
            }
        }
    }

    private func show(_ message: String) {
        notice.stringValue = message
        notice.toolTip = message
    }
}

import AppKit
import AVKit
import AVFoundation

private enum PlayerPalette {
    static let accent = NSColor(srgbRed: 0.34, green: 0.48, blue: 0.99, alpha: 1)
    static let foreground = NSColor(srgbRed: 0.94, green: 0.96, blue: 1, alpha: 1)
    static let secondary = NSColor(srgbRed: 0.64, green: 0.70, blue: 0.82, alpha: 1)
    static let divider = NSColor.white.withAlphaComponent(0.10)
}

@MainActor
private final class PlaybackView: AVPlayerView {
    var playbackKey: ((NSEvent) -> Bool)?

    override var acceptsFirstResponder: Bool { true }

    override func keyDown(with event: NSEvent) {
        if playbackKey?(event) != true { super.keyDown(with: event) }
    }
}

private final class TransportDeck: NSView {
    override func makeBackingLayer() -> CALayer {
        let layer = CAGradientLayer()
        layer.colors = [
            NSColor(srgbRed: 0.13, green: 0.16, blue: 0.23, alpha: 1).cgColor,
            NSColor(srgbRed: 0.074, green: 0.091, blue: 0.14, alpha: 1).cgColor,
        ]
        layer.startPoint = CGPoint(x: 0.5, y: 1)
        layer.endPoint = CGPoint(x: 0.5, y: 0)
        return layer
    }
}

private final class TransportButton: NSButton {
    private let primary: Bool
    private var hoverArea: NSTrackingArea?
    private var isHovered = false

    init(primary: Bool) {
        self.primary = primary
        super.init(frame: .zero)
        isBordered = false
        imagePosition = .imageOnly
        symbolConfiguration = .init(pointSize: primary ? 18 : 13, weight: .semibold)
        contentTintColor = primary ? .white : PlayerPalette.foreground
        focusRingType = .exterior
        wantsLayer = true
        layer?.cornerRadius = primary ? 21 : 8
        layer?.borderWidth = primary ? 1 : 0
        layer?.borderColor = NSColor.white.withAlphaComponent(0.22).cgColor
        widthAnchor.constraint(equalToConstant: primary ? 42 : 33).isActive = true
        heightAnchor.constraint(equalToConstant: primary ? 42 : 32).isActive = true
        updateSurface()
    }

    required init?(coder: NSCoder) { nil }

    override var alignmentRectInsets: NSEdgeInsets { NSEdgeInsets() }

    override var isEnabled: Bool {
        didSet {
            if oldValue != isEnabled { updateSurface() }
        }
    }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        if let hoverArea { removeTrackingArea(hoverArea) }
        let area = NSTrackingArea(
            rect: .zero, options: [.mouseEnteredAndExited, .activeInKeyWindow, .inVisibleRect],
            owner: self, userInfo: nil
        )
        addTrackingArea(area)
        hoverArea = area
    }

    override func mouseEntered(with event: NSEvent) {
        isHovered = true
        updateSurface()
    }

    override func mouseExited(with event: NSEvent) {
        isHovered = false
        updateSurface()
    }

    override func highlight(_ flag: Bool) {
        super.highlight(flag)
        updateSurface(pressed: flag)
    }

    private func updateSurface(pressed: Bool = false) {
        guard primary else {
            let opacity = isEnabled ? (pressed ? 0.16 : isHovered ? 0.10 : 0) : 0
            layer?.backgroundColor = NSColor.white.withAlphaComponent(opacity).cgColor
            return
        }
        let accent = PlayerPalette.accent
        let background: NSColor
        if !isEnabled {
            background = accent.withAlphaComponent(0.30)
        } else if pressed {
            background = accent.shadow(withLevel: 0.22) ?? accent
        } else if isHovered {
            background = accent.highlight(withLevel: 0.16) ?? accent
        } else {
            background = accent
        }
        layer?.backgroundColor = background.cgColor
    }
}

private final class TimelineCell: NSSliderCell {
    override func drawBar(inside rect: NSRect, flipped: Bool) {
        let track = NSRect(x: rect.minX, y: rect.midY - 2.5, width: rect.width, height: 5)
        NSColor.white.withAlphaComponent(0.16).setFill()
        NSBezierPath(roundedRect: track, xRadius: 2.5, yRadius: 2.5).fill()

        if isEnabled, doubleValue > minValue {
            let thumbX = knobRect(flipped: flipped).midX
            let played = NSRect(x: track.minX, y: track.minY,
                                width: min(max(thumbX - track.minX, 0), track.width), height: track.height)
            PlayerPalette.accent.setFill()
            NSBezierPath(roundedRect: played, xRadius: 2.5, yRadius: 2.5).fill()
        }
    }

    override func drawKnob(_ knobRect: NSRect) {
        let thumb = NSRect(x: knobRect.midX - 5.5, y: knobRect.midY - 5.5, width: 11, height: 11)
        (isEnabled ? PlayerPalette.foreground : PlayerPalette.secondary).setFill()
        NSBezierPath(ovalIn: thumb).fill()
    }
}

private final class TimelineSlider: NSSlider {
    private(set) var isScrubbing = false

    override func mouseDown(with event: NSEvent) {
        isScrubbing = true
        defer { isScrubbing = false }
        super.mouseDown(with: event)
    }
}

@MainActor
private final class MoviePlayer: NSObject {
    static var active: MoviePlayer?
    static var frameOpener: BedizOpenFrame?
    static var frameRenderer: BedizFrameRenderer?

    private let view = PlaybackView()
    private lazy var workspace = content()
    private var window: NSWindow? { workspace.window }
    private var focusPreview = false
    private let timeline = TimelineSlider(value: 0, minValue: 0, maxValue: 1, target: nil, action: nil)
    private let elapsed = NSTextField(labelWithString: "00:00:00.000")
    private let duration = NSTextField(labelWithString: "00:00:00.000")
    private let speed = NSPopUpButton(frame: .zero, pullsDown: false)
    private let volume = NSSlider(value: 1, minValue: 0, maxValue: 1, target: nil, action: nil)
    private let speeds: [Float] = [0.5, 0.75, 1, 1.25, 1.5, 2]
    private lazy var play = button("play.fill", "Play", #selector(togglePlayback), primary: true)
    private lazy var previous = button("backward.frame", "Previous frame", #selector(previousFrame))
    private lazy var next = button("forward.frame", "Next frame", #selector(nextFrame))
    private lazy var mute = button("speaker.wave.2", "Mute", #selector(toggleMute))
    private lazy var fullscreen = button("arrow.up.left.and.arrow.down.right", "Enter full screen", #selector(toggleFullscreen))
    private lazy var edit = button("film", "Show video timeline", #selector(toggleEditing))
    private let editor = VideoTimelineEditor()
    private var currentURL: URL?
    private var exactPosition: Int64?
    private var statusObservation: NSKeyValueObservation?
    private var timeObserver: Any?
    private var endObserver: NSObjectProtocol?
    private var isMuted = false

    override init() {
        super.init()
        view.playbackKey = { [weak self] event in self?.handleKey(event) ?? false }
        view.controlsStyle = .none
        view.videoGravity = .resizeAspect
        editor.onSeek = { [weak self] position in self?.seekExact(position, pause: true) }
        editor.onPreview = { [weak self] item, position in
            self?.install(item)
            self?.seekExact(position, pause: true)
        }
        editor.onSource = { [weak self] url, _ in
            self?.currentURL = url
        }
        editor.onStill = { [weak self] position, maxEdge in
            guard let item = self?.view.player?.currentItem else { return nil }
            let generator = AVAssetImageGenerator(asset: item.asset)
            generator.appliesPreferredTrackTransform = true
            generator.videoComposition = item.videoComposition
            generator.maximumSize = CGSize(width: maxEdge, height: maxEdge)
            generator.requestedTimeToleranceBefore = .zero
            generator.requestedTimeToleranceAfter = .zero
            let length = self?.editor.length ?? self?.ticks(item.duration.seconds) ?? 0
            let time = min(position, max(0, length - 1))
            return try? await generator.image(at: videoTime(time)).image
        }
    }

    func layout(in host: NSView?, x: CGFloat, y: CGFloat, width: CGFloat, height: CGFloat, pause: Bool) {
        guard let host, width > 0, height > 0 else {
            if !workspace.isHidden, let window, let responder = window.firstResponder as? NSView,
               responder.isDescendant(of: workspace) {
                window.makeFirstResponder(workspace.superview)
                focusPreview = true
            }
            workspace.isHidden = true
            if pause { view.player?.pause() }
            return
        }
        let bounds = host.bounds
        let bottom = host.isFlipped ? bounds.minY + y : bounds.maxY - y - height
        let frame = NSRect(x: bounds.minX + x, y: bottom, width: width, height: height).intersection(bounds)
        if workspace.superview !== host {
            workspace.removeFromSuperview()
            host.addSubview(workspace, positioned: .above, relativeTo: nil)
        }
        if workspace.frame != frame { workspace.frame = frame }
        workspace.isHidden = frame.isEmpty
        if focusPreview && !workspace.isHidden {
            focusPreview = false
            window?.makeFirstResponder(view)
        }
        let full = window?.styleMask.contains(.fullScreen) == true
        setSymbol(fullscreen, full ? "arrow.down.right.and.arrow.up.left" : "arrow.up.left.and.arrow.down.right",
                  full ? "Exit full screen" : "Enter full screen")
    }

    private func content() -> NSView {
        let root = NSView()
        root.wantsLayer = true
        root.layer?.backgroundColor = NSColor.black.cgColor
        let transport = TransportDeck()
        transport.appearance = NSAppearance(named: .darkAqua)
        transport.wantsLayer = true
        let topRule = NSView()
        topRule.translatesAutoresizingMaskIntoConstraints = false
        topRule.wantsLayer = true
        topRule.layer?.backgroundColor = PlayerPalette.divider.cgColor
        transport.addSubview(topRule)

        timeline.cell = TimelineCell()
        timeline.minValue = 0
        timeline.maxValue = 1
        timeline.target = self
        timeline.action = #selector(seek)
        timeline.isContinuous = true
        timeline.setAccessibilityLabel("Playback timeline")
        timeline.setContentHuggingPriority(.defaultLow, for: .horizontal)
        timeline.heightAnchor.constraint(equalToConstant: 24).isActive = true

        for label in [elapsed, duration] {
            label.font = .monospacedDigitSystemFont(ofSize: 12, weight: .medium)
            label.widthAnchor.constraint(greaterThanOrEqualToConstant: 88).isActive = true
            label.setContentHuggingPriority(.defaultHigh, for: .horizontal)
        }
        elapsed.textColor = PlayerPalette.foreground
        duration.textColor = PlayerPalette.secondary
        duration.alignment = .right
        elapsed.setAccessibilityLabel("Elapsed time")
        duration.setAccessibilityLabel("Duration")
        let seekRow = row([elapsed, timeline, duration], spacing: 12)

        speed.addItems(withTitles: ["0.5×", "0.75×", "1×", "1.25×", "1.5×", "2×"])
        speed.selectItem(at: 2)
        speed.target = self
        speed.action = #selector(changeSpeed)
        speed.setAccessibilityLabel("Playback speed")
        speed.toolTip = "Playback speed"
        speed.font = .monospacedDigitSystemFont(ofSize: 12, weight: .semibold)
        speed.isBordered = false
        speed.contentTintColor = PlayerPalette.foreground
        speed.wantsLayer = true
        speed.layer?.cornerRadius = 8
        speed.layer?.backgroundColor = NSColor.white.withAlphaComponent(0.075).cgColor
        speed.widthAnchor.constraint(equalToConstant: 70).isActive = true
        speed.heightAnchor.constraint(equalToConstant: 30).isActive = true

        volume.target = self
        volume.action = #selector(changeVolume)
        volume.isContinuous = true
        volume.controlSize = .small
        volume.setAccessibilityLabel("Volume")
        volume.toolTip = "Volume"
        volume.trackFillColor = PlayerPalette.accent
        volume.widthAnchor.constraint(equalToConstant: 66).isActive = true
        let separator = NSView()
        separator.wantsLayer = true
        separator.layer?.backgroundColor = PlayerPalette.divider.cgColor
        separator.widthAnchor.constraint(equalToConstant: 1).isActive = true
        separator.heightAnchor.constraint(equalToConstant: 18).isActive = true
        let audio = row([mute, volume, separator, fullscreen], spacing: 8)
        let playback = row([previous, play, next], spacing: 12)
        let tools = row([speed, edit], spacing: 8)
        let actions = NSView()
        for subview in [tools, playback, audio] {
            subview.translatesAutoresizingMaskIntoConstraints = false
            actions.addSubview(subview)
        }
        NSLayoutConstraint.activate([
            actions.heightAnchor.constraint(equalTo: play.heightAnchor),
            tools.leadingAnchor.constraint(equalTo: actions.leadingAnchor),
            tools.centerYAnchor.constraint(equalTo: actions.centerYAnchor),
            playback.centerXAnchor.constraint(equalTo: actions.centerXAnchor),
            playback.centerYAnchor.constraint(equalTo: actions.centerYAnchor),
            audio.trailingAnchor.constraint(equalTo: actions.trailingAnchor),
            audio.centerYAnchor.constraint(equalTo: actions.centerYAnchor),
            playback.leadingAnchor.constraint(greaterThanOrEqualTo: tools.trailingAnchor, constant: 12),
            audio.leadingAnchor.constraint(greaterThanOrEqualTo: playback.trailingAnchor, constant: 12),
        ])
        let controls = NSStackView(views: [seekRow, actions])
        controls.orientation = .vertical
        controls.alignment = .width
        controls.spacing = 5
        controls.edgeInsets = NSEdgeInsets(top: 8, left: 18, bottom: 12, right: 18)
        controls.translatesAutoresizingMaskIntoConstraints = false
        transport.addSubview(controls)
        let preferredWidth = controls.widthAnchor.constraint(equalTo: transport.widthAnchor)
        // Let AppKit resize the window independently of the control deck's width cap.
        let stayPut = NSLayoutConstraint.Priority.windowSizeStayPut.rawValue
        preferredWidth.priority = NSLayoutConstraint.Priority(rawValue: stayPut - 1)

        let bottom = VideoEditUI.column([transport, editor.view], spacing: 0)
        editor.view.isHidden = true
        for subview in [view, bottom] {
            subview.translatesAutoresizingMaskIntoConstraints = false
            root.addSubview(subview)
        }
        NSLayoutConstraint.activate([
            view.topAnchor.constraint(equalTo: root.topAnchor),
            view.leadingAnchor.constraint(equalTo: root.leadingAnchor),
            view.trailingAnchor.constraint(equalTo: root.trailingAnchor),
            view.bottomAnchor.constraint(equalTo: bottom.topAnchor),
            bottom.leadingAnchor.constraint(equalTo: root.leadingAnchor),
            bottom.trailingAnchor.constraint(equalTo: root.trailingAnchor),
            bottom.bottomAnchor.constraint(equalTo: root.bottomAnchor),
            topRule.leadingAnchor.constraint(equalTo: transport.leadingAnchor),
            topRule.trailingAnchor.constraint(equalTo: transport.trailingAnchor),
            topRule.topAnchor.constraint(equalTo: transport.topAnchor),
            topRule.heightAnchor.constraint(equalToConstant: 1),
            controls.centerXAnchor.constraint(equalTo: transport.centerXAnchor),
            controls.widthAnchor.constraint(lessThanOrEqualTo: transport.widthAnchor),
            controls.widthAnchor.constraint(lessThanOrEqualToConstant: 920),
            preferredWidth,
            controls.topAnchor.constraint(equalTo: transport.topAnchor),
            controls.bottomAnchor.constraint(equalTo: transport.bottomAnchor),
        ])
        return root
    }

    private func row(_ views: [NSView], spacing: CGFloat) -> NSStackView {
        let result = NSStackView(views: views)
        result.orientation = .horizontal
        result.alignment = .centerY
        result.spacing = spacing
        return result
    }

    private func button(_ symbol: String, _ label: String, _ action: Selector, primary: Bool = false) -> NSButton {
        let button = TransportButton(primary: primary)
        button.target = self
        button.action = action
        setSymbol(button, symbol, label)
        return button
    }

    private func setSymbol(_ button: NSButton, _ symbol: String, _ label: String) {
        if button.toolTip == label { return }
        button.image = NSImage(systemSymbolName: symbol, accessibilityDescription: label)
        button.toolTip = label
        button.setAccessibilityLabel(label)
    }

    func open(_ url: URL, artworkRenderer: BedizArtworkRenderer?, edit: Bool) -> Bool {
        guard !editor.hasFrameReference else { return false }
        // Assemble the workspace before accessing its timeline controls.
        _ = workspace
        editor.artworkRenderer = artworkRenderer
        editor.openReference = Self.frameOpener
        editor.frameRenderer = Self.frameRenderer
        if url.pathExtension.lowercased() == "bedizvideo" {
            guard editor.allowClose() else { return false }
            setEditingVisible(true)
            view.player?.pause()
            editor.loadProject(url: url)
        } else if currentURL != url {
            guard editor.allowClose() else { return false }
            editor.reset()
            setEditingVisible(false)
            currentURL = url
            install(AVPlayerItem(url: url))
        }
        if edit && editor.view.isHidden { toggleEditing() }
        focusPreview = true
        return true
    }

    private func install(_ item: AVPlayerItem?) {
        closeItem()
        if let item {
            let player = AVPlayer(playerItem: item)
            player.volume = volume.floatValue
            player.isMuted = isMuted
            view.player = player
            updateControls()
            statusObservation = item.observe(\.status, options: [.initial, .new]) { [weak self] item, _ in
                Task { @MainActor [weak self, weak item] in
                    guard let self, let item, self.view.player?.currentItem === item else { return }
                    self.updateControls()
                    if item.status == .failed { self.showError(item.error) }
                }
            }
            timeObserver = player.addPeriodicTimeObserver(
                forInterval: CMTime(value: 1, timescale: 12), queue: .main
            ) { [weak self] _ in
                Task { @MainActor [weak self] in self?.updateControls() }
            }
            endObserver = NotificationCenter.default.addObserver(
                forName: .AVPlayerItemDidPlayToEndTime, object: item, queue: .main
            ) { [weak self] _ in
                Task { @MainActor [weak self] in self?.updateControls() }
            }
        }
        updateControls()
    }

    private var playbackRate: Float { speeds[max(0, speed.indexOfSelectedItem)] }

    private func ticks(_ seconds: Double) -> Int64 {
        seconds.isFinite ? Int64(min(max(0, seconds * 1_000), Double(VideoTime.maximum)).rounded()) : 0
    }

    private func updateControls() {
        let player = view.player
        let end = player?.currentItem?.duration.seconds ?? 0
        let ready = end.isFinite && end > 0 && player?.currentItem?.status == .readyToPlay
        let maximum = editor.length ?? ticks(end)
        timeline.isEnabled = ready
        play.isEnabled = ready
        previous.isEnabled = ready
        next.isEnabled = ready
        timeline.maxValue = maximum > 0 ? Double(maximum) / 1_000 : 1
        duration.stringValue = VideoTime.format(maximum)
        if !timeline.isScrubbing {
            let now = min(exactPosition ?? ticks(player?.currentTime().seconds ?? 0), maximum)
            timeline.doubleValue = Double(now) / 1_000
            elapsed.stringValue = VideoTime.format(now)
            if editor.isPreviewCurrent { editor.updatePlaybackTime(now) }
        }
        if (player?.timeControlStatus ?? .paused) == .paused {
            setSymbol(play, "play.fill", "Play")
        } else {
            setSymbol(play, "pause.fill", "Pause")
        }
    }

    @objc private func togglePlayback() {
        guard let player = view.player, play.isEnabled, editor.isPreviewCurrent else { return }
        if player.timeControlStatus != .paused {
            player.pause()
        } else {
            exactPosition = nil
            if timeline.doubleValue >= timeline.maxValue - 0.001 { player.seek(to: .zero) }
            player.playImmediately(atRate: playbackRate)
        }
        updateControls()
    }

    @objc private func seek() {
        seekExact(ticks(timeline.doubleValue), pause: !editor.view.isHidden)
    }

    private func seekExact(_ position: Int64, pause: Bool) {
        let player = view.player
        if pause { player?.pause() }
        exactPosition = (player?.timeControlStatus ?? .paused) == .paused ? position : nil
        let end = editor.length ?? ticks(player?.currentItem?.duration.seconds ?? 0)
        let lastFramePosition = end > 0 && position >= end ? end - 1 : position
        player?.seek(to: videoTime(lastFramePosition), toleranceBefore: .zero, toleranceAfter: .zero)
        elapsed.stringValue = VideoTime.format(position)
        timeline.doubleValue = Double(position) / 1_000
        if editor.isPreviewCurrent { editor.updatePlaybackTime(position) }
    }

    @objc private func previousFrame() { step(-1) }
    @objc private func nextFrame() { step(1) }

    private func step(_ count: Int) {
        guard let player = view.player, editor.isPreviewCurrent else { return }
        player.pause()
        exactPosition = nil
        player.currentItem?.step(byCount: count)
        updateControls()
    }

    @objc private func changeSpeed() {
        if let player = view.player, player.timeControlStatus != .paused {
            player.playImmediately(atRate: playbackRate)
        }
    }

    @objc private func toggleMute() {
        isMuted.toggle()
        view.player?.isMuted = isMuted
        setSymbol(mute, isMuted ? "speaker.slash" : "speaker.wave.2", isMuted ? "Unmute" : "Mute")
    }

    @objc private func changeVolume() {
        view.player?.volume = volume.floatValue
        if isMuted { toggleMute() }
    }

    @objc private func toggleFullscreen() { window?.toggleFullScreen(nil) }

    @objc private func toggleEditing() {
        guard let currentURL else { return }
        let opening = editor.view.isHidden
        setEditingVisible(opening)
        if opening {
            view.player?.pause()
            editor.start(source: currentURL, time: exactPosition ?? ticks(view.player?.currentTime().seconds ?? 0))
        }
    }

    private func setEditingVisible(_ visible: Bool) {
        editor.view.isHidden = !visible
        setSymbol(edit, "film", visible ? "Hide video timeline" : "Show video timeline")
    }

    private func handleKey(_ event: NSEvent) -> Bool {
        guard event.modifierFlags.intersection([.command, .control, .option]).isEmpty else { return false }
        switch event.keyCode {
        case 49: togglePlayback()
        case 123: previousFrame()
        case 124: nextFrame()
        default: return false
        }
        return true
    }

    private func showError(_ error: Error?) {
        guard let window, window.attachedSheet == nil else { return }
        let alert = NSAlert()
        alert.messageText = "Bediz couldn’t play this video"
        alert.informativeText = error?.localizedDescription ?? "The video may be incomplete or use an unsupported codec."
        alert.beginSheetModal(for: window)
    }

    private func closeItem() {
        statusObservation = nil
        if let timeObserver { view.player?.removeTimeObserver(timeObserver) }
        if let endObserver { NotificationCenter.default.removeObserver(endObserver) }
        timeObserver = nil
        endObserver = nil
        exactPosition = nil
        view.player?.pause()
        view.player = nil
    }

    func receiveReference(id: UInt64, data: Data?, duration: Int64, effects: Bool, objects: Data?) -> Bool {
        editor.receiveReference(id: id, bytes: data, duration: duration, usesEffects: effects, objects: objects)
    }

    func saveProject() {
        guard !editor.hasFrameReference else { return }
        if editor.view.isHidden { toggleEditing() }
        _ = editor.saveProject()
    }

    func prepareToQuit() -> Bool {
        guard editor.allowClose() else { return false }
        closeItem()
        currentURL = nil
        editor.reset()
        setEditingVisible(false)
        workspace.removeFromSuperview()
        return true
    }
}

@_cdecl("bediz_video_frame_bridge")
@MainActor
public func bediz_video_frame_bridge(
    _ open: @convention(c) (UInt64, UnsafePointer<UInt8>, Int, Int64, Int64, UnsafePointer<UInt8>?, Int) -> Bool,
    _ render: @convention(c) (UnsafePointer<UInt8>, Int, UnsafeMutablePointer<UInt8>, Int, UInt32, UInt32) -> Bool
) {
    MoviePlayer.frameOpener = open
    MoviePlayer.frameRenderer = render
}

@_cdecl("bediz_video_frame_result")
@MainActor
public func bediz_video_frame_result(
    _ id: UInt64, _ bytes: UnsafePointer<UInt8>?, _ length: Int, _ duration: Int64, _ kind: Int32,
    _ objects: UnsafePointer<UInt8>?, _ objectLength: Int
) -> Bool {
    guard let player = MoviePlayer.active else { return false }
    if kind == 0 { return player.receiveReference(id: id, data: nil, duration: 0, effects: false, objects: nil) }
    let limit = 32 * 1_024 * 1_024
    guard (kind == 1 || kind == 2), let bytes, (1...limit).contains(length), (0...limit).contains(objectLength),
          kind == 1 ? (objects != nil && objectLength > 0) : objectLength == 0 else { return false }
    let editable = objects.map { Data(bytes: $0, count: objectLength) }
    return player.receiveReference(id: id, data: Data(bytes: bytes, count: length),
                                   duration: duration, effects: kind == 2, objects: editable)
}

@_cdecl("bediz_play_video")
@MainActor
public func bediz_play_video(
    _ path: UnsafePointer<CChar>, _ artworkRenderer: (@convention(c) (UnsafePointer<CChar>, UnsafePointer<CChar>) -> Bool)?,
    _ edit: Bool
) -> Bool {
    let player = MoviePlayer.active ?? MoviePlayer()
    MoviePlayer.active = player
    return player.open(URL(fileURLWithPath: String(cString: path)), artworkRenderer: artworkRenderer, edit: edit)
}

@_cdecl("bediz_video_layout")
@MainActor
public func bediz_video_layout(
    _ host: UnsafeMutableRawPointer?, _ x: Double, _ y: Double, _ width: Double, _ height: Double, _ pause: Bool
) {
    let view = host.map { Unmanaged<NSView>.fromOpaque($0).takeUnretainedValue() }
    MoviePlayer.active?.layout(in: view, x: x, y: y, width: width, height: height, pause: pause)
}

@_cdecl("bediz_video_save_project")
@MainActor
public func bediz_video_save_project() {
    MoviePlayer.active?.saveProject()
}

@_cdecl("bediz_video_prepare_to_quit")
@MainActor
public func bediz_video_prepare_to_quit() -> Bool {
    guard MoviePlayer.active?.prepareToQuit() != false else { return false }
    MoviePlayer.active = nil
    return true
}

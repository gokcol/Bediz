import AppKit
import AVKit
import AVFoundation

@MainActor
private final class MovieWindow: NSWindowController, NSWindowDelegate {
    static let shared = MovieWindow()

    private let view = AVPlayerView()
    private var currentURL: URL?
    private var observation: NSKeyValueObservation?

    private init() {
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 880, height: 540),
            styleMask: [.titled, .closable, .miniaturizable, .resizable],
            backing: .buffered,
            defer: false
        )
        super.init(window: window)
        window.delegate = self
        window.isReleasedWhenClosed = false
        window.minSize = NSSize(width: 360, height: 240)
        window.collectionBehavior = [.fullScreenPrimary]
        view.controlsStyle = .floating
        view.showsFullScreenToggleButton = true
        view.videoGravity = .resizeAspect
        window.contentView = view
        window.center()
    }

    required init?(coder: NSCoder) { nil }

    func open(_ url: URL) {
        guard let window else { return }
        if currentURL != url {
            observation = nil
            view.player?.pause()
            currentURL = url
            window.title = "\(url.lastPathComponent) — Bediz Player"
            let item = AVPlayerItem(url: url)
            let player = AVPlayer(playerItem: item)
            view.player = player
            observation = item.observe(\.status, options: [.initial, .new]) { [weak self] item, _ in
                guard item.status == .failed else { return }
                Task { @MainActor [weak self, weak item] in
                    guard let self, let item, self.view.player?.currentItem === item else { return }
                    self.showError(item.error)
                }
            }
            player.play()
        }
        if window.isMiniaturized { window.deminiaturize(nil) }
        window.makeKeyAndOrderFront(nil)
        window.makeFirstResponder(view)
    }

    private func showError(_ error: Error?) {
        guard let window, window.attachedSheet == nil else { return }
        let alert = NSAlert()
        alert.messageText = "Bediz couldn’t play this video"
        alert.informativeText = error?.localizedDescription ?? "The MP4 may be incomplete or use an unsupported codec."
        alert.beginSheetModal(for: window)
    }

    func windowWillClose(_ notification: Notification) {
        observation = nil
        currentURL = nil
        view.player?.pause()
        view.player = nil
    }
}

@_cdecl("bediz_play_video")
@MainActor
public func bediz_play_video(_ path: UnsafePointer<CChar>) {
    MovieWindow.shared.open(URL(fileURLWithPath: String(cString: path)))
}

import AppKit
import AVFoundation
import CoreImage
import Foundation
import ImageIO
import OSLog
@preconcurrency import ScreenCaptureKit

private struct RecorderError: LocalizedError {
    let errorDescription: String?

    init(_ message: String) {
        errorDescription = message
    }
}

// Share a single camera between the live preview and the recording encoder.
private final class WebcamSource: NSObject, AVCaptureVideoDataOutputSampleBufferDelegate, @unchecked Sendable {
    private static let queue = DispatchQueue(label: "com.myimage.app.webcam")
    private static let log = Logger(subsystem: "com.myimage.app", category: "PresenterCamera")
    let session = AVCaptureSession()
    private let lock = NSLock()
    private var frame: CIImage?
    private var failure: String?

    override init() {
        super.init()
        Task { [weak self] in
            let allowed: Bool
            switch AVCaptureDevice.authorizationStatus(for: .video) {
            case .authorized: allowed = true
            case .notDetermined: allowed = await AVCaptureDevice.requestAccess(for: .video)
            default: allowed = false
            }
            if allowed {
                Self.queue.async { [weak self] in self?.configure() }
            } else {
                self?.fail("Allow Bediz in System Settings → Privacy & Security → Camera, or turn off the presenter camera.")
            }
        }
    }

    deinit {
        let capture = session
        Self.queue.async { if capture.isRunning { capture.stopRunning() } }
    }

    private func configure() {
        do {
            guard let device = AVCaptureDevice.default(.builtInWideAngleCamera, for: .video, position: .front)
                    ?? AVCaptureDevice.default(for: .video) else {
                throw RecorderError("No camera is available. Turn off the presenter camera to record the screen.")
            }
            if session.canSetSessionPreset(.medium) { session.sessionPreset = .medium }
            let input = try AVCaptureDeviceInput(device: device)
            let output = AVCaptureVideoDataOutput()
            output.alwaysDiscardsLateVideoFrames = true
            output.videoSettings = [kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32BGRA]
            guard session.canAddInput(input) else {
                throw RecorderError("The camera could not connect. Check whether another app is using it.")
            }
            session.addInput(input)
            guard session.canAddOutput(output) else {
                throw RecorderError("Bediz could not receive video from this camera.")
            }
            session.addOutput(output)
            output.setSampleBufferDelegate(self, queue: Self.queue)
            session.startRunning()
            if !session.isRunning { throw RecorderError("The camera session could not start.") }
            Self.log.notice("Camera session started")
            Self.queue.asyncAfter(deadline: .now() + 6) { [weak self] in
                guard let self, self.snapshot().frame == nil else { return }
                self.fail("No camera frames arrived. Close other apps using the camera, then turn the presenter camera off and on.")
            }
        } catch {
            fail(error.localizedDescription)
        }
    }

    private func fail(_ message: String) {
        Self.log.error("Camera unavailable: \(message, privacy: .public)")
        lock.lock()
        failure = message
        lock.unlock()
    }

    func snapshot() -> (frame: CIImage?, failure: String?) {
        lock.lock()
        defer { lock.unlock() }
        return (frame, failure)
    }

    func captureOutput(_ output: AVCaptureOutput, didOutput sample: CMSampleBuffer, from connection: AVCaptureConnection) {
        guard let buffer = sample.imageBuffer else { return }
        let image = CIImage(cvPixelBuffer: buffer)
        lock.lock()
        let firstFrame = frame == nil
        frame = image
        failure = nil
        lock.unlock()
        if firstFrame { Self.log.notice("First camera frame received") }
    }
}

@MainActor
private final class WebcamPreview {
    private static weak var activeSource: WebcamSource?
    let source: WebcamSource
    private var panel: NSPanel?
    private var layer: AVCaptureVideoPreviewLayer?
    private lazy var thumbnailContext = CIContext(options: [.cacheIntermediates: false])
    private var didRenderThumbnail = false

    init() {
        source = Self.activeSource ?? WebcamSource()
        Self.activeSource = source
    }

    func show(target: CGRect, position: CGPoint, windowID: CGWindowID, sizeRatio: CGFloat, mirrored: Bool) {
        guard source.snapshot().frame != nil else { return }
        var bounds = target
        if windowID != 0 {
            guard let window = CGWindowListCopyWindowInfo(.optionIncludingWindow, windowID)
                    as? [[String: Any]],
                  let info = window.first, info[kCGWindowIsOnscreen as String] as? Bool == true,
                  let rect = info[kCGWindowBounds as String] as? [String: Any],
                  let frame = CGRect(dictionaryRepresentation: rect as CFDictionary) else {
                hide()
                return
            }
            bounds = frame
        }
        guard let primary = NSScreen.screens.first else { return }
        let bubble = webcamBounds(in: bounds, at: position, sizeRatio: sizeRatio)
        let frame = CGRect(x: bubble.minX, y: primary.frame.maxY - bubble.maxY,
                           width: bubble.width, height: bubble.height)
        if panel == nil { makePanel(frame: frame) }
        guard let panel, let layer, let view = panel.contentView else { return }
        if let connection = layer.connection, connection.isVideoMirroringSupported,
           connection.isVideoMirrored != mirrored {
            connection.isVideoMirrored = mirrored
        }
        if panel.frame != frame {
            panel.setFrame(frame, display: false)
            view.layer?.cornerRadius = frame.width / 2
            CATransaction.begin()
            CATransaction.setDisableActions(true)
            layer.frame = view.bounds
            CATransaction.commit()
        }
        if !panel.isVisible { panel.orderFrontRegardless() }
    }

    private func makePanel(frame: CGRect) {
        let panel = NSPanel(contentRect: frame, styleMask: [.borderless, .nonactivatingPanel],
                            backing: .buffered, defer: false)
        panel.title = "Bediz Webcam Preview"
        panel.isReleasedWhenClosed = false
        panel.animationBehavior = .none
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = false
        panel.hidesOnDeactivate = false
        panel.ignoresMouseEvents = true
        panel.level = NSWindow.Level(rawValue: Int(CGWindowLevelForKey(.overlayWindow)) + 1)
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .transient, .ignoresCycle]
        if #available(macOS 13.0, *) { panel.collectionBehavior.insert(.canJoinAllApplications) }
        let view = NSView(frame: CGRect(origin: .zero, size: frame.size))
        view.wantsLayer = true
        view.layer?.masksToBounds = true
        view.layer?.cornerRadius = frame.width / 2
        view.layer?.borderWidth = 1.4
        view.layer?.borderColor = NSColor.white.withAlphaComponent(0.75).cgColor
        let layer = AVCaptureVideoPreviewLayer(session: source.session)
        layer.videoGravity = .resizeAspectFill
        if let connection = layer.connection, connection.isVideoMirroringSupported {
            connection.automaticallyAdjustsVideoMirroring = false
            connection.isVideoMirrored = false
        }
        layer.frame = view.bounds
        view.layer?.addSublayer(layer)
        panel.contentView = view
        self.panel = panel
        self.layer = layer
    }

    func hide() { panel?.orderOut(nil) }

    func copyThumbnail(to buffer: UnsafeMutableRawPointer, side: Int, mirrored: Bool) -> Bool {
        guard let camera = source.snapshot().frame else { return false }
        let box = CGRect(x: 0, y: 0, width: side, height: side)
        let face = fillCamera(camera, box: box, mirrored: mirrored)
        thumbnailContext.render(face, toBitmap: buffer, rowBytes: side * 4, bounds: box,
                                format: .RGBA8, colorSpace: CGColorSpace(name: CGColorSpace.sRGB))
        if !didRenderThumbnail {
            didRenderThumbnail = true
            Logger(subsystem: "com.myimage.app", category: "PresenterCamera").notice("Live preview received its first frame")
        }
        return true
    }

    func close() {
        layer?.session = nil
        panel?.close()
        layer = nil
        panel = nil
    }
}

private func webcamBounds(in bounds: CGRect, at position: CGPoint, sizeRatio: CGFloat) -> CGRect {
    let size = min(bounds.width, bounds.height)
    let diameter = size * sizeRatio
    let inset = size * 0.035
    return CGRect(
        x: bounds.minX + inset + position.x * (bounds.width - diameter - 2 * inset),
        y: bounds.minY + inset + position.y * (bounds.height - diameter - 2 * inset),
        width: diameter, height: diameter
    )
}

private func fillCamera(_ camera: CIImage, box: CGRect, mirrored: Bool) -> CIImage {
    let scale = box.width / min(camera.extent.width, camera.extent.height)
    let scaleX = mirrored ? -scale : scale
    let transform = CGAffineTransform(
        a: scaleX, b: 0, c: 0, d: scale,
        tx: box.midX - camera.extent.midX * scaleX,
        ty: box.midY - camera.extent.midY * scale
    )
    return camera.transformed(by: transform).cropped(to: box)
}

@_cdecl("bediz_webcam_create")
@MainActor
public func bediz_webcam_create() -> UnsafeMutableRawPointer {
    Unmanaged.passRetained(WebcamPreview()).toOpaque()
}

@_cdecl("bediz_webcam_show")
@MainActor
public func bediz_webcam_show(
    _ handle: UnsafeRawPointer, _ x: Double, _ y: Double, _ width: Double, _ height: Double,
    _ cameraX: Double, _ cameraY: Double, _ window: UInt32,
    _ sizeRatio: Double, _ mirrored: Int32
) {
    let preview = Unmanaged<WebcamPreview>.fromOpaque(handle).takeUnretainedValue()
    if width > 0 && height > 0 {
        preview.show(target: CGRect(x: x, y: y, width: width, height: height),
                     position: CGPoint(x: cameraX, y: cameraY), windowID: window,
                     sizeRatio: sizeRatio, mirrored: mirrored != 0)
    } else {
        preview.hide()
    }
}

@_cdecl("bediz_webcam_copy_preview")
@MainActor
public func bediz_webcam_copy_preview(
    _ handle: UnsafeRawPointer, _ output: UnsafeMutableRawPointer, _ capacity: Int,
    _ side: Int, _ mirrored: Int32
) -> Int32 {
    guard (32...256).contains(side), capacity >= 4 * side * side else { return 0 }
    let preview = Unmanaged<WebcamPreview>.fromOpaque(handle).takeUnretainedValue()
    return preview.copyThumbnail(to: output, side: side, mirrored: mirrored != 0) ? 1 : 0
}

@_cdecl("bediz_webcam_status")
@MainActor
public func bediz_webcam_status(
    _ handle: UnsafeRawPointer, _ output: UnsafeMutablePointer<CChar>, _ capacity: Int
) -> Int32 {
    let preview = Unmanaged<WebcamPreview>.fromOpaque(handle).takeUnretainedValue()
    let (frame, failure) = preview.source.snapshot()
    if let failure {
        _ = failure.withCString { strlcpy(output, $0, capacity) }
        return -1
    }
    return frame == nil ? 0 : 1
}

@_cdecl("bediz_webcam_release")
@MainActor
public func bediz_webcam_release(_ handle: UnsafeRawPointer) {
    Unmanaged<WebcamPreview>.fromOpaque(handle).takeRetainedValue().close()
}

// Screen frames, controls and the writer run on one serial queue.
// The UI reads the current status without blocking that queue.
private struct WindowTile: Decodable {
    let id: CGWindowID
    let x: Int
    let y: Int
    let width: Int
    let height: Int
    let cursor: Bool
    let fill: Bool
    let detail: Int

    func place(_ image: CIImage, canvasHeight: Int) -> CIImage {
        let box = CGRect(x: x, y: canvasHeight - y - height, width: width, height: height)
        let original = image.extent
        let scales = (box.width / original.width, box.height / original.height)
        let scale = fill ? max(scales.0, scales.1) : min(scales.0, scales.1)
        let scaled = image.transformed(by: CGAffineTransform(
            a: scale, b: 0, c: 0, d: scale,
            tx: box.midX - original.midX * scale,
            ty: box.midY - original.midY * scale
        ))
        return scaled.cropped(to: box)
    }
}

private struct RecordingRate: Equatable {
    static let fallback = RecordingRate(numerator: 30, denominator: 1)
    let numerator: Int32
    let denominator: Int64

    var fps: Double { Double(numerator) / Double(denominator) }
    var tickNanoseconds: Int { Int((Double(denominator) * 1_000_000_000 / Double(numerator)).rounded()) }

    func time(atFrame frame: Int64) -> CMTime {
        CMTime(value: frame * denominator, timescale: numerator)
    }

    func frame(at elapsed: UInt64, nearest: Bool = false) -> Int64 {
        let scale = UInt64(denominator) * 1_000_000_000
        let rounding = nearest ? scale / 2 : 0
        return Int64((elapsed * UInt64(numerator) + rounding) / scale)
    }
}

private final class Recorder: NSObject, SCStreamOutput, SCStreamDelegate,
    AVCaptureAudioDataOutputSampleBufferDelegate,
    @unchecked Sendable
{
    private let queue = DispatchQueue(label: "com.myimage.app.recording")
    private let statusLock = NSLock()
    private var status: Int32 = 0
    private var statusText = ""
    private let destination: URL
    private let cameraPosition: CGPoint?
    private let cameraSize: CGFloat
    private let cameraMirrored: Bool
    private var webcam: WebcamSource?
    private let maxNanoseconds: UInt64
    private let diskReserve: UInt64
    private let audioSource: AudioSource
    private let audioSampleRate: Int
    private let audioBitsPerSecond: Int
    private let voiceProcessing: Bool
    private let countdownNanoseconds: UInt64
    private var countdownDeadline: UInt64?
    private var audioEngine: AVAudioEngine?
    private var audioClock: (pts: CMTime, uptime: Double)?
    private var lastAudioEnd: Double = 0
    private var audioInput: AVAssetWriterInput?
    private var lastDiskCheck: UInt64 = 0
    private let windowTiles: [WindowTile]
    private var streams: [SCStream] = []
    private var screenStreams: [SCStream] = []
    private var screenConfigurations: [SCStreamConfiguration] = []
    private var windowFrames: [ObjectIdentifier: CIImage] = [:]
    private var pendingStarts = 0
    private var deviceSession: AVCaptureSession?
    private var screenFrame: CVPixelBuffer?
    private var timer: DispatchSourceTimer?
    private var writer: AVAssetWriter?
    private var input: AVAssetWriterInput?
    private var pixels: AVAssetWriterInputPixelBufferAdaptor?
    private var compositor: CIContext?
    private var startupTicks = 0
    private var activeStart: UInt64?
    private var elapsed: UInt64 = 0
    private var lastFrameIndex: Int64 = -1
    private var finishing = false
    private var frameRate: RecordingRate
    private let fallbackBitrate: Int
    private var usedFallbackRate = false
    private var outputWidth = 0
    private var outputHeight = 0
    private let fallbackNotice = "Recorded at 30 fps because the encoder could not use the selected rate."

    fileprivate enum AudioSource: UInt32 {
        case off, system, microphone
    }

    init(destination: URL, cameraPosition: CGPoint?, cameraSize: CGFloat, cameraMirrored: Bool,
         webcam: WebcamSource?, maxSeconds: UInt32, diskReserve: UInt64,
         audioSource: AudioSource, audioSampleRate: Int, audioBitsPerSecond: Int,
         voiceProcessing: Bool, countdownSeconds: UInt32, windowTiles: [WindowTile],
         frameRate: RecordingRate, fallbackBitrate: Int) {
        self.destination = destination
        self.cameraPosition = cameraPosition
        self.cameraSize = cameraSize
        self.cameraMirrored = cameraMirrored
        self.webcam = webcam
        self.maxNanoseconds = UInt64(maxSeconds) * 1_000_000_000
        self.diskReserve = diskReserve
        self.audioSource = audioSource
        self.audioSampleRate = audioSampleRate
        self.audioBitsPerSecond = audioBitsPerSecond
        self.voiceProcessing = voiceProcessing
        self.countdownNanoseconds = UInt64(countdownSeconds) * 1_000_000_000
        self.windowTiles = windowTiles
        self.frameRate = frameRate
        self.fallbackBitrate = fallbackBitrate
    }

    func statusSnapshot() -> (Int32, String) {
        statusLock.lock()
        defer { statusLock.unlock() }
        return (status, statusText)
    }

    private func publish(_ value: Int32, _ message: String = "") {
        statusLock.lock()
        status = value
        statusText = message
        statusLock.unlock()
    }

    func start(displayID: CGDirectDisplayID, windowID: CGWindowID, cursor: Bool,
               width: Int, height: Int, bitsPerSecond: Int) {
        // Permission and ScreenCaptureKit setup are asynchronous. In
        // particular, a camera consent prompt must never block AppKit.
        Task { [self] in
            do {
                if audioSource == .microphone {
                    let allowed: Bool
                    switch AVCaptureDevice.authorizationStatus(for: .audio) {
                    case .authorized: allowed = true
                    case .notDetermined: allowed = await AVCaptureDevice.requestAccess(for: .audio)
                    default: allowed = false
                    }
                    guard allowed else {
                        throw RecorderError("Microphone access is off. Enable Bediz in Privacy & Security → Microphone, or choose a different audio source.")
                    }
                }
                let content = try await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: false)
                var sources: [(filter: SCContentFilter, config: SCStreamConfiguration, screen: Bool, audio: Bool)] = []
                func displayFilter() throws -> SCContentFilter {
                    let mirrorID = CGDisplayMirrorsDisplay(displayID)
                    guard let display = content.displays.first(where: { $0.displayID == displayID })
                        ?? content.displays.first(where: { $0.displayID == mirrorID }) else {
                        throw RecorderError("The display chosen for recording is no longer available.")
                    }
                    let applications = content.applications.filter {
                        $0.processID == ProcessInfo.processInfo.processIdentifier
                    }
                    return SCContentFilter(display: display, excludingApplications: applications, exceptingWindows: [])
                }
                if !windowTiles.isEmpty {
                    for tile in windowTiles {
                        guard let window = content.windows.first(where: { $0.windowID == tile.id }) else {
                            throw RecorderError("One of the selected windows is no longer available.")
                        }
                        let filter = SCContentFilter(desktopIndependentWindow: window)
                        let retina = CGFloat(filter.pointPixelScale)
                        let w = max(2, window.frame.width * retina)
                        let h = max(2, window.frame.height * retina)
                        let scales = (CGFloat(tile.width) / w, CGFloat(tile.height) / h)
                        let tileScale = tile.fill ? max(scales.0, scales.1) : min(scales.0, scales.1)
                        let detail = tile.detail > 0 ? CGFloat(tile.detail) / min(w, h) : 1
                        let scale = min(1, tileScale, detail, 3840 / max(w, h))
                        let config = streamConfiguration(
                            width: max(2, Int(w * scale) & ~1),
                            height: max(2, Int(h * scale) & ~1),
                            cursor: tile.cursor,
                            window: true,
                            audio: false
                        )
                        config.preservesAspectRatio = true
                        sources.append((filter, config, true, false))
                    }
                    if audioSource == .system {
                        // Capture system audio once. It must not be limited to one of
                        // the windows, or counted twice when both are in the same app.
                        let config = streamConfiguration(width: 2, height: 2, cursor: false, window: false, audio: true)
                        sources.append((try displayFilter(), config, false, true))
                    }
                } else if windowID != 0 {
                    guard let window = content.windows.first(where: { $0.windowID == windowID }) else {
                        throw RecorderError("The window chosen for recording is no longer available.")
                    }
                    let audio = audioSource == .system
                    let config = streamConfiguration(width: width, height: height, cursor: cursor, window: true, audio: audio)
                    sources.append((SCContentFilter(desktopIndependentWindow: window), config, true, audio))
                } else {
                    let audio = audioSource == .system
                    let config = streamConfiguration(width: width, height: height, cursor: cursor, window: false, audio: audio)
                    sources.append((try displayFilter(), config, true, audio))
                }
                queue.async { [sources] in
                    guard !self.finishing else { return }
                    do {
                        self.outputWidth = width
                        self.outputHeight = height
                        do {
                            try self.configureWriter(bitsPerSecond: bitsPerSecond)
                        } catch where self.frameRate != .fallback {
                            try self.configureFallbackWriter()
                        }
                        if self.webcam != nil || !self.windowTiles.isEmpty {
                            self.compositor = CIContext(options: [.cacheIntermediates: false])
                        }
                        if self.audioSource == .microphone && !self.voiceProcessing {
                            try self.configureMicrophone()
                        }
                        if self.audioSource == .microphone && self.voiceProcessing {
                            try self.configureProcessedMicrophone()
                        }
                        for source in sources {
                            source.config.minimumFrameInterval = self.frameRate.time(atFrame: 1)
                            let stream = SCStream(filter: source.filter, configuration: source.config, delegate: self)
                            if source.screen {
                                try stream.addStreamOutput(self, type: .screen, sampleHandlerQueue: self.queue)
                                self.screenStreams.append(stream)
                                self.screenConfigurations.append(source.config)
                            }
                            if source.audio {
                                try stream.addStreamOutput(self, type: .audio, sampleHandlerQueue: self.queue)
                            }
                            self.streams.append(stream)
                        }
                        self.pendingStarts = self.streams.count
                        for stream in self.streams {
                            stream.startCapture { [weak self] error in
                                guard let self else { return }
                                self.queue.async {
                                    guard !self.finishing else { return }
                                    if let error {
                                        self.fail(error)
                                    } else {
                                        self.pendingStarts -= 1
                                        if self.pendingStarts == 0 { self.startTimer() }
                                    }
                                }
                            }
                        }
                    } catch {
                        self.fail(error)
                    }
                }
            } catch {
                queue.async {
                    guard !self.finishing else { return }
                    self.fail(error)
                }
            }
        }
    }

    private func streamConfiguration(width: Int, height: Int, cursor: Bool, window: Bool,
                                     audio: Bool) -> SCStreamConfiguration {
        let config = SCStreamConfiguration()
        config.width = width
        config.height = height
        config.preservesAspectRatio = false
        config.ignoreShadowsSingleWindow = window
        config.pixelFormat = kCVPixelFormatType_32BGRA
        config.queueDepth = 4
        config.showsCursor = cursor
        config.colorSpaceName = CGColorSpace.sRGB
        if audio {
            config.capturesAudio = true
            config.excludesCurrentProcessAudio = true
            config.sampleRate = 48_000
            config.channelCount = 2
        }
        return config
    }

    private func configureWriter(bitsPerSecond: Int) throws {
        let writer = try AVAssetWriter(outputURL: destination, fileType: .mp4)
        let input = AVAssetWriterInput(mediaType: .video, outputSettings: [
            AVVideoCodecKey: AVVideoCodecType.h264,
            AVVideoWidthKey: outputWidth,
            AVVideoHeightKey: outputHeight,
            AVVideoCompressionPropertiesKey: [
                AVVideoExpectedSourceFrameRateKey: frameRate.fps,
                AVVideoAverageBitRateKey: bitsPerSecond,
            ],
        ])
        input.expectsMediaDataInRealTime = true
        input.mediaTimeScale = frameRate.numerator
        guard writer.canAdd(input) else {
            throw RecorderError("The H.264 recorder could not use the selected dimensions.")
        }
        let adaptor = AVAssetWriterInputPixelBufferAdaptor(assetWriterInput: input, sourcePixelBufferAttributes: [
            kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32BGRA,
            kCVPixelBufferWidthKey as String: outputWidth,
            kCVPixelBufferHeightKey as String: outputHeight,
            kCVPixelBufferIOSurfacePropertiesKey as String: [:],
        ])
        writer.add(input)
        if audioSource != .off {
            let channels: UInt32 = audioSource == .microphone ? 1 : 2
            let rates = supportedAACBitRates(sampleRate: audioSampleRate, channels: channels)
            guard let bitrate = rates.last(where: { $0 <= audioBitsPerSecond }) ?? rates.first else {
                throw RecorderError("The recorder could not use the selected AAC sample rate.")
            }
            let audioSettings: [String: Any] = [
                AVFormatIDKey: kAudioFormatMPEG4AAC,
                AVSampleRateKey: audioSampleRate,
                AVNumberOfChannelsKey: channels,
                AVEncoderBitRateKey: bitrate,
            ]
            guard writer.canApply(outputSettings: audioSettings, forMediaType: .audio) else {
                throw RecorderError("The recorder could not use the selected AAC audio settings.")
            }
            let audio = AVAssetWriterInput(mediaType: .audio, outputSettings: audioSettings)
            audio.expectsMediaDataInRealTime = true
            guard writer.canAdd(audio) else {
                throw RecorderError("The recorder could not use the selected AAC audio settings.")
            }
            writer.add(audio)
            audioInput = audio
        }
        guard writer.startWriting() else {
            throw writer.error ?? RecorderError("The video file could not be created.")
        }
        writer.startSession(atSourceTime: .zero)
        self.writer = writer
        self.input = input
        self.pixels = adaptor
    }

    private func configureFallbackWriter() throws {
        if writer?.status == .writing { writer?.cancelWriting() }
        writer = nil
        input = nil
        pixels = nil
        audioInput = nil
        if FileManager.default.fileExists(atPath: destination.path) {
            try FileManager.default.removeItem(at: destination)
        }
        frameRate = .fallback
        usedFallbackRate = true
        try configureWriter(bitsPerSecond: fallbackBitrate)
    }

    private func handleWriterFailure(_ error: Error) {
        guard lastFrameIndex < 0, frameRate != .fallback else {
            fail(error)
            return
        }
        do {
            try configureFallbackWriter()
            activeStart = nil
            for (stream, configuration) in zip(screenStreams, screenConfigurations) {
                configuration.minimumFrameInterval = frameRate.time(atFrame: 1)
                stream.updateConfiguration(configuration) { _ in }
            }
            timer?.cancel()
            startTimer()
        } catch {
            fail(error)
        }
    }

    private func configureMicrophone() throws {
        let session = AVCaptureSession()
        guard let device = AVCaptureDevice.default(for: .audio) else {
            throw RecorderError("No microphone is available.")
        }
        let microphone = try AVCaptureDeviceInput(device: device)
        let audio = AVCaptureAudioDataOutput()
        guard session.canAddInput(microphone), session.canAddOutput(audio) else {
            throw RecorderError("The microphone could not be started.")
        }
        session.addInput(microphone)
        session.addOutput(audio)
        audio.setSampleBufferDelegate(self, queue: queue)
        session.startRunning()
        guard session.isRunning else {
            throw RecorderError("The microphone is unavailable. Check the device and try again.")
        }
        deviceSession = session
    }

    private func configureProcessedMicrophone() throws {
        let engine = AVAudioEngine()
        let input = engine.inputNode
        do {
            try input.setVoiceProcessingEnabled(true)
        } catch {
            throw RecorderError("Microphone noise reduction is unavailable for this input. Turn it off or choose another microphone in System Settings.")
        }
        let format = input.outputFormat(forBus: 0)
        guard format.sampleRate > 0, format.channelCount > 0 else {
            throw RecorderError("The microphone did not provide an audio format.")
        }
        input.installTap(onBus: 0, bufferSize: 1024, format: nil) { [weak self] buffer, time in
            guard let self, let sample = Self.makeSample(buffer, time: time) else { return }
            self.queue.async { self.appendAudio(sample) }
        }
        do {
            try engine.start()
        } catch {
            input.removeTap(onBus: 0)
            throw RecorderError("The microphone could not start with noise reduction. Check the input device or turn noise reduction off.")
        }
        audioEngine = engine
    }

    private static func makeSample(_ pcm: AVAudioPCMBuffer, time: AVAudioTime) -> CMSampleBuffer? {
        guard pcm.frameLength > 0, pcm.format.sampleRate > 0, time.isSampleTimeValid else { return nil }
        let rate = Int32(pcm.format.sampleRate.rounded())
        var timing = CMSampleTimingInfo(
            duration: CMTime(value: 1, timescale: rate),
            presentationTimeStamp: CMTime(value: time.sampleTime, timescale: rate),
            decodeTimeStamp: .invalid
        )
        var buffer: CMSampleBuffer?
        let created = CMSampleBufferCreate(
            allocator: kCFAllocatorDefault, dataBuffer: nil, dataReady: false,
            makeDataReadyCallback: nil, refcon: nil, formatDescription: pcm.format.formatDescription,
            sampleCount: Int(pcm.frameLength), sampleTimingEntryCount: 1, sampleTimingArray: &timing,
            sampleSizeEntryCount: 0, sampleSizeArray: nil, sampleBufferOut: &buffer
        )
        guard created == noErr, let buffer,
              CMSampleBufferSetDataBufferFromAudioBufferList(
                buffer, blockBufferAllocator: kCFAllocatorDefault,
                blockBufferMemoryAllocator: kCFAllocatorDefault, flags: 0,
                bufferList: pcm.audioBufferList
              ) == noErr,
              CMSampleBufferSetDataReady(buffer) == noErr else { return nil }
        return buffer
    }

    func stream(_ stream: SCStream, didOutputSampleBuffer sampleBuffer: CMSampleBuffer, of type: SCStreamOutputType) {
        if type == .audio {
            appendAudio(sampleBuffer)
            return
        }
        guard !finishing, type == .screen, let frame = sampleBuffer.imageBuffer,
              let metadata = (CMSampleBufferGetSampleAttachmentsArray(sampleBuffer, createIfNecessary: false)
                as? [[SCStreamFrameInfo: Any]])?.first,
              let value = metadata[.status] as? Int,
              SCFrameStatus(rawValue: value) == .complete else { return }
        if windowTiles.isEmpty {
            screenFrame = frame
        } else {
            let image = CIImage(cvPixelBuffer: frame)
            var content = image
            if let dictionary = metadata[.contentRect] as? [String: Any],
               let rect = CGRect(dictionaryRepresentation: dictionary as CFDictionary),
               let scale = metadata[.scaleFactor] as? CGFloat, scale > 0 {
                // The attachments use top-left points and Core Image uses bottom-left pixels.
                let crop = CGRect(x: rect.minX * scale, y: image.extent.height - rect.maxY * scale,
                                  width: rect.width * scale, height: rect.height * scale)
                    .intersection(image.extent)
                if !crop.isNull && crop.width >= 2 && crop.height >= 2 {
                    content = image.cropped(to: crop)
                }
            }
            windowFrames[ObjectIdentifier(stream)] = content
        }
    }

    func stream(_ stream: SCStream, didStopWithError error: Error) {
        queue.async {
            guard !self.finishing else { return }
            // Save any frames written before an unexpected display disconnect.
            self.finish(note: "Screen capture ended early. " + error.localizedDescription)
        }
    }

    func captureOutput(_ output: AVCaptureOutput, didOutput sampleBuffer: CMSampleBuffer, from connection: AVCaptureConnection) {
        appendAudio(sampleBuffer)
    }

    private func appendAudio(_ sample: CMSampleBuffer) {
        guard !finishing, lastFrameIndex >= 0, let start = activeStart,
              let audioInput, audioInput.isReadyForMoreMediaData,
              sample.presentationTimeStamp.isNumeric,
              let format = sample.formatDescription,
              let description = CMAudioFormatDescriptionGetStreamBasicDescription(format),
              description.pointee.mSampleRate > 0 else { return }
        let rate = description.pointee.mSampleRate
        let duration = Double(sample.numSamples) / rate
        guard duration > 0 else { return }
        let pts = sample.presentationTimeStamp
        // Neither input is required to share the video stopwatch's epoch.
        // Anchor each active segment when the first PCM buffer arrives and use
        // its source clock for every following packet until the next resume.
        let now = Double(DispatchTime.now().uptimeNanoseconds) / 1_000_000_000
        if audioClock == nil {
            audioClock = (pts, max(Double(start) / 1_000_000_000, now - duration))
        }
        guard let audioClock else { return }
        let offset = CMTimeSubtract(pts, audioClock.pts).seconds
        guard offset.isFinite else { return }
        let activeTime = audioClock.uptime + offset - Double(start) / 1_000_000_000
        guard activeTime >= -0.001 else { return }
        let time = max(lastAudioEnd, Double(elapsed) / 1_000_000_000 + max(0, activeTime))
        guard time + duration <= Double(maxNanoseconds) / 1_000_000_000 else { return }
        var timing = CMSampleTimingInfo(
            duration: CMTime(seconds: 1 / rate, preferredTimescale: 1_000_000_000),
            presentationTimeStamp: CMTime(seconds: time, preferredTimescale: 1_000_000_000),
            decodeTimeStamp: .invalid
        )
        var retimed: CMSampleBuffer?
        let result = CMSampleBufferCreateCopyWithNewTiming(
            allocator: kCFAllocatorDefault, sampleBuffer: sample,
            sampleTimingEntryCount: 1, sampleTimingArray: &timing, sampleBufferOut: &retimed
        )
        guard result == noErr, let retimed else {
            fail(RecorderError("The recording audio could not be synchronized."))
            return
        }
        if audioInput.append(retimed) {
            lastAudioEnd = time + duration
        } else if writer?.status == .failed {
            fail(writer?.error ?? RecorderError("The encoder could not write audio."))
        }
    }

    private func startTimer() {
        let timer = DispatchSource.makeTimerSource(queue: queue)
        let interval = frameRate.tickNanoseconds
        timer.schedule(deadline: .now(), repeating: .nanoseconds(interval),
                       leeway: .nanoseconds(min(3_000_000, interval / 10)))
        timer.setEventHandler { [weak self] in self?.tick() }
        self.timer = timer
        timer.resume()
    }

    private func tick() {
        let camera = webcam?.snapshot()
        let cameraFrame = camera?.frame
        if let failure = camera?.failure {
            fail(RecorderError(failure))
            return
        }
        let captureReady = windowTiles.isEmpty ? screenFrame != nil : windowFrames.count == windowTiles.count
        guard captureReady, cameraPosition == nil || cameraFrame != nil else {
            if lastFrameIndex == -1 {
                startupTicks += 1
                if startupTicks > Int(frameRate.fps * 8) {
                    fail(RecorderError("No screen or webcam frames arrived. Check the permissions and try again."))
                }
            }
            return
        }
        let now = DispatchTime.now().uptimeNanoseconds
        if lastFrameIndex < 0 && countdownNanoseconds > 0 {
            let deadline = countdownDeadline ?? now + countdownNanoseconds
            countdownDeadline = deadline
            if now < deadline {
                let seconds = (deadline - now + 999_999_999) / 1_000_000_000
                publish(6, "Starts in \(seconds)…")
                return
            }
        }
        if lastFrameIndex >= 0 && now - lastDiskCheck >= 2_000_000_000 {
            lastDiskCheck = now
            let folder = destination.deletingLastPathComponent()
            if let free = try? folder.resourceValues(forKeys: [.volumeAvailableCapacityKey]).volumeAvailableCapacity,
               free >= 0 && UInt64(free) < diskReserve {
                finish(note: "Stopped because the recording drive is low on space.")
                return
            }
        }
        if lastFrameIndex == -1 && activeStart == nil {
            activeStart = now
        }
        if let start = activeStart, elapsed + now - start >= maxNanoseconds {
            finish(note: "Maximum recording duration reached.")
            return
        }
        guard let start = activeStart, let input, let pixels, input.isReadyForMoreMediaData else {
            if writer?.status == .failed {
                handleWriterFailure(writer?.error ?? RecorderError("The encoder stopped."))
            }
            return
        }
        let lastAllowedFrame = max(0, frameRate.frame(at: maxNanoseconds) - 1)
        let frameIndex = lastFrameIndex < 0 ? 0
            : min(lastAllowedFrame, frameRate.frame(at: elapsed + now - start, nearest: true))
        guard frameIndex > lastFrameIndex else { return }
        autoreleasepool {
            let frame: CVPixelBuffer
            if let compositor {
                guard let pool = pixels.pixelBufferPool else {
                    fail(RecorderError("The video encoder could not allocate a frame."))
                    return
                }
                var target: CVPixelBuffer?
                let result = CVPixelBufferPoolCreatePixelBuffer(kCFAllocatorDefault, pool, &target)
                guard result == kCVReturnSuccess, let target else {
                    fail(RecorderError("The video encoder ran out of frame buffers."))
                    return
                }
                let canvas = CGRect(x: 0, y: 0, width: CVPixelBufferGetWidth(target), height: CVPixelBufferGetHeight(target))
                var image: CIImage
                if let screenFrame {
                    image = CIImage(cvPixelBuffer: screenFrame)
                } else {
                    image = CIImage(color: .black).cropped(to: canvas)
                    for (tile, stream) in zip(windowTiles, screenStreams) {
                        guard let content = windowFrames[ObjectIdentifier(stream)] else { return }
                        image = tile.place(content, canvasHeight: Int(canvas.height)).composited(over: image)
                    }
                }
                if let cameraFrame, let cameraPosition {
                    image = renderCamera(cameraFrame, at: cameraPosition, over: image)
                }
                compositor.render(image, to: target, bounds: canvas, colorSpace: CGColorSpace(name: CGColorSpace.sRGB))
                frame = target
            } else if let screenFrame {
                frame = screenFrame
            } else {
                return
            }
            // Fill brief scheduling gaps without building a backlog if the
            // machine cannot sustain the selected rate.
            let firstFrame = max(lastFrameIndex + 1, frameIndex - 2)
            for index in firstFrame...frameIndex {
                guard input.isReadyForMoreMediaData else { return }
                guard pixels.append(frame, withPresentationTime: frameRate.time(atFrame: index)) else {
                    handleWriterFailure(writer?.error ?? RecorderError("The encoder could not write a video frame."))
                    return
                }
                if lastFrameIndex < 0 { publish(1, usedFallbackRate ? fallbackNotice : "") }
                lastFrameIndex = index
            }
        }
    }

    private func renderCamera(_ camera: CIImage, at position: CGPoint, over screen: CIImage) -> CIImage {
        // Core Image's origin is at the bottom left of the captured frame.
        let box = webcamBounds(in: screen.extent, at: CGPoint(x: position.x, y: 1 - position.y),
                               sizeRatio: cameraSize)
        let diameter = box.width
        let center = CGPoint(x: box.midX, y: box.midY)
        let face = fillCamera(camera, box: box, mirrored: cameraMirrored)
        let mask = CIFilter(name: "CIRadialGradient", parameters: [
            "inputCenter": CIVector(cgPoint: center),
            "inputRadius0": max(0, diameter / 2 - 1),
            "inputRadius1": diameter / 2,
            "inputColor0": CIColor.white,
            "inputColor1": CIColor.black,
        ])?.outputImage?.cropped(to: box)
        return mask.flatMap {
            CIFilter(name: "CIBlendWithMask", parameters: [
                kCIInputImageKey: face,
                kCIInputBackgroundImageKey: CIImage(color: .clear).cropped(to: box),
                kCIInputMaskImageKey: $0,
            ])?.outputImage?.composited(over: screen)
        } ?? screen
    }

    func command(_ command: Int32) {
        queue.async {
            guard !self.finishing else { return }
            switch command {
            case 1 where self.lastFrameIndex >= 0:
                guard let start = self.activeStart else { return }
                self.elapsed += DispatchTime.now().uptimeNanoseconds - start
                self.activeStart = nil
                self.audioClock = nil
                self.publish(2)
            case 2 where self.lastFrameIndex >= 0 && self.activeStart == nil:
                self.activeStart = DispatchTime.now().uptimeNanoseconds
                self.publish(1)
            case 3:
                self.finish()
            default:
                break
            }
        }
    }

    private func releaseSources() {
        timer?.cancel()
        timer = nil
        for stream in streams { stream.stopCapture(completionHandler: nil) }
        streams.removeAll()
        screenStreams.removeAll()
        screenConfigurations.removeAll()
        windowFrames.removeAll()
        deviceSession?.stopRunning()
        deviceSession = nil
        if let audioEngine {
            audioEngine.inputNode.removeTap(onBus: 0)
            audioEngine.stop()
            self.audioEngine = nil
        }
        screenFrame = nil
        webcam = nil
    }

    private func fail(_ error: Error) {
        finishing = true
        releaseSources()
        writer?.cancelWriting()
        publish(-1, error.localizedDescription)
    }

    private func finish(note: String? = nil) {
        finishing = true
        publish(3)
        let activeDuration = elapsed + (activeStart.map { DispatchTime.now().uptimeNanoseconds - $0 } ?? 0)
        releaseSources()
        guard lastFrameIndex >= 0, let writer, let input else {
            self.writer?.cancelWriting()
            if let note {
                publish(-1, note)
            } else {
                publish(5)
            }
            return
        }
        let cappedDuration = min(activeDuration, maxNanoseconds)
        let endFrame = max(lastFrameIndex + 1, frameRate.frame(at: cappedDuration))
        writer.endSession(atSourceTime: frameRate.time(atFrame: endFrame))
        input.markAsFinished()
        audioInput?.markAsFinished()
        writer.finishWriting { [self] in
            if writer.status == .completed {
                let messages = [note, usedFallbackRate ? fallbackNotice : nil].compactMap { $0 }
                publish(4, messages.joined(separator: " · "))
            } else {
                publish(-1, writer.error?.localizedDescription ?? "The video could not be finalized.")
            }
        }
    }
}

private func recorder(_ handle: UnsafeRawPointer) -> Recorder {
    Unmanaged<Recorder>.fromOpaque(handle).takeUnretainedValue()
}

@_cdecl("bediz_record_start")
@MainActor
public func bediz_record_start(
    _ path: UnsafePointer<CChar>, _ display: UInt32, _ window: UInt32,
    _ cursor: Int32, _ webcamPreview: UnsafeRawPointer?, _ cameraX: Double, _ cameraY: Double,
    _ cameraSize: Double, _ cameraMirrored: Int32,
    _ outputWidth: UInt32, _ outputHeight: UInt32, _ bitsPerSecond: UInt32,
    _ rateNumerator: UInt32, _ rateDenominator: UInt32, _ fallbackBitrate: UInt32,
    _ maxSeconds: UInt32, _ diskReserve: UInt64, _ audioSource: UInt32, _ audioSampleRate: UInt32, _ audioBitsPerSecond: UInt32,
    _ voiceProcessing: Int32, _ countdownSeconds: UInt32, _ tilesJSON: UnsafePointer<CChar>?
) -> UnsafeMutableRawPointer? {
    let tiles: [WindowTile]
    if let tilesJSON {
        guard let parsed = try? JSONDecoder().decode([WindowTile].self, from: Data(String(cString: tilesJSON).utf8)),
              (2...10).contains(parsed.count), Set(parsed.map(\.id)).count == parsed.count,
              parsed.allSatisfy({ tile in
                  tile.id != 0 && tile.x >= 0 && tile.y >= 0 && tile.width >= 2 && tile.height >= 2
                      && tile.x <= Int(outputWidth) - tile.width && tile.y <= Int(outputHeight) - tile.height
                      && (0...3840).contains(tile.detail)
              }) else { return nil }
        tiles = parsed
    } else {
        tiles = []
    }
    let destination = URL(fileURLWithPath: String(cString: path))
    let webcam = webcamPreview.map { Unmanaged<WebcamPreview>.fromOpaque($0).takeUnretainedValue().source }
    let position = webcam.map { _ in CGPoint(x: cameraX, y: cameraY) }
    let job = Recorder(destination: destination, cameraPosition: position, cameraSize: cameraSize,
                       cameraMirrored: cameraMirrored != 0, webcam: webcam, maxSeconds: maxSeconds,
                       diskReserve: diskReserve, audioSource: Recorder.AudioSource(rawValue: audioSource) ?? .off,
                       audioSampleRate: Int(audioSampleRate),
                       audioBitsPerSecond: Int(audioBitsPerSecond),
                       voiceProcessing: voiceProcessing != 0, countdownSeconds: countdownSeconds, windowTiles: tiles,
                       frameRate: RecordingRate(numerator: Int32(rateNumerator), denominator: Int64(rateDenominator)),
                       fallbackBitrate: Int(fallbackBitrate))
    job.start(displayID: display, windowID: window, cursor: cursor != 0,
              width: Int(outputWidth), height: Int(outputHeight), bitsPerSecond: Int(bitsPerSecond))
    return Unmanaged.passRetained(job).toOpaque()
}

@_cdecl("bediz_record_status")
public func bediz_record_status(_ handle: UnsafeRawPointer, _ output: UnsafeMutablePointer<CChar>, _ capacity: Int) -> Int32 {
    let (status, message) = recorder(handle).statusSnapshot()
    if capacity > 0 {
        _ = message.withCString { strlcpy(output, $0, capacity) }
    }
    return status
}

@_cdecl("bediz_record_command")
public func bediz_record_command(_ handle: UnsafeRawPointer, _ command: Int32) {
    recorder(handle).command(command)
}

@_cdecl("bediz_record_release")
public func bediz_record_release(_ handle: UnsafeRawPointer) {
    Unmanaged<Recorder>.fromOpaque(handle).release()
}

@_cdecl("bediz_record_thumbnail")
public func bediz_record_thumbnail(
    _ movie: UnsafePointer<CChar>,
    _ poster: UnsafePointer<CChar>,
    _ width: Int,
    _ height: Int,
    _ dimensions: UnsafeMutablePointer<UInt32>
) -> Int32 {
    autoreleasepool {
        let asset = AVURLAsset(url: URL(fileURLWithPath: String(cString: movie)))
        guard let track = asset.tracks(withMediaType: .video).first else { return 0 }
        let display = CGRect(origin: .zero, size: track.naturalSize).applying(track.preferredTransform).standardized
        guard let displayWidth = UInt32(exactly: display.width.rounded()), displayWidth > 0,
              let displayHeight = UInt32(exactly: display.height.rounded()), displayHeight > 0 else { return 0 }
        let generator = AVAssetImageGenerator(asset: asset)
        generator.appliesPreferredTrackTransform = true
        generator.maximumSize = CGSize(width: width, height: height)
        guard let image = try? generator.copyCGImage(at: .zero, actualTime: nil),
              let destination = CGImageDestinationCreateWithURL(
                URL(fileURLWithPath: String(cString: poster)) as CFURL,
                "public.jpeg" as CFString, 1, nil
              ) else { return 0 }
        CGImageDestinationAddImage(destination, image, nil)
        guard CGImageDestinationFinalize(destination) else { return 0 }
        dimensions[0] = displayWidth
        dimensions[1] = displayHeight
        return 1
    }
}

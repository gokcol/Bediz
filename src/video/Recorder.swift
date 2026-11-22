import AVFoundation
import CoreImage
import Foundation
import ImageIO
@preconcurrency import ScreenCaptureKit

private struct RecorderError: LocalizedError {
    let errorDescription: String?

    init(_ message: String) {
        errorDescription = message
    }
}

// Screen frames, camera frames, control changes and the writer all run on the
// same serial queue. The UI polls a separate status snapshot without blocking.
private final class Recorder: NSObject, SCStreamOutput, SCStreamDelegate,
    AVCaptureVideoDataOutputSampleBufferDelegate, AVCaptureAudioDataOutputSampleBufferDelegate,
    @unchecked Sendable
{
    private let queue = DispatchQueue(label: "com.myimage.app.recording")
    private let statusLock = NSLock()
    private var status: Int32 = 0
    private var statusText = ""
    private let destination: URL
    private let cameraPosition: CGPoint?
    private let maxNanoseconds: UInt64
    private let diskReserve: UInt64
    private let audioSource: AudioSource
    private let audioBitsPerSecond: Int
    private let voiceProcessing: Bool
    private let countdownNanoseconds: UInt64
    private var countdownDeadline: UInt64?
    private var audioEngine: AVAudioEngine?
    private var audioClock: (pts: CMTime, uptime: Double)?
    private var lastAudioEnd: Double = 0
    private var audioInput: AVAssetWriterInput?
    private var lastDiskCheck: UInt64 = 0
    private var stream: SCStream?
    private var deviceSession: AVCaptureSession?
    private var screenFrame: CVPixelBuffer?
    private var cameraFrame: CIImage?
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
    private let fps: Int32 = 30

    fileprivate enum AudioSource: UInt32 {
        case off, system, microphone
    }

    init(destination: URL, cameraPosition: CGPoint?, maxSeconds: UInt32, diskReserve: UInt64,
         audioSource: AudioSource, audioBitsPerSecond: Int,
         voiceProcessing: Bool, countdownSeconds: UInt32) {
        self.destination = destination
        self.cameraPosition = cameraPosition
        self.maxNanoseconds = UInt64(maxSeconds) * 1_000_000_000
        self.diskReserve = diskReserve
        self.audioSource = audioSource
        self.audioBitsPerSecond = audioBitsPerSecond
        self.voiceProcessing = voiceProcessing
        self.countdownNanoseconds = UInt64(countdownSeconds) * 1_000_000_000
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

    func start(displayID: CGDirectDisplayID, windowID: CGWindowID, selection: CGRect, cursor: Bool,
               width: Int, height: Int, bitsPerSecond: Int) {
        // Permission and ScreenCaptureKit setup are asynchronous. In
        // particular, a camera consent prompt must never block AppKit.
        Task { [self] in
            do {
                if cameraPosition != nil {
                    let allowed: Bool
                    switch AVCaptureDevice.authorizationStatus(for: .video) {
                    case .authorized: allowed = true
                    case .notDetermined: allowed = await AVCaptureDevice.requestAccess(for: .video)
                    default: allowed = false
                    }
                    guard allowed else {
                        throw RecorderError("Camera access is off. Enable Bediz in Privacy & Security → Camera, or record with the webcam off.")
                    }
                }
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
                let filter: SCContentFilter
                let config = SCStreamConfiguration()
                if windowID != 0 {
                    guard let window = content.windows.first(where: { $0.windowID == windowID }) else {
                        throw RecorderError("The window chosen for recording is no longer available.")
                    }
                    filter = SCContentFilter(desktopIndependentWindow: window)
                    config.ignoreShadowsSingleWindow = true
                } else {
                    let mirrorID = CGDisplayMirrorsDisplay(displayID)
                    guard let display = content.displays.first(where: { $0.displayID == displayID })
                        ?? content.displays.first(where: { $0.displayID == mirrorID }) else {
                        throw RecorderError("The display chosen for recording is no longer available.")
                    }
                    let applications = content.applications.filter {
                        $0.processID == ProcessInfo.processInfo.processIdentifier
                    }
                    filter = SCContentFilter(display: display, excludingApplications: applications, exceptingWindows: [])
                    let extent = filter.contentRect.size
                    config.sourceRect = CGRect(
                        x: selection.minX * extent.width, y: selection.minY * extent.height,
                        width: selection.width * extent.width, height: selection.height * extent.height
                    )
                }
                config.width = width
                config.height = height
                config.preservesAspectRatio = false
                config.pixelFormat = kCVPixelFormatType_32BGRA
                config.minimumFrameInterval = CMTime(value: 1, timescale: fps)
                config.queueDepth = 4
                config.showsCursor = cursor
                config.colorSpaceName = CGColorSpace.sRGB
                if audioSource == .system {
                    config.capturesAudio = true
                    config.excludesCurrentProcessAudio = true
                    config.sampleRate = 48_000
                    config.channelCount = 2
                }
                queue.async {
                    guard !self.finishing else { return }
                    do {
                        try self.configureWriter(width: width, height: height, bitsPerSecond: bitsPerSecond)
                        if self.cameraPosition != nil || (self.audioSource == .microphone && !self.voiceProcessing) {
                            try self.configureDevices()
                        }
                        if self.audioSource == .microphone && self.voiceProcessing {
                            try self.configureProcessedMicrophone()
                        }
                        let stream = SCStream(filter: filter, configuration: config, delegate: self)
                        self.stream = stream
                        try stream.addStreamOutput(self, type: .screen, sampleHandlerQueue: self.queue)
                        if self.audioSource == .system {
                            try stream.addStreamOutput(self, type: .audio, sampleHandlerQueue: self.queue)
                        }
                        stream.startCapture { [weak self] error in
                            guard let self else { return }
                            self.queue.async {
                                guard !self.finishing else { return }
                                if let error {
                                    self.fail(error)
                                } else {
                                    self.startTimer()
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

    private func configureWriter(width: Int, height: Int, bitsPerSecond: Int) throws {
        let writer = try AVAssetWriter(outputURL: destination, fileType: .mp4)
        let input = AVAssetWriterInput(mediaType: .video, outputSettings: [
            AVVideoCodecKey: AVVideoCodecType.h264,
            AVVideoWidthKey: width,
            AVVideoHeightKey: height,
            AVVideoCompressionPropertiesKey: [
                AVVideoExpectedSourceFrameRateKey: fps,
                AVVideoAverageBitRateKey: bitsPerSecond,
            ],
        ])
        input.expectsMediaDataInRealTime = true
        guard writer.canAdd(input) else {
            throw RecorderError("The H.264 recorder could not use the selected dimensions.")
        }
        let adaptor = AVAssetWriterInputPixelBufferAdaptor(assetWriterInput: input, sourcePixelBufferAttributes: [
            kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32BGRA,
            kCVPixelBufferWidthKey as String: width,
            kCVPixelBufferHeightKey as String: height,
            kCVPixelBufferIOSurfacePropertiesKey as String: [:],
        ])
        writer.add(input)
        if audioSource != .off {
            let audio = AVAssetWriterInput(mediaType: .audio, outputSettings: [
                AVFormatIDKey: kAudioFormatMPEG4AAC,
                AVSampleRateKey: 48_000,
                AVNumberOfChannelsKey: audioSource == .microphone ? 1 : 2,
                AVEncoderBitRateKey: audioBitsPerSecond,
            ])
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

    private func configureDevices() throws {
        let session = AVCaptureSession()
        if cameraPosition != nil {
            guard let device = AVCaptureDevice.default(for: .video) else {
                throw RecorderError("No webcam is available. Choose Webcam off to record the screen.")
            }
            if session.canSetSessionPreset(.medium) {
                session.sessionPreset = .medium
            }
            let camera = try AVCaptureDeviceInput(device: device)
            let video = AVCaptureVideoDataOutput()
            video.alwaysDiscardsLateVideoFrames = true
            video.videoSettings = [kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32BGRA]
            guard session.canAddInput(camera), session.canAddOutput(video) else {
                throw RecorderError("The webcam could not be started.")
            }
            session.addInput(camera)
            session.addOutput(video)
            video.setSampleBufferDelegate(self, queue: queue)
            compositor = CIContext()
        }
        if audioSource == .microphone && !voiceProcessing {
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
        }
        session.startRunning()
        guard session.isRunning else {
            throw RecorderError("The webcam or microphone is unavailable. Check the device and try again.")
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
        screenFrame = frame
    }

    func stream(_ stream: SCStream, didStopWithError error: Error) {
        queue.async {
            guard !self.finishing else { return }
            // Save any frames written before an unexpected display disconnect.
            self.finish(note: "Screen capture ended early. " + error.localizedDescription)
        }
    }

    func captureOutput(_ output: AVCaptureOutput, didOutput sampleBuffer: CMSampleBuffer, from connection: AVCaptureConnection) {
        if output is AVCaptureAudioDataOutput {
            appendAudio(sampleBuffer)
        } else if !finishing, let frame = sampleBuffer.imageBuffer {
            cameraFrame = CIImage(cvPixelBuffer: frame)
        }
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
        timer.schedule(deadline: .now(), repeating: .nanoseconds(1_000_000_000 / Int(fps)), leeway: .milliseconds(3))
        timer.setEventHandler { [weak self] in self?.tick() }
        self.timer = timer
        timer.resume()
    }

    private func tick() {
        guard let screenFrame, cameraPosition == nil || cameraFrame != nil else {
            if lastFrameIndex == -1 {
                startupTicks += 1
                if startupTicks > Int(fps) * 8 {
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
                fail(writer?.error ?? RecorderError("The encoder stopped."))
            }
            return
        }
        let frameIndex = lastFrameIndex < 0 ? 0 : Int64(Double(elapsed + now - start) * Double(fps) / 1_000_000_000)
        guard frameIndex > lastFrameIndex else { return }
        autoreleasepool {
            let frame: CVPixelBuffer
            if let cameraFrame, let cameraPosition, let compositor {
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
                renderCamera(cameraFrame, at: cameraPosition, over: screenFrame, into: target, using: compositor)
                frame = target
            } else {
                frame = screenFrame
            }
            if pixels.append(frame, withPresentationTime: CMTime(value: frameIndex, timescale: fps)) {
                if lastFrameIndex < 0 { publish(1) }
                lastFrameIndex = frameIndex
            } else {
                fail(writer?.error ?? RecorderError("The encoder could not write a video frame."))
            }
        }
    }

    private func renderCamera(_ camera: CIImage, at position: CGPoint,
                              over frame: CVPixelBuffer, into target: CVPixelBuffer, using context: CIContext) {
        let screen = CIImage(cvPixelBuffer: frame)
        let size = min(screen.extent.width, screen.extent.height)
        let diameter = size * 0.22
        let inset = size * 0.035
        let left = inset + position.x * (screen.extent.width - diameter - 2 * inset)
        let bottom = inset + (1 - position.y) * (screen.extent.height - diameter - 2 * inset)
        let center = CGPoint(x: left + diameter / 2, y: bottom + diameter / 2)
        let box = CGRect(x: left, y: bottom, width: diameter, height: diameter)
        let scale = diameter / min(camera.extent.width, camera.extent.height)
        let transform = CGAffineTransform(
            a: scale, b: 0, c: 0, d: scale,
            tx: center.x - camera.extent.midX * scale, ty: center.y - camera.extent.midY * scale
        )
        let face = camera.transformed(by: transform).cropped(to: box)
        let mask = CIFilter(name: "CIRadialGradient", parameters: [
            "inputCenter": CIVector(cgPoint: center),
            "inputRadius0": max(0, diameter / 2 - 1),
            "inputRadius1": diameter / 2,
            "inputColor0": CIColor.white,
            "inputColor1": CIColor.black,
        ])?.outputImage?.cropped(to: box)
        let composition = mask.flatMap {
            CIFilter(name: "CIBlendWithMask", parameters: [
                kCIInputImageKey: face,
                kCIInputBackgroundImageKey: CIImage(color: .clear).cropped(to: box),
                kCIInputMaskImageKey: $0,
            ])?.outputImage?.composited(over: screen)
        } ?? screen
        context.render(composition, to: target, bounds: screen.extent, colorSpace: CGColorSpace(name: CGColorSpace.sRGB))
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
        stream?.stopCapture(completionHandler: nil)
        stream = nil
        deviceSession?.stopRunning()
        deviceSession = nil
        if let audioEngine {
            audioEngine.inputNode.removeTap(onBus: 0)
            audioEngine.stop()
            self.audioEngine = nil
        }
        screenFrame = nil
        cameraFrame = nil
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
        let endFrame = max(lastFrameIndex + 1, Int64(Double(cappedDuration) * Double(fps) / 1_000_000_000))
        writer.endSession(atSourceTime: CMTime(value: endFrame, timescale: fps))
        input.markAsFinished()
        audioInput?.markAsFinished()
        writer.finishWriting { [self] in
            if writer.status == .completed {
                publish(4, note ?? "")
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
public func bediz_record_start(
    _ path: UnsafePointer<CChar>, _ display: UInt32, _ window: UInt32,
    _ x: Double, _ y: Double, _ width: Double, _ height: Double,
    _ cursor: Int32, _ cameraX: Double, _ cameraY: Double,
    _ outputWidth: UInt32, _ outputHeight: UInt32, _ bitsPerSecond: UInt32,
    _ maxSeconds: UInt32, _ diskReserve: UInt64, _ audioSource: UInt32, _ audioBitsPerSecond: UInt32,
    _ voiceProcessing: Int32, _ countdownSeconds: UInt32
) -> UnsafeMutableRawPointer {
    let destination = URL(fileURLWithPath: String(cString: path))
    let position = cameraX < 0 ? nil : CGPoint(x: cameraX, y: cameraY)
    let job = Recorder(destination: destination, cameraPosition: position, maxSeconds: maxSeconds,
                       diskReserve: diskReserve, audioSource: Recorder.AudioSource(rawValue: audioSource) ?? .off,
                       audioBitsPerSecond: Int(audioBitsPerSecond),
                       voiceProcessing: voiceProcessing != 0, countdownSeconds: countdownSeconds)
    job.start(displayID: display, windowID: window, selection: CGRect(x: x, y: y, width: width, height: height),
              cursor: cursor != 0, width: Int(outputWidth), height: Int(outputHeight), bitsPerSecond: Int(bitsPerSecond))
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
    _ height: Int
) -> Int32 {
    autoreleasepool {
        let asset = AVURLAsset(url: URL(fileURLWithPath: String(cString: movie)))
        let generator = AVAssetImageGenerator(asset: asset)
        generator.appliesPreferredTrackTransform = true
        generator.maximumSize = CGSize(width: width, height: height)
        guard let image = try? generator.copyCGImage(at: .zero, actualTime: nil),
              let destination = CGImageDestinationCreateWithURL(
                URL(fileURLWithPath: String(cString: poster)) as CFURL,
                "public.jpeg" as CFString, 1, nil
              ) else { return 0 }
        CGImageDestinationAddImage(destination, image, nil)
        return CGImageDestinationFinalize(destination) ? 1 : 0
    }
}

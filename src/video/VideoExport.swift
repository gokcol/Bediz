@preconcurrency import AVFoundation
import AudioToolbox
import UniformTypeIdentifiers

enum VideoContainer: String, CaseIterable {
    case mp4 = "MP4"
    case mov = "MOV"

    var fileType: AVFileType { self == .mp4 ? .mp4 : .mov }
    var contentType: UTType { self == .mp4 ? .mpeg4Movie : .quickTimeMovie }
    var suffix: String { rawValue.lowercased() }
}

enum VideoEncoder: String, CaseIterable {
    case h264 = "H.264"
    case hevc = "HEVC (H.265)"
    case proRes = "Apple ProRes 422"

    var type: AVVideoCodecType {
        switch self {
        case .h264: .h264
        case .hevc: .hevc
        case .proRes: .proRes422
        }
    }

    func fits(_ container: VideoContainer) -> Bool { self != .proRes || container == .mov }
}

enum SoundEncoder: String, CaseIterable {
    case aac = "AAC"
    case alac = "Apple Lossless"
    case pcm = "Linear PCM"
    case none = "No audio"

    func fits(_ container: VideoContainer) -> Bool { container == .mov || self == .aac || self == .none }
}

enum VideoExportQuality: String, CaseIterable {
    case compact = "Compact"
    case balanced = "Balanced"
    case high = "High"

    var bitsPerPixel: Double {
        switch self {
        case .compact: 0.10
        case .balanced: 0.16
        case .high: 0.26
        }
    }
}

enum VideoFraming: String, CaseIterable {
    case fit = "Fit all content"
    case fill = "Fill frame"

    func transform(from source: CGRect, to output: CGRect) -> CGAffineTransform {
        let x = output.width / source.width, y = output.height / source.height
        let scale = self == .fit ? min(x, y) : max(x, y)
        return CGAffineTransform(
            a: scale, b: 0, c: 0, d: scale,
            tx: (output.width - source.width * scale) / 2 - source.minX * scale,
            ty: (output.height - source.height * scale) / 2 - source.minY * scale
        )
    }
}

enum VideoExportPreset: String, CaseIterable {
    case source = "Original size"
    case phone = "Phone portrait"
    case phoneWide = "Phone landscape"
    case tablet = "Tablet (4:3)"
    case web = "Web (720p)"
    case hd = "Full HD (1080p)"
    case uhd = "4K UHD (2160p)"
    case square = "Square"
    case custom = "Custom"

    func size(source: CGSize) -> CGSize? {
        switch self {
        case .source: source
        case .phone: CGSize(width: 1080, height: 1920)
        case .phoneWide, .hd: CGSize(width: 1920, height: 1080)
        case .tablet: CGSize(width: 1600, height: 1200)
        case .web: CGSize(width: 1280, height: 720)
        case .uhd: CGSize(width: 3840, height: 2160)
        case .square: CGSize(width: 1080, height: 1080)
        case .custom: nil
        }
    }
}

struct VideoExportOptions {
    var size: CGSize
    var container: VideoContainer = .mp4
    var video: VideoEncoder = .h264
    var sound: SoundEncoder = .aac
    var framing: VideoFraming = .fit
    var quality: VideoExportQuality = .balanced
    var aacKbps = 192
    var includeSubtitles = true

    static let dimensionRange = 16...4096

    static func valid(_ value: Int) -> Bool { dimensionRange.contains(value) && value.isMultiple(of: 2) }

    var isValid: Bool {
        let values = [size.width, size.height]
        return values.allSatisfy {
            $0.isFinite && $0 >= CGFloat(Self.dimensionRange.lowerBound) && $0 <= CGFloat(Self.dimensionRange.upperBound)
                && $0 == $0.rounded() && Self.valid(Int($0))
        } && video.fits(container) && sound.fits(container) && [96, 128, 192, 256, 320].contains(aacKbps)
    }

    func pictureSettings(secondsPerFrame: Double) -> [String: Any] {
        var settings: [String: Any] = [
            AVVideoCodecKey: video.type.rawValue,
            AVVideoWidthKey: Int(size.width),
            AVVideoHeightKey: Int(size.height),
        ]
        if video != .proRes {
            let rate = secondsPerFrame.isFinite && secondsPerFrame > 0 ? min(60, max(1, 1 / secondsPerFrame)) : 30
            let efficiency = video == .hevc ? 0.72 : 1.0
            let bitrate = Int(size.width * size.height * rate * quality.bitsPerPixel * efficiency)
            settings[AVVideoCompressionPropertiesKey] = [AVVideoAverageBitRateKey: max(180_000, min(75_000_000, bitrate))]
        }
        return settings
    }

    static let pcmSettings: [String: Any] = [
        AVFormatIDKey: kAudioFormatLinearPCM,
        AVSampleRateKey: 48_000,
        AVNumberOfChannelsKey: 2,
        AVLinearPCMBitDepthKey: 16,
        AVLinearPCMIsBigEndianKey: false,
        AVLinearPCMIsFloatKey: false,
        AVLinearPCMIsNonInterleaved: false,
    ]

    var soundSettings: [String: Any]? {
        switch sound {
        case .aac:
            [AVFormatIDKey: kAudioFormatMPEG4AAC, AVSampleRateKey: 48_000,
             AVNumberOfChannelsKey: 2, AVEncoderBitRateKey: aacKbps * 1_000]
        case .alac:
            [AVFormatIDKey: kAudioFormatAppleLossless, AVSampleRateKey: 48_000,
             AVNumberOfChannelsKey: 2, AVEncoderBitDepthHintKey: 16]
        case .pcm: Self.pcmSettings
        case .none: nil
        }
    }
}

private enum VideoExportFailure: LocalizedError {
    case invalidOptions, videoSettings, audioSettings, start, read, write

    var errorDescription: String? {
        switch self {
        case .invalidOptions: "Check the output dimensions and codecs."
        case .videoSettings: "This Mac can’t encode the selected video codec at that size."
        case .audioSettings: "This Mac can’t encode the selected audio codec in this format."
        case .start: "Bediz couldn’t start the video export."
        case .read: "Bediz couldn’t read a frame from the edited timeline."
        case .write: "Bediz couldn’t finish writing the video."
        }
    }
}

extension VideoEditRender {
    func export(to url: URL, options: VideoExportOptions) async throws {
        guard options.isValid, video.renderSize == options.size else { throw VideoExportFailure.invalidOptions }
        let reader = try AVAssetReader(asset: asset)
        let writer = try AVAssetWriter(outputURL: url, fileType: options.container.fileType)
        writer.shouldOptimizeForNetworkUse = true
        let videoTracks = try await asset.loadTracks(withMediaType: .video)
        guard !videoTracks.isEmpty else { throw VideoEditError.noVideo }
        let picture = AVAssetReaderVideoCompositionOutput(videoTracks: videoTracks, videoSettings: [
            kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32BGRA,
        ])
        picture.videoComposition = video
        picture.alwaysCopiesSampleData = false
        let videoSettings = options.pictureSettings(secondsPerFrame: video.frameDuration.seconds)
        guard writer.canApply(outputSettings: videoSettings, forMediaType: .video), reader.canAdd(picture)
        else { throw VideoExportFailure.videoSettings }
        let videoInput = AVAssetWriterInput(mediaType: .video, outputSettings: videoSettings)
        guard writer.canAdd(videoInput) else { throw VideoExportFailure.videoSettings }
        reader.add(picture)
        writer.add(videoInput)
        var tracks = [ExportTransfer.Track(output: picture, input: videoInput)]

        if let soundSettings = options.soundSettings {
            let audio = try await asset.loadTracks(withMediaType: .audio)
            if !audio.isEmpty {
                let sound = AVAssetReaderAudioMixOutput(audioTracks: audio, audioSettings: VideoExportOptions.pcmSettings)
                sound.audioMix = audioMix
                sound.alwaysCopiesSampleData = false
                guard writer.canApply(outputSettings: soundSettings, forMediaType: .audio), reader.canAdd(sound)
                else { throw VideoExportFailure.audioSettings }
                let audioInput = AVAssetWriterInput(mediaType: .audio, outputSettings: soundSettings)
                guard writer.canAdd(audioInput) else { throw VideoExportFailure.audioSettings }
                reader.add(sound)
                writer.add(audioInput)
                tracks.append(.init(output: sound, input: audioInput))
            }
        }

        guard writer.startWriting() else { throw writer.error ?? VideoExportFailure.start }
        guard reader.startReading() else {
            writer.cancelWriting()
            throw reader.error ?? VideoExportFailure.start
        }
        writer.startSession(atSourceTime: .zero)
        let transfer = ExportTransfer(reader: reader, writer: writer)
        try await transfer.write(tracks)
    }
}

/// The reader and writer share one serial queue for progress and failure handling.
private final class ExportTransfer: @unchecked Sendable {
    // Once configured, sample I/O for both of these runs on the transfer queue.
    struct Track: @unchecked Sendable {
        let output: AVAssetReaderOutput
        let input: AVAssetWriterInput
    }
    private let queue = DispatchQueue(label: "com.myimage.app.video-export", qos: .userInitiated)
    private let reader: AVAssetReader
    private let writer: AVAssetWriter
    private var finished: Set<ObjectIdentifier> = []
    private var terminal = false

    init(reader: AVAssetReader, writer: AVAssetWriter) {
        self.reader = reader
        self.writer = writer
    }

    func write(_ tracks: [Track]) async throws {
        try await withCheckedThrowingContinuation { (result: CheckedContinuation<Void, Error>) in
            queue.async {
                for track in tracks {
                    track.input.requestMediaDataWhenReady(on: self.queue) {
                        self.transfer(track, count: tracks.count, result: result)
                    }
                }
            }
        }
    }

    private func transfer(_ track: Track, count: Int, result: CheckedContinuation<Void, Error>) {
        let id = ObjectIdentifier(track.input)
        guard !terminal, !finished.contains(id) else { return }
        while track.input.isReadyForMoreMediaData {
            guard let sample = track.output.copyNextSampleBuffer() else {
                if reader.status == .failed || reader.status == .cancelled {
                    fail(reader.error ?? VideoExportFailure.read, result: result)
                    return
                }
                track.input.markAsFinished()
                finished.insert(id)
                if finished.count == count {
                    terminal = true
                    writer.finishWriting {
                        if self.writer.status == .completed { result.resume() }
                        else { result.resume(throwing: self.writer.error ?? VideoExportFailure.write) }
                    }
                }
                return
            }
            if !track.input.append(sample) {
                fail(writer.error ?? VideoExportFailure.write, result: result)
                return
            }
        }
        if writer.status == .failed || writer.status == .cancelled {
            fail(writer.error ?? VideoExportFailure.write, result: result)
        }
    }

    private func fail(_ error: Error, result: CheckedContinuation<Void, Error>) {
        terminal = true
        if reader.status == .reading { reader.cancelReading() }
        if writer.status == .writing { writer.cancelWriting() }
        result.resume(throwing: error)
    }
}

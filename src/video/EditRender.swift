import AppKit
@preconcurrency import AVFoundation
import CoreImage
import CoreText
import ImageIO

typealias BedizFrameRenderer = @convention(c) (
    UnsafePointer<UInt8>, Int, UnsafeMutablePointer<UInt8>, Int, UInt32, UInt32
) -> Bool

func videoTime(_ milliseconds: Int64) -> CMTime { CMTime(value: milliseconds, timescale: 1_000) }

extension VideoInk {
    var color: CGColor {
        let rgb: (CGFloat, CGFloat, CGFloat)
        switch self {
        case .white: rgb = (0.96, 0.97, 1)
        case .blue: rgb = (0.38, 0.53, 1)
        case .yellow: rgb = (1, 0.79, 0.29)
        case .coral: rgb = (1, 0.43, 0.43)
        case .mint: rgb = (0.34, 0.85, 0.75)
        case .black: rgb = (0.08, 0.10, 0.15)
        }
        return CGColor(srgbRed: rgb.0, green: rgb.1, blue: rgb.2, alpha: 1)
    }
}

enum VideoDesign {
    static func image(_ data: Data, limit: Int = 4096) -> CGImage? {
        guard let source = CGImageSourceCreateWithData(data as CFData, nil) else { return nil }
        let options: [CFString: Any] = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceThumbnailMaxPixelSize: limit,
        ]
        return CGImageSourceCreateThumbnailAtIndex(source, 0, options as CFDictionary)
    }

    static func png(_ image: CGImage) -> Data? {
        let data = NSMutableData()
        guard let destination = CGImageDestinationCreateWithData(data, "public.png" as CFString, 1, nil) else { return nil }
        CGImageDestinationAddImage(destination, image, nil)
        return CGImageDestinationFinalize(destination) ? data as Data : nil
    }

    static func resize(_ image: CGImage, to size: CGSize) -> CGImage? {
        let width = Int(size.width), height = Int(size.height)
        if width == image.width && height == image.height { return image }
        guard let canvas = bitmap(width, height) else { return nil }
        canvas.draw(image, in: CGRect(origin: .zero, size: size))
        return canvas.makeImage()
    }

    private static func bitmap(_ width: Int, _ height: Int) -> CGContext? {
        CGContext(data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: 0,
                  space: CGColorSpace(name: CGColorSpace.sRGB)!,
                  bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
    }

    private static func fill(_ context: CGContext, _ rect: CGRect, _ color: CGColor) {
        context.setFillColor(color)
        context.fill(rect)
    }

    @discardableResult
    private static func text(_ context: CGContext, _ value: String, in rect: CGRect,
                             size: CGFloat, bold: Bool = false, color: CGColor = VideoInk.white.color,
                             centered: Bool = false) -> CGFloat {
        guard !value.isEmpty, rect.width > 0, rect.height > 0 else { return 0 }
        let font = CTFontCreateWithName((bold ? "HelveticaNeue-Bold" : "HelveticaNeue-Medium") as CFString, size, nil)
        let paragraph = NSMutableParagraphStyle()
        if centered { paragraph.alignment = .center }
        let attributed = NSAttributedString(string: value, attributes: [
            NSAttributedString.Key(kCTFontAttributeName as String): font,
            NSAttributedString.Key(kCTForegroundColorAttributeName as String): color,
            .paragraphStyle: paragraph,
        ])
        let setter = CTFramesetterCreateWithAttributedString(attributed)
        let frame = CTFramesetterCreateFrame(setter, CFRange(location: 0, length: 0), CGPath(rect: rect, transform: nil), nil)
        context.textMatrix = .identity
        CTFrameDraw(frame, context)
        return CTFramesetterSuggestFrameSizeWithConstraints(setter, CFRange(location: 0, length: 0),
                                                           nil, rect.size, nil).height
    }

    static func subtitle(_ value: String, size: CGSize) -> CIImage? {
        let unit = min(size.width, size.height)
        let font = max(1, unit * 0.046)
        let width = max(1, Int(size.width * 0.84))
        let height = max(1, Int(size.height * 0.23))
        guard let canvas = bitmap(width, height) else { return nil }
        let pad = max(1, font * 0.35)
        let box = CGRect(x: 0, y: 0, width: width, height: height)
        let area = box.insetBy(dx: pad, dy: pad)
        guard area.width > 0, area.height > 0 else { return nil }
        let used = text(canvas, value, in: area, size: font, bold: true, centered: true)
        let top = CGFloat(height) - min(area.height, ceil(used)) - 2 * pad
        canvas.setBlendMode(.destinationOver)
        canvas.setFillColor(CGColor(srgbRed: 0, green: 0, blue: 0, alpha: 0.78))
        let plate = CGRect(x: 0, y: max(0, top), width: CGFloat(width), height: CGFloat(height) - max(0, top))
        canvas.addPath(CGPath(roundedRect: plate, cornerWidth: pad, cornerHeight: pad, transform: nil))
        canvas.fillPath()
        guard let image = canvas.makeImage() else { return nil }
        return CIImage(cgImage: image).transformed(by: .init(
            translationX: (size.width - CGFloat(width)) / 2, y: size.height * 0.055 - top
        ))
    }

    private static func place(_ image: CGImage, in target: CGRect, on context: CGContext) {
        let scale = min(target.width / CGFloat(image.width), target.height / CGFloat(image.height))
        let size = CGSize(width: CGFloat(image.width) * scale, height: CGFloat(image.height) * scale)
        context.draw(image, in: CGRect(x: target.midX - size.width / 2, y: target.midY - size.height / 2,
                                      width: size.width, height: size.height))
    }

    static func card(_ design: VideoCard, size: CGSize) throws -> CGImage {
        guard let context = bitmap(Int(size.width), Int(size.height)) else { throw VideoEditError.render }
        let w = size.width, h = size.height
        let bounds = CGRect(origin: .zero, size: size)
        let margin = w * 0.085
        let usable = w - 2 * margin
        let unit = min(w, h)
        let accent = design.accent.color
        let background = CGColor(srgbRed: 0.07, green: 0.10, blue: 0.17, alpha: 1)
        fill(context, bounds, background)
        let photo = design.image.flatMap { image($0) }
        if design.image != nil && photo == nil { throw VideoEditError.artwork }

        switch design.style {
        case .photo:
            if let photo {
                place(photo, in: CGRect(x: margin / 2, y: h * 0.24, width: w - margin, height: h * 0.72), on: context)
            }
            fill(context, CGRect(x: margin, y: h * 0.20, width: usable, height: max(2, unit * 0.004)), accent)
            text(context, design.title, in: CGRect(x: margin, y: h * 0.09, width: usable, height: h * 0.09), size: unit * 0.055, bold: true)
            text(context, design.body, in: CGRect(x: margin, y: h * 0.01, width: usable, height: h * 0.075), size: unit * 0.032)
        case .blank:
            if let photo { place(photo, in: bounds, on: context) }
            text(context, design.title, in: CGRect(x: margin, y: h * 0.48, width: usable, height: h * 0.40), size: unit * 0.09, bold: true)
            text(context, design.body, in: CGRect(x: margin, y: h * 0.08, width: usable, height: h * 0.37), size: unit * 0.045)
        default:
            let textWidth = photo == nil ? usable : usable * 0.56
            if let photo {
                place(photo, in: CGRect(x: margin + usable * 0.61, y: h * 0.15, width: usable * 0.39, height: h * 0.70), on: context)
            } else if design.style != .brief {
                context.setFillColor(accent.copy(alpha: 0.10) ?? accent)
                context.fillEllipse(in: CGRect(x: w * 0.70, y: h * 0.08, width: unit * 1.15, height: unit * 1.15))
                context.setStrokeColor(accent.copy(alpha: 0.20) ?? accent)
                context.setLineWidth(max(1, unit * 0.004))
                context.strokeEllipse(in: CGRect(x: w * 0.75, y: h * 0.12, width: unit * 1.05, height: unit * 1.05))
            }
            if design.style == .chapter {
                fill(context, CGRect(x: 0, y: 0, width: w * 0.028, height: h), accent)
            } else if design.style == .end {
                fill(context, CGRect(x: margin, y: h * 0.13, width: usable * 0.18, height: max(3, unit * 0.009)), accent)
            } else {
                fill(context, CGRect(x: margin, y: h * 0.78, width: usable * 0.15, height: max(3, unit * 0.009)), accent)
            }
            let brief = design.style == .brief
            let titleBottom = h * (brief ? 0.54 : 0.36)
            let titleTop = h * (design.style == .end ? 0.64 : 0.73)
            let titleHeight = text(context, design.title,
                                   in: CGRect(x: margin, y: titleBottom, width: textWidth, height: titleTop - titleBottom),
                                   size: unit * (brief ? 0.062 : 0.096), bold: true)
            let bodyTop = max(titleBottom - unit * 0.055, titleTop - titleHeight - unit * 0.055)
            let bodyBottom = h * 0.18
            text(context, design.body, in: CGRect(x: margin, y: bodyBottom, width: textWidth, height: bodyTop - bodyBottom),
                 size: unit * (brief ? 0.046 : 0.041))
        }
        guard let result = context.makeImage() else { throw VideoEditError.render }
        return result
    }

    static func spotlight(_ art: VideoArtwork, size: CGSize, progress: Double) -> CIImage {
        let target = art.spotlightEnd ?? VideoSpotlightPoint(x: art.x, y: art.y)
        let center = CIVector(x: size.width * (art.x + (target.x - art.x) * progress),
                              y: size.height * (1 - art.y - (target.y - art.y) * progress))
        let diameter = min(size.width, size.height) * art.width
        let radius = max(1, diameter / 2)
        let line = max(1, min(size.width, size.height) * 0.003)
        let bounds = CGRect(origin: .zero, size: size)
        func gradient(_ inner: Double, _ outer: Double, from: CIColor, to: CIColor) -> CIImage {
            CIFilter(name: "CIRadialGradient", parameters: [
                "inputCenter": center, "inputRadius0": inner, "inputRadius1": outer,
                "inputColor0": from, "inputColor1": to,
            ])?.outputImage?.cropped(to: bounds) ?? CIImage.empty()
        }
        let shade = gradient(radius, radius + max(line * 2, radius * 0.24),
                             from: .clear, to: CIColor(red: 0, green: 0, blue: 0, alpha: 0.66))
        let ink = CIColor(cgColor: art.ink.color.copy(alpha: 0.82) ?? art.ink.color)
        let ring = gradient(max(0, radius - line), radius, from: .clear, to: ink)
        let edge = gradient(radius, radius + line, from: .white, to: .clear)
        return ring.applyingFilter("CISourceInCompositing", parameters: [kCIInputBackgroundImageKey: edge])
            .composited(over: shade)
    }

    static func overlay(_ art: VideoArtwork, size: CGSize) throws -> CIImage {
        let width = max(1, Int(size.width * art.width))
        let graphics: CGImage
        if art.kind == .image {
            guard let data = art.image, let image = image(data) else { throw VideoEditError.artwork }
            graphics = image
        } else {
            let square = art.kind == .step || art.kind == .ellipse
            let height = square ? width : max(2, width / 2)
            guard let context = bitmap(width, height) else { throw VideoEditError.render }
            let w = Double(width), h = Double(height)
            let bounds = CGRect(x: 0, y: 0, width: w, height: h)
            let border = max(2, w * 0.025)
            let ink = art.ink.color
            context.setStrokeColor(ink)
            context.setFillColor(ink)
            context.setLineWidth(border)
            switch art.kind {
            case .text, .callout, .step:
                if art.kind == .callout {
                    context.addPath(CGPath(roundedRect: bounds, cornerWidth: border * 2, cornerHeight: border * 2, transform: nil))
                    context.fillPath()
                } else if art.kind == .step {
                    context.fillEllipse(in: bounds.insetBy(dx: border, dy: border))
                }
                let labelColor = art.kind == .text ? ink : (art.ink == .black ? VideoInk.white.color : VideoInk.black.color)
                let label = art.kind == .step ? String(art.text.prefix(3)) : art.text
                let box = art.kind == .step
                    ? CGRect(x: w * 0.27, y: h * 0.20, width: w * 0.66, height: h * 0.61)
                    : bounds.insetBy(dx: w * 0.04, dy: h * 0.09)
                text(context, label, in: box, size: art.kind == .step ? w * 0.46 : w * 0.115, bold: true, color: labelColor)
            case .arrow:
                context.setLineCap(.round)
                context.move(to: CGPoint(x: border, y: h * 0.18))
                context.addLine(to: CGPoint(x: w * 0.94, y: h * 0.86))
                context.strokePath()
                context.move(to: CGPoint(x: w * 0.66, y: h * 0.84))
                context.addLine(to: CGPoint(x: w * 0.95, y: h * 0.88))
                context.addLine(to: CGPoint(x: w * 0.89, y: h * 0.55))
                context.closePath()
                context.fillPath()
            case .outline: context.stroke(bounds.insetBy(dx: border, dy: border))
            case .ellipse: context.strokeEllipse(in: bounds.insetBy(dx: border, dy: border))
            case .highlight: fill(context, bounds, ink.copy(alpha: 0.32) ?? ink)
            case .image, .spotlight, .frameEffects: break
            }
            guard let image = context.makeImage() else { throw VideoEditError.render }
            graphics = image
        }
        let factor = CGFloat(width) / CGFloat(graphics.width)
        let tall = CGFloat(graphics.height) * factor
        return CIImage(cgImage: graphics)
            .transformed(by: CGAffineTransform(scaleX: factor, y: factor))
            .transformed(by: CGAffineTransform(translationX: size.width * art.x, y: size.height * (1 - art.y) - tall))
    }
}

struct VideoEditMedia {
    let asset: AVURLAsset
    let video: AVAssetTrack
    let range: CMTimeRange
    let audio: [(AVAssetTrack, CMTimeRange)]
    let transform: CGAffineTransform
    let size: CGSize
    let duration: Int64
    let frame: CMTime

    static func load(_ url: URL) async throws -> Self {
        let asset = AVURLAsset(url: url)
        guard let video = try await asset.loadTracks(withMediaType: .video).first else { throw VideoEditError.noVideo }
        let (range, natural, transform, rate) = try await video.load(.timeRange, .naturalSize, .preferredTransform, .nominalFrameRate)
        let displayed = CGRect(origin: .zero, size: natural).applying(transform).standardized
        let milliseconds = range.duration.seconds * 1_000
        guard milliseconds.isFinite, (1...Double(VideoTime.maximum)).contains(milliseconds),
              displayed.width.isFinite, displayed.height.isFinite, min(displayed.width, displayed.height) >= 2
        else { throw VideoEditError.noVideo }
        let scale = min(1, 4096 / max(displayed.width, displayed.height))
        let size = CGSize(width: max(2, floor(displayed.width * scale / 2) * 2),
                          height: max(2, floor(displayed.height * scale / 2) * 2))
        let normalized = transform.concatenating(.init(translationX: -displayed.minX, y: -displayed.minY))
            .concatenating(.init(scaleX: size.width / displayed.width, y: size.height / displayed.height))
        let fps = rate.isFinite && rate >= 1 ? min(Double(rate), 60) : 30
        let frame = CMTime(seconds: 1 / fps, preferredTimescale: 60_000)
        var audio: [(AVAssetTrack, CMTimeRange)] = []
        for track in try await asset.loadTracks(withMediaType: .audio) {
            audio.append((track, try await track.load(.timeRange)))
        }
        return Self(asset: asset, video: video, range: range, audio: audio, transform: normalized,
                    size: size, duration: Int64(milliseconds), frame: frame)
    }
}

private struct RenderOverlay {
    enum Content {
        case image(CIImage)
        case spotlight(VideoArtwork, VideoSpotlightTiming?)
        case effects(Data, BedizFrameRenderer)
    }
    var start: CMTime
    var end: CMTime
    var content: Content

    func apply(over frame: CIImage, at time: CMTime, bounds: CGRect) throws -> CIImage {
        switch content {
        case .image(let image):
            return image.composited(over: frame)
        case .spotlight(let art, let motion):
            let progress = motion.map { track in
                min(1, max(0, ((time - start).seconds * 1_000 + Double(track.offset)) / Double(track.length)))
            } ?? 0
            return VideoDesign.spotlight(art, size: bounds.size, progress: progress).composited(over: frame)
        case .effects(let program, let render):
            return try FrameEffects.apply(program, using: render, to: frame, bounds: bounds)
        }
    }
}

private enum FrameEffects {
    private static let colorSpace = CGColorSpace(name: CGColorSpace.sRGB)!
    private static let context = CIContext(options: [.cacheIntermediates: false])

    static func apply(_ program: Data, using render: BedizFrameRenderer, to frame: CIImage, bounds: CGRect) throws -> CIImage {
        guard let source = context.createCGImage(frame, from: bounds, format: .RGBA8, colorSpace: colorSpace)
        else { throw VideoEditError.render }
        let (width, height) = (source.width, source.height)
        let bytesPerRow = width * 4
        let rgba = CGBitmapInfo.byteOrder32Big.rawValue | CGImageAlphaInfo.noneSkipLast.rawValue
        let resultInfo = CGBitmapInfo.byteOrder32Big.rawValue | CGImageAlphaInfo.last.rawValue
        var pixels = Data(count: bytesPerRow * height)
        let rendered = pixels.withUnsafeMutableBytes { (buffer: UnsafeMutableRawBufferPointer) -> Bool in
            guard let mutable = buffer.bindMemory(to: UInt8.self).baseAddress,
                  let bitmap = CGContext(data: mutable, width: width, height: height, bitsPerComponent: 8,
                                         bytesPerRow: bytesPerRow, space: colorSpace, bitmapInfo: rgba) else { return false }
            bitmap.draw(source, in: bounds)
            // The RGBX context leaves the padding byte unspecified. The source is an opaque video frame.
            for index in stride(from: 3, to: buffer.count, by: 4) { mutable[index] = 255 }
            return program.withUnsafeBytes { data in
                guard let commands = data.bindMemory(to: UInt8.self).baseAddress else { return false }
                return render(commands, data.count, mutable, buffer.count, UInt32(width), UInt32(height))
            }
        }
        guard rendered, let provider = CGDataProvider(data: pixels as CFData),
              let image = CGImage(width: width, height: height, bitsPerComponent: 8, bitsPerPixel: 32,
                                  bytesPerRow: bytesPerRow, space: colorSpace, bitmapInfo: CGBitmapInfo(rawValue: resultInfo),
                                  provider: provider, decode: nil, shouldInterpolate: false, intent: .defaultIntent)
        else { throw VideoEditError.render }
        return CIImage(cgImage: image)
    }
}

private final class VideoEditInstruction: NSObject, AVVideoCompositionInstructionProtocol, @unchecked Sendable {
    let timeRange: CMTimeRange
    let enablePostProcessing = false
    let containsTweening = true
    let requiredSourceTrackIDs: [NSValue]?
    let passthroughTrackID = kCMPersistentTrackID_Invalid
    let track: CMPersistentTrackID
    let card: CIImage?
    let overlays: [RenderOverlay]
    let transform: CGAffineTransform
    let bounds: CGRect
    let transition: VideoTransition
    let transitionLength: Double
    let previous: CIImage?
    let outgoingDip: Double
    let framing: VideoFraming
    let subtitles: [VideoSubtitleCue]

    init(range: CMTimeRange, track: CMPersistentTrackID, card: CIImage?, overlays: [RenderOverlay],
         transform: CGAffineTransform, size: CGSize, transition: VideoTransition, transitionLength: Double,
         previous: CIImage?, outgoingDip: Double, framing: VideoFraming, subtitles: [VideoSubtitleCue]) {
        self.timeRange = range
        self.track = track
        self.requiredSourceTrackIDs = [NSNumber(value: track)]
        self.card = card
        self.overlays = overlays
        self.transform = transform
        self.bounds = CGRect(origin: .zero, size: size)
        self.transition = transition
        self.transitionLength = transitionLength
        self.previous = previous
        self.outgoingDip = outgoingDip
        self.framing = framing
        self.subtitles = subtitles
    }

    func frame(source: CIImage?, at time: CMTime) throws -> CIImage {
        let black = CIImage(color: .black).cropped(to: bounds)
        var image = card ?? source?.transformed(by: transform) ?? black
        for overlay in overlays where time >= overlay.start && time < overlay.end {
            image = try overlay.apply(over: image, at: time, bounds: bounds)
        }
        return image.composited(over: black).cropped(to: bounds)
    }

    func render(source: CVPixelBuffer?, at time: CMTime) throws -> CIImage {
        let local = time - timeRange.start
        let seconds = max(0, local.seconds)
        let black = CIImage(color: .black).cropped(to: bounds)
        let picture = try frame(source: source.map(CIImage.init(cvPixelBuffer:)), at: local)
        if seconds < transitionLength {
            let progress = seconds / transitionLength
            let lead = previous ?? black
            switch transition {
            case .cut: break
            case .dissolve, .dip:
                let alpha = picture.applyingFilter("CIColorMatrix", parameters: [
                    "inputAVector": CIVector(x: 0, y: 0, z: 0, w: progress),
                ])
                return alpha.composited(over: transition == .dip ? black : lead)
            case .wipe:
                let showing = CGRect(x: 0, y: 0, width: bounds.width * progress, height: bounds.height)
                return progress <= 0 ? lead : picture.cropped(to: showing).composited(over: lead)
            case .slide:
                let shifted = picture.transformed(by: .init(translationX: bounds.width * (1 - progress), y: 0))
                return shifted.composited(over: lead).cropped(to: bounds)
            }
        }
        let remaining = timeRange.duration.seconds - seconds
        if outgoingDip > 0 && remaining < outgoingDip {
            return picture.applyingFilter("CIColorMatrix", parameters: [
                "inputAVector": CIVector(x: 0, y: 0, z: 0, w: max(0, remaining / outgoingDip)),
            ]).composited(over: black)
        }
        return picture
    }
}

final class VideoEditCompositor: NSObject, AVVideoCompositing {
    private let context = CIContext(options: [.cacheIntermediates: false])
    private let colorSpace = CGColorSpace(name: CGColorSpace.sRGB)!
    // NSCache serializes concurrent lookups and eviction.
    nonisolated(unsafe) private let captions: NSCache<NSString, CIImage> = {
        let cache = NSCache<NSString, CIImage>()
        cache.totalCostLimit = 16 * 1_024 * 1_024
        return cache
    }()
    var sourcePixelBufferAttributes: [String: Any]? {
        [kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32BGRA]
    }
    var requiredPixelBufferAttributesForRenderContext: [String: Any] {
        [kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32BGRA]
    }

    func renderContextChanged(_ newRenderContext: AVVideoCompositionRenderContext) {}

    func startRequest(_ request: AVAsynchronousVideoCompositionRequest) {
        guard let instruction = request.videoCompositionInstruction as? VideoEditInstruction,
              let output = request.renderContext.newPixelBuffer() else {
            request.finish(with: VideoEditError.render)
            return
        }
        do {
            var image = try instruction.render(source: request.sourceFrame(byTrackID: instruction.track), at: request.compositionTime)
            let bounds = CGRect(x: 0, y: 0, width: CVPixelBufferGetWidth(output), height: CVPixelBufferGetHeight(output))
            if bounds != instruction.bounds {
                let black = CIImage(color: .black).cropped(to: bounds)
                image = image.transformed(by: instruction.framing.transform(from: instruction.bounds, to: bounds))
                    .cropped(to: bounds).composited(over: black)
            }
            let local = request.compositionTime - instruction.timeRange.start
            let ms = CMTimeConvertScale(local, timescale: 1_000, method: .roundTowardZero).value
            if let cue = VideoSubtitleCue.active(in: instruction.subtitles, at: ms) {
                let key = "\(Int(bounds.width))x\(Int(bounds.height)):\(cue.text)" as NSString
                if let subtitle = captions.object(forKey: key) {
                    image = subtitle.composited(over: image)
                } else if let subtitle = VideoDesign.subtitle(cue.text, size: bounds.size) {
                    captions.setObject(subtitle, forKey: key, cost: Int(subtitle.extent.width * subtitle.extent.height * 4))
                    image = subtitle.composited(over: image)
                }
            }
            context.render(image, to: output, bounds: bounds, colorSpace: colorSpace)
            request.finish(withComposedVideoFrame: output)
        } catch {
            request.finish(with: error)
        }
    }
}

struct VideoEditRender {
    let asset: AVMutableComposition
    let video: AVMutableVideoComposition
    let audioMix: AVAudioMix?

    static func build(project: VideoEditProject, media: VideoEditMedia, frameRenderer: BedizFrameRenderer? = nil,
                      export: VideoExportOptions? = nil) async throws -> Self {
        try project.validate(sourceDuration: media.duration)
        let asset = AVMutableComposition()
        guard let track = asset.addMutableTrack(withMediaType: .video, preferredTrackID: kCMPersistentTrackID_Invalid) else {
            throw VideoEditError.noVideo
        }
        let fadesUsed = project.clips.contains { $0.audioFades != nil }
        let audio = try media.audio.map { source, range in
            guard let output = asset.addMutableTrack(withMediaType: .audio, preferredTrackID: kCMPersistentTrackID_Invalid) else {
                throw VideoEditError.noExport
            }
            let parameters = fadesUsed ? AVMutableAudioMixInputParameters(track: output) : nil
            return (source, range, output, parameters)
        }
        let stage = AVAssetImageGenerator(asset: media.asset)
        stage.requestedTimeToleranceBefore = media.frame
        stage.requestedTimeToleranceAfter = media.frame

        let entries = project.entries()
        func boundary(_ index: Int) -> Double {
            guard index > 0, index < entries.count, entries[index].clip.transition != .cut else { return 0 }
            let clip = entries[index].clip
            return Double(min(clip.transitionLength, clip.length / 2, entries[index - 1].clip.length / 2)) / 1_000
        }
        var instructions: [VideoEditInstruction] = []
        for (index, entry) in entries.enumerated() {
            try Task.checkCancellation()
            let clip = entry.clip
            let time = videoTime(entry.offset)
            let duration = videoTime(clip.length)
            let range = CMTimeRange(start: time, duration: duration)
            let still: CIImage?
            if let frozen = clip.freeze {
                guard let image = VideoDesign.image(frozen) else { throw VideoEditError.artwork }
                still = CIImage(cgImage: image).transformed(by: CGAffineTransform(
                    scaleX: media.size.width / CGFloat(image.width), y: media.size.height / CGFloat(image.height)
                ))
            } else {
                still = try clip.card.map { CIImage(cgImage: try VideoDesign.card($0, size: media.size)) }
            }
            let overlays = try clip.overlays.map { item in
                let content: RenderOverlay.Content
                switch item.art.kind {
                case .spotlight:
                    content = .spotlight(item.art, item.motion)
                case .frameEffects:
                    guard let program = item.art.image, let frameRenderer else { throw VideoEditError.artwork }
                    content = .effects(program, frameRenderer)
                default:
                    content = .image(try VideoDesign.overlay(item.art, size: media.size))
                }
                return RenderOverlay(start: videoTime(item.start), end: videoTime(item.end), content: content)
            }
            if clip.isStill {
                // The source sample supplies timing. The compositor supplies the still's pixels.
                let sample = min(media.frame, media.range.duration)
                try track.insertTimeRange(CMTimeRange(start: media.range.start, duration: sample), of: media.video, at: time)
                track.scaleTimeRange(CMTimeRange(start: time, duration: sample), toDuration: duration)
            } else {
                let sourceRange = CMTimeRange(start: media.range.start + videoTime(clip.sourceStart), duration: duration)
                try track.insertTimeRange(sourceRange, of: media.video, at: time)
                for (source, available, target, parameters) in audio {
                    let intersection = sourceRange.intersection(available)
                    if intersection.isValid && !intersection.isEmpty {
                        let localStart = intersection.start - sourceRange.start
                        try target.insertTimeRange(intersection, of: source, at: time + localStart)
                        if let parameters {
                            applyFades(clip.audioFades, to: parameters, at: time,
                                       localStart: localStart, localEnd: localStart + intersection.duration)
                        }
                    }
                }
            }

            let transitionLength = boundary(index)
            var previous: CIImage?
            if transitionLength > 0, clip.transition != .dip, let earlier = instructions.last {
                let before = entries[index - 1].clip
                let lastFrame = max(.zero, videoTime(before.length) - media.frame)
                var source: CIImage?
                if !before.isStill {
                    let still = try await stage.image(at: media.range.start + videoTime(before.sourceStart) + lastFrame)
                    source = CIImage(cgImage: still.image)
                }
                previous = try earlier.frame(source: source, at: lastFrame)
            }
            let outgoingDip = index + 1 < entries.count && entries[index + 1].clip.transition == .dip ? boundary(index + 1) : 0
            instructions.append(VideoEditInstruction(
                range: range, track: track.trackID, card: still, overlays: overlays, transform: media.transform,
                size: media.size, transition: clip.transition, transitionLength: transitionLength,
                previous: previous, outgoingDip: outgoingDip, framing: export?.framing ?? .fit,
                subtitles: export?.includeSubtitles == false ? [] : (clip.subtitles ?? [])
            ))
        }
        let mix = AVMutableAudioMix()
        var parameters: [AVAudioMixInputParameters] = []
        for (_, _, track, volume) in audio {
            if track.segments.allSatisfy(\.isEmpty) { asset.removeTrack(track) }
            else if let volume { parameters.append(volume) }
        }
        mix.inputParameters = parameters

        let video = AVMutableVideoComposition()
        video.customVideoCompositorClass = VideoEditCompositor.self
        video.instructions = instructions
        video.renderSize = export?.size ?? media.size
        video.frameDuration = media.frame
        return Self(asset: asset, video: video, audioMix: parameters.isEmpty ? nil : mix)
    }

    private static func applyFades(
        _ fades: VideoAudioFades?, to mix: AVMutableAudioMixInputParameters, at time: CMTime,
        localStart: CMTime, localEnd: CMTime
    ) {
        var points = [localStart, localEnd]
        if let fades {
            for point in [fades.fadeIn, fades.length - fades.fadeOut] {
                let local = videoTime(point - fades.offset)
                if localStart < local, local < localEnd { points.append(local) }
            }
            points.sort()
        }
        func level(_ point: CMTime) -> Float {
            guard let fades else { return 1 }
            let elapsed = point.seconds * 1_000 + Double(fades.offset)
            let gainIn = fades.fadeIn == 0 ? 1 : elapsed / Double(fades.fadeIn)
            let gainOut = fades.fadeOut == 0 ? 1 : (Double(fades.length) - elapsed) / Double(fades.fadeOut)
            return Float(max(0, min(1, gainIn, gainOut)))
        }
        for (start, end) in zip(points, points.dropFirst()) where start < end {
            mix.setVolumeRamp(fromStartVolume: level(start), toEndVolume: level(end),
                              timeRange: CMTimeRange(start: time + start, duration: end - start))
        }
    }

    func playerItem() -> AVPlayerItem {
        let item = AVPlayerItem(asset: asset)
        item.videoComposition = video
        item.audioMix = audioMix
        return item
    }

    func still(at time: Int64) async throws -> CGImage {
        let generator = AVAssetImageGenerator(asset: asset)
        generator.videoComposition = video
        generator.requestedTimeToleranceBefore = .zero
        generator.requestedTimeToleranceAfter = .zero
        return try await generator.image(at: videoTime(time)).image
    }

}

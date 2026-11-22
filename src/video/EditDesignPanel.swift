import AppKit
import CoreImage
import UniformTypeIdentifiers

typealias BedizArtworkRenderer = @convention(c) (UnsafePointer<CChar>, UnsafePointer<CChar>) -> Bool

@MainActor
enum VideoEditUI {
    static let accent = NSColor(srgbRed: 0.34, green: 0.48, blue: 0.99, alpha: 1)
    static let muted = NSColor(srgbRed: 0.68, green: 0.73, blue: 0.82, alpha: 1)

    static func row(_ views: [NSView], spacing: CGFloat = 8) -> NSStackView {
        stack(views, spacing: spacing, vertical: false)
    }

    static func column(_ views: [NSView], spacing: CGFloat = 8) -> NSStackView {
        stack(views, spacing: spacing, vertical: true)
    }

    private static func stack(_ views: [NSView], spacing: CGFloat, vertical: Bool) -> NSStackView {
        let stack = NSStackView(views: views)
        stack.orientation = vertical ? .vertical : .horizontal
        stack.alignment = vertical ? .width : .centerY
        stack.spacing = spacing
        stack.detachesHiddenViews = true
        return stack
    }

    static func button(_ title: String, _ icon: String, _ target: AnyObject,
                       _ action: Selector, text: Bool = false, primary: Bool = false) -> NSButton {
        let control = NSButton(title: text ? title : "", target: target, action: action)
        control.bezelStyle = .rounded
        control.controlSize = text ? .regular : .small
        control.image = NSImage(systemSymbolName: icon, accessibilityDescription: nil)
        control.imagePosition = text ? .imageLeading : .imageOnly
        control.symbolConfiguration = .init(pointSize: 12, weight: .semibold)
        control.toolTip = title
        control.setAccessibilityLabel(title)
        if primary { control.bezelColor = accent }
        return control
    }

    static func label(_ value: String, quiet: Bool = false) -> NSTextField {
        let field = NSTextField(labelWithString: value)
        field.font = .systemFont(ofSize: 12, weight: quiet ? .regular : .medium)
        if quiet { field.textColor = muted }
        field.lineBreakMode = .byTruncatingTail
        return field
    }

    static func timeField(_ label: String, value: Int64, width: CGFloat = 104) -> NSTextField {
        let field = NSTextField(string: VideoTime.format(value))
        field.setAccessibilityLabel(label)
        field.font = .monospacedDigitSystemFont(ofSize: 12, weight: .medium)
        field.alignment = .center
        field.widthAnchor.constraint(equalToConstant: width).isActive = true
        return field
    }

    static func popup<T: RawRepresentable & Equatable>(_ values: [T], selected: T,
                                                      label: String, target: AnyObject? = nil,
                                                      action: Selector? = nil) -> NSPopUpButton where T.RawValue == String {
        let control = NSPopUpButton(frame: .zero, pullsDown: false)
        control.addItems(withTitles: values.map(\.rawValue))
        control.selectItem(withTitle: selected.rawValue)
        control.setAccessibilityLabel(label)
        control.target = target
        control.action = action
        return control
    }

    static func gap() -> NSView {
        let view = NSView()
        view.setContentHuggingPriority(.defaultLow, for: .horizontal)
        return view
    }
}

@MainActor
private final class ArtworkPreview: NSImageView {
    var pick: ((VideoSpotlightPoint) -> Void)?

    override func mouseDown(with event: NSEvent) {
        guard pick != nil else { return super.mouseDown(with: event) }
        place(from: event)
    }

    override func mouseDragged(with event: NSEvent) {
        guard pick != nil else { return super.mouseDragged(with: event) }
        place(from: event)
    }

    private func place(from event: NSEvent) {
        guard let size = image?.size, size.width > 0, size.height > 0 else { return }
        let scale = min(bounds.width / size.width, bounds.height / size.height)
        let width = size.width * scale, height = size.height * scale
        guard width > 0, height > 0 else { return }
        let point = convert(event.locationInWindow, from: nil)
        let x = (point.x - bounds.midX + width / 2) / width
        let y = (bounds.midY + height / 2 - point.y) / height
        pick?(VideoSpotlightPoint(x: min(1, max(0, x)), y: min(1, max(0, isFlipped ? 1 - y : y))))
    }
}

@MainActor
final class VideoDesignPanel: NSObject, NSTextFieldDelegate, NSTextViewDelegate {
    enum Design {
        case page(VideoCard, length: Int64)
        case overlay(VideoArtwork, start: Int64, length: Int64)
    }

    private let sheet = NSPanel()
    private let preview = ArtworkPreview()
    private let titleField = NSTextField()
    private let bodyField = NSTextView()
    private let bodyScroll = NSScrollView()
    private let notice = VideoEditUI.label("", quiet: true)
    private let length: NSTextField
    private let start: NSTextField?
    private let style: NSPopUpButton
    private let ink: NSPopUpButton
    private var imageData: Data?
    private let isPage: Bool
    private let x: NSSlider
    private let y: NSSlider
    private let width: NSSlider
    private let canvas: CGSize
    private let still: CGImage?
    private let endpointStill: (Int64) async -> CGImage?
    private let artworkRenderer: BedizArtworkRenderer?
    private let apply: (Design) -> String?
    private let ci = CIContext()
    private var lastCardStyle: VideoCardStyle?
    private let motionToggle = NSButton(checkboxWithTitle: "Move to a second point", target: nil, action: nil)
    private let motionPoint = NSSegmentedControl(labels: ["Start", "End"], trackingMode: .selectOne, target: nil, action: nil)
    private var imageActions: NSStackView?
    private var spotlightActions: NSStackView?
    private var widthLabel: NSTextField?
    private var contentStack: NSStackView?
    private var startPoint: VideoSpotlightPoint?
    private var endPoint: VideoSpotlightPoint?
    private var lastKind: VideoArtworkKind?
    private var pointingAtEnd = false
    private var endStill: CGImage?
    private var endStillTask: Task<Void, Never>?

    init(design: Design, canvas: CGSize, still: CGImage?, artworkRenderer: BedizArtworkRenderer?,
         endpointStill: @escaping (Int64) async -> CGImage?,
         apply: @escaping (Design) -> String?) {
        self.canvas = canvas
        self.still = still
        self.endpointStill = endpointStill
        self.artworkRenderer = artworkRenderer
        self.apply = apply

        let selected: String
        let options: [String]
        let color: VideoInk
        switch design {
        case .page(let card, let milliseconds):
            isPage = true
            start = nil
            length = VideoEditUI.timeField("Page duration", value: milliseconds)
            selected = card.style.rawValue
            options = VideoCardStyle.allCases.map(\.rawValue)
            color = card.accent
            imageData = card.image
            lastCardStyle = card.style
            x = NSSlider()
            y = NSSlider()
            width = NSSlider()
        case .overlay(let art, let offset, let milliseconds):
            isPage = false
            start = VideoEditUI.timeField("Annotation start", value: offset)
            length = VideoEditUI.timeField("Annotation duration", value: milliseconds)
            selected = art.kind.rawValue
            options = VideoArtworkKind.allCases.filter { $0 != .frameEffects }.map(\.rawValue)
            color = art.ink
            imageData = art.image
            if art.kind == .spotlight {
                startPoint = VideoSpotlightPoint(x: art.x, y: art.y)
                endPoint = art.spotlightEnd
            }
            x = NSSlider(value: art.x, minValue: 0, maxValue: 1, target: nil, action: nil)
            y = NSSlider(value: art.y, minValue: 0, maxValue: 1, target: nil, action: nil)
            width = NSSlider(value: art.width, minValue: 0.04, maxValue: 1, target: nil, action: nil)
        }
        style = NSPopUpButton(frame: .zero, pullsDown: false)
        style.addItems(withTitles: options)
        style.selectItem(withTitle: selected)
        ink = VideoEditUI.popup(VideoInk.allCases, selected: color, label: "Color")
        super.init()
        start?.delegate = self
        length.delegate = self
        motionToggle.target = self
        motionToggle.action = #selector(changeMotion)
        motionToggle.state = endPoint == nil ? .off : .on
        motionPoint.target = self
        motionPoint.action = #selector(changeMotionPoint)
        motionPoint.selectedSegment = 0
        motionPoint.setAccessibilityLabel("Spotlight point")

        switch design {
        case .page(let card, _):
            titleField.stringValue = card.title
            bodyField.string = card.body
        case .overlay(let art, _, _):
            titleField.stringValue = art.text
        }
        for control in [style, ink, x, y, width] as [NSControl] {
            control.target = self
            control.action = #selector(refresh)
        }
        style.setAccessibilityLabel(isPage ? "Page template" : "Annotation type")
        for (control, label) in [(x, "Horizontal position"), (y, "Vertical position"), (width, "Width")] {
            control.setAccessibilityLabel(label)
            control.toolTip = label
            control.isContinuous = true
        }
        titleField.placeholderString = isPage ? "Title" : "Annotation text"
        titleField.setAccessibilityLabel(isPage ? "Title" : "Annotation text")
        titleField.delegate = self
        bodyField.setAccessibilityLabel("Page text")
        bodyField.font = .systemFont(ofSize: 13)
        bodyField.isRichText = false
        bodyField.delegate = self
        bodyScroll.borderType = .bezelBorder
        bodyScroll.documentView = bodyField
        bodyScroll.hasVerticalScroller = true
        bodyScroll.heightAnchor.constraint(equalToConstant: 63).isActive = true

        preview.imageScaling = .scaleProportionallyUpOrDown
        preview.wantsLayer = true
        preview.layer?.cornerRadius = 8
        preview.layer?.backgroundColor = NSColor.black.withAlphaComponent(0.45).cgColor
        preview.heightAnchor.constraint(equalToConstant: 220).isActive = true
        preview.setAccessibilityLabel(isPage ? "Page preview" : "Annotation preview")
        sheet.styleMask = [.titled]
        sheet.title = isPage ? "Design a page" : "Timed annotation"

        let root = NSView()
        root.appearance = NSAppearance(named: .darkAqua)
        root.wantsLayer = true
        root.layer?.backgroundColor = NSColor(srgbRed: 0.11, green: 0.14, blue: 0.20, alpha: 1).cgColor
        sheet.contentView = root

        let template = VideoEditUI.row([VideoEditUI.label(isPage ? "Template" : "Object"), style,
                                       VideoEditUI.gap(), VideoEditUI.label("Color"), ink])
        let imageActions = VideoEditUI.row([
            VideoEditUI.button("Choose artwork", "photo.on.rectangle", self, #selector(chooseImage), text: true),
            VideoEditUI.button("Paste image", "doc.on.clipboard", self, #selector(pasteImage)),
            VideoEditUI.button("Remove image", "xmark.circle", self, #selector(removeImage)),
            VideoEditUI.gap(),
        ])
        self.imageActions = imageActions
        var rows = [template, preview, titleField as NSView]
        if isPage {
            rows.append(bodyScroll)
        } else {
            let sizeLabel = VideoEditUI.label("Width")
            widthLabel = sizeLabel
            rows.append(VideoEditUI.row([VideoEditUI.label("Left"), x, VideoEditUI.label("Top"), y,
                                        sizeLabel, width]))
            NSLayoutConstraint.activate([
                x.widthAnchor.constraint(equalTo: y.widthAnchor),
                y.widthAnchor.constraint(equalTo: width.widthAnchor),
            ])
            let controls = VideoEditUI.row([
                motionToggle, motionPoint, VideoEditUI.gap(), VideoEditUI.label("Click the preview to place it.", quiet: true),
            ])
            spotlightActions = controls
            controls.isHidden = true
        }
        rows.append(imageActions)
        if let spotlightActions { rows.append(spotlightActions) }
        var times: [NSView] = []
        if let start { times += [VideoEditUI.label("Start"), start] }
        times += [VideoEditUI.label("Duration"), length, VideoEditUI.gap()]
        rows.append(VideoEditUI.row(times))
        let confirm = VideoEditUI.button("Apply", "checkmark", self, #selector(confirmDesign), text: true, primary: true)
        confirm.keyEquivalent = "\r"
        let cancel = VideoEditUI.button("Cancel", "xmark", self, #selector(cancelDesign), text: true)
        cancel.keyEquivalent = "\u{1b}"
        rows.append(VideoEditUI.row([notice, VideoEditUI.gap(), cancel, confirm]))

        let content = VideoEditUI.column(rows, spacing: 10)
        contentStack = content
        content.translatesAutoresizingMaskIntoConstraints = false
        root.addSubview(content)
        NSLayoutConstraint.activate([
            content.widthAnchor.constraint(equalToConstant: 536),
            content.leadingAnchor.constraint(equalTo: root.leadingAnchor, constant: 16),
            content.trailingAnchor.constraint(equalTo: root.trailingAnchor, constant: -16),
            content.topAnchor.constraint(equalTo: root.topAnchor, constant: 16),
            content.bottomAnchor.constraint(equalTo: root.bottomAnchor, constant: -16),
        ])
        sizeSheet()
        refresh()
    }

    func show(in parent: NSWindow, completion: @escaping () -> Void) {
        parent.beginSheet(sheet) { _ in completion() }
    }

    func controlTextDidChange(_ obj: Notification) {
        if pointingAtEnd && ((obj.object as? NSTextField) === length || (obj.object as? NSTextField) === start) {
            loadEndStill()
        }
        refresh()
    }
    func textDidChange(_ notification: Notification) { refresh() }

    private var selectedInk: VideoInk { VideoInk(rawValue: ink.titleOfSelectedItem ?? "") ?? .white }

    private var card: VideoCard {
        let style = VideoCardStyle(rawValue: style.titleOfSelectedItem ?? "") ?? .title
        var result = VideoCard(style: style)
        result.title = titleField.stringValue
        result.body = bodyField.string
        result.accent = selectedInk
        result.image = imageData
        return result
    }

    private var artwork: VideoArtwork {
        let kind = VideoArtworkKind(rawValue: style.titleOfSelectedItem ?? "") ?? .text
        let spotlight = kind == .spotlight
        return VideoArtwork(kind: kind, text: titleField.stringValue, image: kind == .image ? imageData : nil,
                            ink: selectedInk, x: spotlight ? startPoint?.x ?? 0.5 : min(x.doubleValue, 1 - width.doubleValue),
                            y: spotlight ? startPoint?.y ?? 0.5 : y.doubleValue, width: width.doubleValue,
                            spotlightEnd: spotlight && motionToggle.state == .on ? endPoint : nil)
    }

    @objc private func refresh() {
        if isPage, let selected = VideoCardStyle(rawValue: style.titleOfSelectedItem ?? ""), selected != lastCardStyle {
            if let defaults = lastCardStyle?.defaults {
                if titleField.stringValue == defaults.0 { titleField.stringValue = selected.defaults.0 }
                if bodyField.string == defaults.1 { bodyField.string = selected.defaults.1 }
            }
            lastCardStyle = selected
        }
        if !isPage { updateSpotlightControls() }
        titleField.isEnabled = isPage || artwork.kind.usesText
        let scale = min(576 / canvas.width, 360 / canvas.height)
        let size = CGSize(width: max(2, floor(canvas.width * scale)), height: max(2, floor(canvas.height * scale)))
        let display: CGImage?
        if isPage {
            display = try? VideoDesign.card(card, size: size)
        } else {
            let bounds = CGRect(origin: .zero, size: size)
            let black = CIImage(color: .black).cropped(to: bounds)
            let background = (pointingAtEnd ? endStill ?? still : still).map {
                CIImage(cgImage: $0).transformed(by: .init(scaleX: size.width / CGFloat($0.width),
                                                          y: size.height / CGFloat($0.height)))
            } ?? black
            let art = artwork.kind == .spotlight
                ? VideoDesign.spotlight(artwork, size: size, progress: pointingAtEnd ? 1 : 0)
                : try? VideoDesign.overlay(artwork, size: size)
            display = ci.createCGImage(art?.composited(over: background) ?? background, from: bounds)
        }
        preview.image = display.map { NSImage(cgImage: $0, size: size) }
    }

    private func updateSpotlightControls() {
        let kind = artwork.kind
        let active = kind == .spotlight
        if active && lastKind != .spotlight {
            let point = startPoint ?? VideoSpotlightPoint(x: 0.5, y: 0.5)
            x.doubleValue = point.x
            y.doubleValue = point.y
        } else if !active && lastKind == .spotlight {
            pointingAtEnd = false
            motionPoint.selectedSegment = 0
            endStillTask?.cancel()
        }
        lastKind = kind
        imageActions?.isHidden = active
        spotlightActions?.isHidden = !active
        if titleField.isHidden != active {
            titleField.isHidden = active
            sizeSheet()
        }
        widthLabel?.stringValue = active ? "Diameter" : "Width"
        width.setAccessibilityLabel(active ? "Spotlight diameter" : "Width")
        motionPoint.isEnabled = motionToggle.state == .on
        if active {
            let point = VideoSpotlightPoint(x: x.doubleValue, y: y.doubleValue)
            if pointingAtEnd { endPoint = point } else { startPoint = point }
            preview.pick = { [weak self] point in
                guard let self else { return }
                self.x.doubleValue = point.x
                self.y.doubleValue = point.y
                self.refresh()
            }
        } else {
            preview.pick = nil
        }
    }

    private func sizeSheet() {
        if let contentStack {
            sheet.setContentSize(NSSize(width: 568, height: contentStack.fittingSize.height + 32))
        }
    }

    @objc private func changeMotion() {
        if motionToggle.state == .on, endPoint == nil {
            endPoint = startPoint
        } else if motionToggle.state == .off {
            motionPoint.selectedSegment = 0
        }
        changeMotionPoint()
    }

    @objc private func changeMotionPoint() {
        pointingAtEnd = motionToggle.state == .on && motionPoint.selectedSegment == 1
        if let point = pointingAtEnd ? endPoint : startPoint {
            x.doubleValue = point.x
            y.doubleValue = point.y
        }
        if pointingAtEnd { loadEndStill() } else { endStillTask?.cancel() }
        refresh()
    }

    private func loadEndStill() {
        endStillTask?.cancel()
        guard let start, let offset = VideoTime.parse(start.stringValue),
              let duration = VideoTime.parse(length.stringValue), duration > 0 else { return }
        endStillTask = Task { [weak self] in
            let image = await self?.endpointStill(offset + duration - 1)
            guard !Task.isCancelled, let self, self.pointingAtEnd else { return }
            self.endStill = image
            self.refresh()
        }
    }

    private func useImage(_ data: Data?) {
        guard let data, let image = VideoDesign.image(data), let png = VideoDesign.png(image),
              png.count <= 32 * 1_024 * 1_024 else {
            notice.stringValue = "This artwork couldn’t be loaded."
            return
        }
        imageData = png
        if !isPage { style.selectItem(withTitle: VideoArtworkKind.image.rawValue) }
        notice.stringValue = ""
        refresh()
    }

    @objc private func chooseImage() {
        let dialog = NSOpenPanel()
        dialog.allowedContentTypes = [.image, UTType(filenameExtension: "myimage") ?? .data]
        dialog.allowsMultipleSelection = false
        guard dialog.runModal() == .OK, let url = dialog.url else { return }
        if url.pathExtension.lowercased() == "myimage" {
            guard let artworkRenderer else {
                notice.stringValue = "Export the Bediz project as a PNG, then choose it here."
                return
            }
            notice.stringValue = "Rendering Bediz artwork…"
            Task {
                let data = await Task.detached(priority: .userInitiated) { () -> Data? in
                    let output = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString + ".png")
                    defer { try? FileManager.default.removeItem(at: output) }
                    let ok = url.path.withCString { source in output.path.withCString { artworkRenderer(source, $0) } }
                    guard ok else { return nil }
                    return try? Data(contentsOf: output)
                }.value
                useImage(data)
            }
        } else if let size = try? url.resourceValues(forKeys: [.fileSizeKey]).fileSize, size <= 64 * 1_024 * 1_024 {
            useImage(try? Data(contentsOf: url, options: .mappedIfSafe))
        } else {
            notice.stringValue = "Choose artwork under 64 MB."
        }
    }

    @objc private func pasteImage() {
        let pasteboard = NSPasteboard.general
        if let data = pasteboard.data(forType: .png) ?? pasteboard.data(forType: .tiff) {
            useImage(data)
            return
        }
        let images = pasteboard.readObjects(forClasses: [NSImage.self]) as? [NSImage]
        useImage(images?.first?.tiffRepresentation)
    }

    @objc private func removeImage() {
        imageData = nil
        refresh()
    }

    @objc private func confirmDesign() {
        guard let duration = VideoTime.parse(length.stringValue), duration > 0 else {
            notice.stringValue = "Enter a duration such as 3.250."
            return
        }
        let result: Design
        if let start {
            guard let offset = VideoTime.parse(start.stringValue), artwork.isValid else {
                notice.stringValue = "Check the start time and artwork."
                return
            }
            result = .overlay(artwork, start: offset, length: duration)
        } else {
            guard card.isValid else {
                notice.stringValue = "Shorten the text or use smaller artwork."
                return
            }
            result = .page(card, length: duration)
        }
        if let error = apply(result) { notice.stringValue = error } else { cancelDesign() }
    }

    @objc private func cancelDesign() {
        endStillTask?.cancel()
        sheet.sheetParent?.endSheet(sheet)
    }
}

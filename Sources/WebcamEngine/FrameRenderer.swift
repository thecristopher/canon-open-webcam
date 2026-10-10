import CoreGraphics
import CoreImage
import CoreText
import CoreVideo
import Foundation
import ImageIO

public struct FrameOptions: Codable, Equatable, Sendable {
    /// Apps already mirror your self view, so mirroring here flips what everyone else sees.
    public var mirror = false
    /// Digital zoom toward the center, 1 means the full live view.
    public var zoom = 1.0

    public static let zoomRange = 1.0...3.0

    public init(mirror: Bool = false, zoom: Double = 1.0) {
        self.mirror = mirror
        self.zoom = zoom
    }
}

/// Turns camera JPEGs (and the "no camera" card) into 1080p BGRA frames for the virtual camera.
public final class FrameRenderer {
    public let outputSize: CGSize
    private let context = CIContext(options: [.cacheIntermediates: false])
    private let colorSpace = CGColorSpace(name: CGColorSpace.sRGB)!
    private var pool: CVPixelBufferPool?

    public init(width: Int = VirtualCamera.width, height: Int = VirtualCamera.height) {
        outputSize = CGSize(width: width, height: height)
        let attributes: [CFString: Any] = [
            kCVPixelBufferWidthKey: width,
            kCVPixelBufferHeightKey: height,
            kCVPixelBufferPixelFormatTypeKey: kCVPixelFormatType_32BGRA,
            // IOSurface backing lets the frame cross into the camera extension without a copy
            kCVPixelBufferIOSurfacePropertiesKey: [:] as CFDictionary,
        ]
        CVPixelBufferPoolCreate(nil, nil, attributes as CFDictionary, &pool)
    }

    public func render(jpeg: Data, options: FrameOptions) -> CVPixelBuffer? {
        guard let image = Self.decode(jpeg) else { return nil }
        return render(image, options: options)
    }

    /// Decodes right now instead of lazily, so a frame the camera mangled gets dropped here
    /// rather than crashing deep inside Core Image halfway through a render.
    static func decode(_ jpeg: Data) -> CIImage? {
        guard looksComplete(jpeg),
              let source = CGImageSourceCreateWithData(jpeg as CFData, nil),
              let image = CGImageSourceCreateImageAtIndex(source, 0, [kCGImageSourceShouldCacheImmediately: true] as CFDictionary)
        else { return nil }
        return CIImage(cgImage: image)
    }

    /// Every live view frame starts with FFD8 and ends with FFD9, a cut off one doesn't.
    static func looksComplete(_ jpeg: Data) -> Bool {
        jpeg.count > 4 && jpeg.prefix(2) == Data([0xFF, 0xD8]) && jpeg.suffix(2) == Data([0xFF, 0xD9])
    }

    public func render(_ image: CIImage, options: FrameOptions) -> CVPixelBuffer? {
        guard let buffer = makeBuffer() else { return nil }
        let output = CGRect(origin: .zero, size: outputSize)
        context.render(Self.upscaled(image, to: outputSize, options: options).cropped(to: output), to: buffer, bounds: output, colorSpace: colorSpace)
        return buffer
    }

    /// Lanczos plus a light sharpen keeps edges and texture crisp when 1024x576 becomes 1080p.
    static func upscaled(_ image: CIImage, to target: CGSize, options: FrameOptions) -> CIImage {
        let full = fillTransform(from: image.extent.size, to: target, options: FrameOptions(zoom: options.zoom))
        let scale = full.a
        // Only upscale the part that ends up on screen, so zooming in doesn't cost a giant intermediate
        let visible = CGRect(x: -full.tx / scale, y: -full.ty / scale, width: target.width / scale, height: target.height / scale)
        let source = image.cropped(to: visible.insetBy(dx: -2, dy: -2))
        var result = source
            .applyingFilter("CILanczosScaleTransform", parameters: [kCIInputScaleKey: scale, kCIInputAspectRatioKey: 1])
            .transformed(by: CGAffineTransform(translationX: full.tx, y: full.ty))
            .applyingFilter("CISharpenLuminance", parameters: [kCIInputSharpnessKey: 0.4, kCIInputRadiusKey: 1.5])
        if options.mirror {
            result = result.transformed(by: CGAffineTransform(a: -1, b: 0, c: 0, d: 1, tx: target.width, ty: 0))
        }
        return result
    }

    /// A dark card with a message, shown whenever there's no live picture to send.
    public func placeholder(title: String, detail: String) -> CVPixelBuffer? {
        guard let buffer = makeBuffer() else { return nil }
        CVPixelBufferLockBaseAddress(buffer, [])
        defer { CVPixelBufferUnlockBaseAddress(buffer, []) }
        guard let drawing = CGContext(
            data: CVPixelBufferGetBaseAddress(buffer),
            width: Int(outputSize.width), height: Int(outputSize.height), bitsPerComponent: 8,
            bytesPerRow: CVPixelBufferGetBytesPerRow(buffer), space: colorSpace,
            bitmapInfo: CGImageAlphaInfo.premultipliedFirst.rawValue | CGBitmapInfo.byteOrder32Little.rawValue
        ) else { return nil }

        drawing.setFillColor(CGColor(srgbRed: 0.08, green: 0.09, blue: 0.11, alpha: 1))
        drawing.fill(CGRect(origin: .zero, size: outputSize))
        let middle = outputSize.height / 2
        drawCentered(title, size: 64, weight: 0.4, gray: 0.95, baseline: middle + 10, in: drawing)
        drawCentered(detail, size: 36, weight: 0, gray: 0.6, baseline: middle - 60, in: drawing)
        return buffer
    }

    /// Scales the picture to fill the frame (cropping rather than letterboxing), then applies zoom and mirror.
    static func fillTransform(from source: CGSize, to target: CGSize, options: FrameOptions) -> CGAffineTransform {
        let zoom = min(max(options.zoom, FrameOptions.zoomRange.lowerBound), FrameOptions.zoomRange.upperBound)
        let scale = max(target.width / source.width, target.height / source.height) * zoom
        let offsetX = (target.width - source.width * scale) / 2
        let offsetY = (target.height - source.height * scale) / 2
        var transform = CGAffineTransform(scaleX: scale, y: scale).concatenating(CGAffineTransform(translationX: offsetX, y: offsetY))
        if options.mirror {
            transform = transform.concatenating(CGAffineTransform(a: -1, b: 0, c: 0, d: 1, tx: target.width, ty: 0))
        }
        return transform
    }

    private func makeBuffer() -> CVPixelBuffer? {
        guard let pool else { return nil }
        var buffer: CVPixelBuffer?
        CVPixelBufferPoolCreatePixelBuffer(nil, pool, &buffer)
        return buffer
    }

    private func drawCentered(_ text: String, size: CGFloat, weight: CGFloat, gray: CGFloat, baseline: CGFloat, in drawing: CGContext) {
        let font = CTFontCreateUIFontForLanguage(weight > 0 ? .emphasizedSystem : .system, size, nil)
            ?? CTFontCreateWithName("Helvetica" as CFString, size, nil)
        let attributes: [CFString: Any] = [
            kCTFontAttributeName: font,
            kCTForegroundColorAttributeName: CGColor(gray: gray, alpha: 1),
        ]
        let line = CTLineCreateWithAttributedString(CFAttributedStringCreate(nil, text as CFString, attributes as CFDictionary))
        let width = CTLineGetTypographicBounds(line, nil, nil, nil)
        drawing.textPosition = CGPoint(x: (outputSize.width - width) / 2, y: baseline)
        CTLineDraw(line, drawing)
    }
}

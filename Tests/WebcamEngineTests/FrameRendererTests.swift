import CoreGraphics
import CoreImage
import CoreVideo
import Foundation
import Testing
@testable import WebcamEngine

private let fullHD = CGSize(width: 1920, height: 1080)

@Test func movieModeLiveViewScalesStraightTo1080p() {
    let transform = FrameRenderer.fillTransform(from: CGSize(width: 1024, height: 576), to: fullHD, options: FrameOptions())
    #expect(transform.a == 1.875)
    #expect(transform.tx == 0 && transform.ty == 0)
}

@Test func photoModeLiveViewIsCroppedNotLetterboxed() {
    let transform = FrameRenderer.fillTransform(from: CGSize(width: 1056, height: 704), to: fullHD, options: FrameOptions())
    let scaledHeight = 704 * transform.d
    #expect(scaledHeight > 1080)
    #expect(transform.tx == 0)
}

@Test func zoomMagnifiesAroundTheCenter() {
    let transform = FrameRenderer.fillTransform(from: CGSize(width: 1024, height: 576), to: fullHD, options: FrameOptions(zoom: 2))
    let center = CGPoint(x: 512, y: 288).applying(transform)
    #expect(transform.a == 3.75)
    #expect(center == CGPoint(x: 960, y: 540))
}

@Test func zoomOutsideTheRangeIsClamped() {
    let transform = FrameRenderer.fillTransform(from: CGSize(width: 1024, height: 576), to: fullHD, options: FrameOptions(zoom: 10))
    #expect(transform.a == 1.875 * 3)
}

@Test func mirrorSendsTheLeftEdgeToTheRight() {
    let transform = FrameRenderer.fillTransform(from: CGSize(width: 1024, height: 576), to: fullHD, options: FrameOptions(mirror: true))
    #expect(CGPoint(x: 0, y: 0).applying(transform).x == 1920)
}

@Test func thePlaceholderIsAFull1080pFrame() throws {
    let card = try #require(FrameRenderer().placeholder(title: "Waiting for Leon", detail: "Raccoon City is offline"))
    #expect(CVPixelBufferGetWidth(card) == 1920)
    #expect(CVPixelBufferGetHeight(card) == 1080)
}

@Test func renderingAnImageFillsTheWholeFrame() throws {
    let red = CIImage(color: .red).cropped(to: CGRect(x: 0, y: 0, width: 1024, height: 576))
    let frame = try #require(FrameRenderer().render(red, options: FrameOptions()))
    #expect(CVPixelBufferGetPixelFormatType(frame) == kCVPixelFormatType_32BGRA)
}

@Test func garbageBytesDontBecomeAFrame() {
    #expect(FrameRenderer().render(jpeg: Data([1, 2, 3]), options: FrameOptions()) == nil)
}

@Test func theUpscaledPictureCoversTheWholeFrame() {
    let liveView = CIImage(color: .gray).cropped(to: CGRect(x: 0, y: 0, width: 1024, height: 576))
    let upscaled = FrameRenderer.upscaled(liveView, to: fullHD, options: FrameOptions())
    #expect(upscaled.extent.contains(CGRect(origin: .zero, size: fullHD)))
}

@Test func zoomingInStillCoversTheWholeFrame() {
    let liveView = CIImage(color: .gray).cropped(to: CGRect(x: 0, y: 0, width: 1024, height: 576))
    let upscaled = FrameRenderer.upscaled(liveView, to: fullHD, options: FrameOptions(mirror: true, zoom: 2.5))
    #expect(upscaled.extent.contains(CGRect(origin: .zero, size: fullHD)))
}

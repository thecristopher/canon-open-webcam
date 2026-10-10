import CoreGraphics
import CoreImage
import Foundation
import Testing
@testable import WebcamEngine

private func applied(_ name: String, _ value: String, to options: FrameOptions = FrameOptions()) -> FrameOptions? {
    try? options.applying(name, value).get()
}

@Test func brightnessTakesAValueBetweenMinusOneAndOne() {
    #expect(applied("brightness", "0.3")?.brightness == 0.3)
}

@Test func brightnessPastOneIsRefused() {
    #expect(applied("brightness", "4") == nil)
}

@Test func framesPerSecondOnlyTakesTheOffers() {
    #expect(applied("fps", "60")?.frameRate == 60)
    #expect(applied("fps", "camera")?.frameRate == 0)
    #expect(applied("fps", "48") == nil)
}

@Test func resettingThePictureLeavesFramingAlone() {
    var options = FrameOptions(mirror: true, zoom: 2)
    options.saturation = 0.8
    let reset = applied("picture", "reset", to: options)
    #expect(reset?.saturation == 0)
    #expect(reset?.zoom == 2)
}

@Test func resettingFramingCentersAndZoomsOut() {
    var options = FrameOptions(zoom: 2.5)
    options.panX = 0.7
    let reset = applied("framing", "reset", to: options)
    #expect(reset?.zoom == 1)
    #expect(reset?.panX == 0)
}

@Test func untouchedColorsSkipTheFilters() {
    #expect(FrameOptions().hasColorAdjustments == false)
    #expect(applied("tint", "0.1")?.hasColorAdjustments == true)
}

@Test func oldOptionFilesStillLoad() throws {
    let old = Data(#"{"mirror":true,"zoom":1.5}"#.utf8)
    let options = try JSONDecoder().decode(FrameOptions.self, from: old)
    #expect(options.mirror && options.zoom == 1.5 && options.sharpness == 0.4)
}

@Test func panningAllTheWayRightShowsThePicturesRightEdge() {
    var options = FrameOptions(zoom: 2)
    options.panX = 1
    let transform = FrameRenderer.fillTransform(from: CGSize(width: 1024, height: 576), to: CGSize(width: 1920, height: 1080), options: options)
    #expect(abs(CGPoint(x: 1024, y: 0).applying(transform).x - 1920) < 0.001)
}

@Test func panningDoesNothingWithoutZoom() {
    var options = FrameOptions()
    options.panX = 1
    let transform = FrameRenderer.fillTransform(from: CGSize(width: 1024, height: 576), to: CGSize(width: 1920, height: 1080), options: options)
    #expect(transform.tx == 0)
}

@Test func warmthMakesGrayWarmer() {
    var options = FrameOptions()
    options.warmth = 1
    let gray = CIImage(color: CIColor(red: 0.5, green: 0.5, blue: 0.5)).cropped(to: CGRect(x: 0, y: 0, width: 2, height: 2))
    var pixel = [UInt8](repeating: 0, count: 4)
    CIContext().render(FrameRenderer.adjustColors(gray, options: options), toBitmap: &pixel, rowBytes: 4,
                       bounds: CGRect(x: 0, y: 0, width: 1, height: 1), format: .RGBA8, colorSpace: CGColorSpace(name: CGColorSpace.sRGB))
    #expect(pixel[0] > pixel[2])
}

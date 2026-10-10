import AppKit

// Draws an app icon: a rounded gradient tile with the webcam symbol, plus a red badge for the uninstaller
let arguments = CommandLine.arguments
guard arguments.count == 3, ["installer", "uninstaller"].contains(arguments[1]) else {
    print("usage: make-icon.swift installer|uninstaller output.png")
    exit(2)
}
let isUninstaller = arguments[1] == "uninstaller"
let size: CGFloat = 1024

let bitmap = NSBitmapImageRep(
    bitmapDataPlanes: nil, pixelsWide: Int(size), pixelsHigh: Int(size), bitsPerSample: 8,
    samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0
)!
NSGraphicsContext.saveGraphicsState()
NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: bitmap)

// macOS icons sit inside a 824pt tile with breathing room around it
let tile = NSRect(x: 100, y: 100, width: 824, height: 824)
let tilePath = NSBezierPath(roundedRect: tile, xRadius: 185, yRadius: 185)
let colors: [NSColor] = isUninstaller
    ? [NSColor(calibratedWhite: 0.62, alpha: 1), NSColor(calibratedWhite: 0.42, alpha: 1)]
    : [NSColor(calibratedRed: 0.25, green: 0.62, blue: 1, alpha: 1), NSColor(calibratedRed: 0.0, green: 0.42, blue: 0.95, alpha: 1)]
NSGradient(colors: colors)!.draw(in: tilePath, angle: -90)

func drawSymbol(_ name: String, pointSize: CGFloat, colors: [NSColor], in rect: NSRect) {
    let configuration = NSImage.SymbolConfiguration(pointSize: pointSize, weight: .semibold)
        .applying(NSImage.SymbolConfiguration(paletteColors: colors))
    guard let symbol = NSImage(systemSymbolName: name, accessibilityDescription: nil)?.withSymbolConfiguration(configuration) else { return }
    let fitted = symbol.size
    let origin = NSPoint(x: rect.midX - fitted.width / 2, y: rect.midY - fitted.height / 2)
    symbol.draw(in: NSRect(origin: origin, size: fitted))
}

// The second palette color paints the lens, so it reads as a camera instead of a white blob
drawSymbol("web.camera.fill", pointSize: 400, colors: [colors[1], .white], in: tile)

if isUninstaller {
    let badge = NSRect(x: 650, y: 130, width: 260, height: 260)
    NSColor.white.setFill()
    NSBezierPath(ovalIn: badge.insetBy(dx: -14, dy: -14)).fill()
    NSColor(calibratedRed: 0.95, green: 0.23, blue: 0.2, alpha: 1).setFill()
    NSBezierPath(ovalIn: badge).fill()
    drawSymbol("minus", pointSize: 150, colors: [.white], in: badge)
}

NSGraphicsContext.restoreGraphicsState()
try! bitmap.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: arguments[2]))

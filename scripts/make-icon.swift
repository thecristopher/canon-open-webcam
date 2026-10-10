import AppKit

// Draws an app icon: a rounded gradient tile with the webcam symbol, with a badge for the installer and uninstaller
let arguments = CommandLine.arguments
guard arguments.count == 3, ["app", "installer", "uninstaller"].contains(arguments[1]) else {
    print("usage: make-icon.swift app|installer|uninstaller output.png")
    exit(2)
}
let kind = arguments[1]
let isUninstaller = kind == "uninstaller"
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

func drawBadge(color: NSColor, symbol: String) {
    let badge = NSRect(x: 650, y: 130, width: 260, height: 260)
    NSColor.white.setFill()
    NSBezierPath(ovalIn: badge.insetBy(dx: -14, dy: -14)).fill()
    color.setFill()
    NSBezierPath(ovalIn: badge).fill()
    drawSymbol(symbol, pointSize: 150, colors: [.white], in: badge)
}

switch kind {
case "installer": drawBadge(color: NSColor(calibratedRed: 0.2, green: 0.75, blue: 0.35, alpha: 1), symbol: "arrow.down")
case "uninstaller": drawBadge(color: NSColor(calibratedRed: 0.95, green: 0.23, blue: 0.2, alpha: 1), symbol: "minus")
default: break
}

NSGraphicsContext.restoreGraphicsState()
try! bitmap.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: arguments[2]))

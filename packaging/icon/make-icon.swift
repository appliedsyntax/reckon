// Draws the Reckon app icon with AppKit (no third-party tools).
//
//   swift packaging/icon/make-icon.swift packaging/icon
//
// Writes:
//   icon-mac-1024.png     rounded square with transparent margin (macOS)
//   icon-square-1024.png  full-bleed square (iOS, Android, Linux, Windows)

import AppKit

func render(size: CGFloat, rounded: Bool) -> Data {
    let rep = NSBitmapImageRep(
        bitmapDataPlanes: nil, pixelsWide: Int(size), pixelsHigh: Int(size),
        bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
        colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)

    // macOS icons leave ~10% transparent margin around a rounded square.
    let inset: CGFloat = rounded ? size * 0.1 : 0
    let rect = NSRect(x: inset, y: inset, width: size - inset * 2, height: size - inset * 2)
    let shape = rounded
        ? NSBezierPath(roundedRect: rect, xRadius: rect.width * 0.225, yRadius: rect.width * 0.225)
        : NSBezierPath(rect: rect)
    let bg = NSGradient(
        starting: NSColor(srgbRed: 0.17, green: 0.18, blue: 0.24, alpha: 1),
        ending: NSColor(srgbRed: 0.09, green: 0.09, blue: 0.12, alpha: 1))!
    bg.draw(in: shape, angle: -90)

    let u = rect.width / 100  // drawing unit
    func bar(_ x: CGFloat, _ y: CGFloat, _ w: CGFloat, _ h: CGFloat, _ c: NSColor) {
        c.setFill()
        NSBezierPath(
            roundedRect: NSRect(x: rect.minX + x * u, y: rect.minY + y * u, width: w * u, height: h * u),
            xRadius: h * u / 2, yRadius: h * u / 2
        ).fill()
    }
    // "Text" lines on the left, like lines of a document.
    let text = NSColor(srgbRed: 0.86, green: 0.87, blue: 0.9, alpha: 0.9)
    let muted = NSColor(srgbRed: 0.86, green: 0.87, blue: 0.9, alpha: 0.35)
    bar(16, 66, 34, 7, text)
    bar(16, 51, 24, 7, muted)
    bar(16, 36, 30, 7, muted)
    // Result "=" sign on the right, in the app's result green.
    let green = NSColor(srgbRed: 0.49, green: 0.88, blue: 0.63, alpha: 1)
    bar(58, 56, 28, 8, green)
    bar(58, 38, 28, 8, green)
    // Divider between editor and results.
    bar(52.5, 22, 1.2, 58, NSColor(white: 1, alpha: 0.12))

    NSGraphicsContext.restoreGraphicsState()
    return rep.representation(using: .png, properties: [:])!
}

let out = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "."
try! render(size: 1024, rounded: true).write(to: URL(fileURLWithPath: "\(out)/icon-mac-1024.png"))
try! render(size: 1024, rounded: false).write(to: URL(fileURLWithPath: "\(out)/icon-square-1024.png"))
print("wrote icons to \(out)")

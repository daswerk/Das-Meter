// Draws the DMG window's background: drag Das-Meter to Applications, and the
// one-time Open Anyway steps (no Developer ID yet, ADR 0004).
//
//   swift scripts/dmg-background.swift OUT.png [SCALE]
//
// The window is 660 × 440 points; SCALE 2 draws it for Retina. The icons sit
// at (170, 150) and (490, 150), where scripts/macos-dmg.sh puts them.
import AppKit

let args = CommandLine.arguments
guard args.count >= 2 else {
    FileHandle.standardError.write("usage: dmg-background.swift OUT.png [SCALE]\n".data(using: .utf8)!)
    exit(2)
}
let scale = args.count >= 3 ? CGFloat(Double(args[2]) ?? 1) : 1
let size = NSSize(width: 660, height: 440)

let rep = NSBitmapImageRep(
    bitmapDataPlanes: nil, pixelsWide: Int(size.width * scale), pixelsHigh: Int(size.height * scale),
    bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
    colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
rep.size = size
NSGraphicsContext.saveGraphicsState()
NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)

// AppKit's origin is bottom left; work in top-left points like Finder.
func y(_ top: CGFloat) -> CGFloat { size.height - top }

// Light, so Finder's black icon labels stay readable.
NSColor(calibratedRed: 0.96, green: 0.95, blue: 0.98, alpha: 1).setFill()
NSRect(origin: .zero, size: size).fill()

// The arrow from the app to Applications.
let arrow = NSBezierPath()
arrow.lineWidth = 3
arrow.lineCapStyle = .round
arrow.move(to: NSPoint(x: 262, y: y(150)))
arrow.line(to: NSPoint(x: 398, y: y(150)))
arrow.move(to: NSPoint(x: 384, y: y(138)))
arrow.line(to: NSPoint(x: 398, y: y(150)))
arrow.line(to: NSPoint(x: 384, y: y(162)))
NSColor(calibratedRed: 0.45, green: 0.33, blue: 0.85, alpha: 1).setStroke()
arrow.stroke()

func text(_ string: String, top: CGFloat, size points: CGFloat, weight: NSFont.Weight, alpha: CGFloat) {
    let paragraph = NSMutableParagraphStyle()
    paragraph.alignment = .center
    let attributes: [NSAttributedString.Key: Any] = [
        .font: NSFont.systemFont(ofSize: points, weight: weight),
        .foregroundColor: NSColor(white: 0.08, alpha: alpha),
        .paragraphStyle: paragraph,
    ]
    let height = points * 1.4
    NSAttributedString(string: string, attributes: attributes)
        .draw(in: NSRect(x: 30, y: y(top + height), width: size.width - 60, height: height))
}

text("Drag Das-Meter to Applications", top: 236, size: 15, weight: .semibold, alpha: 0.95)
text("The first time you open it, macOS says it can't check the app for malicious software.",
     top: 282, size: 12, weight: .regular, alpha: 0.75)
text("Open System Settings ▸ Privacy & Security, scroll down and click Open Anyway.",
     top: 302, size: 12, weight: .regular, alpha: 0.75)
text("You only do this once: Das-Meter is open source and signed with its own certificate,",
     top: 340, size: 11, weight: .regular, alpha: 0.5)
text("not with an Apple Developer ID yet.", top: 356, size: 11, weight: .regular, alpha: 0.5)

NSGraphicsContext.restoreGraphicsState()
let png = rep.representation(using: .png, properties: [:])!
try! png.write(to: URL(fileURLWithPath: args[1]))

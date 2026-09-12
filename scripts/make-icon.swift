// The app icon's 1024pt master, drawn with AppKit rather than shipped as a
// vector asset: the glyph is the same rounded-stroke figure the menu bar draws,
// so keeping it in code keeps the two from drifting. Run via scripts/make-icon.sh,
// which renders this and folds the sizes into assets/AppIcon.icns.
//
//   swift scripts/make-icon.swift out.png

import AppKit

let S: CGFloat = 1024
let inset: CGFloat = 100
let bodyR: CGFloat = 185

let ink = NSColor(srgbRed: 0.12, green: 0.12, blue: 0.11, alpha: 1)
let accent = NSColor(srgbRed: 0.77, green: 0.33, blue: 0.23, alpha: 1)

func rounded(_ r: NSRect, _ rad: CGFloat) -> NSBezierPath {
    NSBezierPath(roundedRect: r, xRadius: rad, yRadius: rad)
}

func render(_ draw: (NSRect) -> Void) -> NSBitmapImageRep {
    let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: Int(S), pixelsHigh: Int(S),
        bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
        colorSpaceName: .calibratedRGB, bytesPerRow: 0, bitsPerPixel: 0)!
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
    let body = NSRect(x: inset, y: inset, width: S - 2*inset, height: S - 2*inset)
    let shape = rounded(body, bodyR)
    NSGraphicsContext.current!.cgContext.saveGState()
    shape.addClip()
    NSGradient(colors: [NSColor(srgbRed: 0.976, green: 0.972, blue: 0.960, alpha: 1),
                        NSColor(srgbRed: 0.898, green: 0.890, blue: 0.870, alpha: 1)])!
        .draw(in: body, angle: -90)
    NSGraphicsContext.current!.cgContext.restoreGState()
    // hairline edge so the icon keeps an edge on a white wallpaper
    NSColor(srgbRed: 0, green: 0, blue: 0, alpha: 0.10).setStroke()
    let edge = rounded(body.insetBy(dx: 1.5, dy: 1.5), bodyR - 1.5); edge.lineWidth = 3; edge.stroke()
    draw(body)
    NSGraphicsContext.restoreGraphicsState()
    return rep
}

func mailbar(_ body: NSRect) {
    let w: CGFloat = 520, h: CGFloat = 386, stroke: CGFloat = 34
    let f = NSRect(x: body.midX - w/2 - 26, y: body.midY - h/2 - 22, width: w, height: h)
    ink.setStroke()
    let e = f.insetBy(dx: stroke/2, dy: stroke/2)
    let p = rounded(e, 54); p.lineWidth = stroke; p.lineJoinStyle = .round; p.stroke()
    let flapInset: CGFloat = 40
    let depth = e.height * 0.52
    let v = NSBezierPath()
    v.move(to: NSPoint(x: e.minX + flapInset, y: e.maxY))
    v.line(to: NSPoint(x: e.midX, y: e.maxY - depth))
    v.line(to: NSPoint(x: e.maxX - flapInset, y: e.maxY))
    v.lineWidth = stroke; v.lineJoinStyle = .round; v.lineCapStyle = .round; v.stroke()
    // the unread dot, clear of the envelope's top-right corner
    let rr: CGFloat = 54
    let c = NSPoint(x: e.maxX + 52, y: e.maxY + 52)
    accent.setFill()
    NSBezierPath(ovalIn: NSRect(x: c.x-rr, y: c.y-rr, width: 2*rr, height: 2*rr)).fill()
}

let out = CommandLine.arguments[1]
let rep = render(mailbar)
try! rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: out))

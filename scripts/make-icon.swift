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

// The same ring the menu bar draws, at 1024pt: a faint full-circle track and an
// arc from 12 o'clock clockwise, here parked at a representative utilisation so
// the icon reads as "usage" at a glance rather than as an empty or full ring.
func ring(_ body: NSRect) {
    let stroke: CGFloat = 78
    let radius: CGFloat = 250
    let percent: CGFloat = 0.65
    let c = NSPoint(x: body.midX, y: body.midY)

    let track = NSBezierPath()
    track.appendArc(withCenter: c, radius: radius, startAngle: 0, endAngle: 360)
    track.lineWidth = stroke
    ink.withAlphaComponent(0.30).setStroke()
    track.stroke()

    // AppKit angles are degrees anticlockwise from 3 o'clock, so 12 o'clock is
    // 90 and a clockwise sweep counts down from there.
    let arc = NSBezierPath()
    arc.appendArc(withCenter: c, radius: radius, startAngle: 90,
                  endAngle: 90 - 360 * percent, clockwise: true)
    arc.lineWidth = stroke
    arc.lineCapStyle = .round
    ink.setStroke()
    arc.stroke()
}

let out = CommandLine.arguments[1]
let rep = render(ring)
try! rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: out))

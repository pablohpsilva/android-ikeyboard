import Foundation

/// A raw screen-space point on a gesture path. The shared value type for the
/// shell's swipe geometry — no UIKit `CGPoint`, so it stays host-testable.
public struct GesturePoint: Equatable {
    public let x: Float
    public let y: Float
    public init(x: Float, y: Float) { self.x = x; self.y = y }
}

/// Accumulates a touch path and classifies swipe-vs-tap so a quick tap is never
/// treated as a glide (BR-41). Pure value type: it holds only the path and does no
/// I/O. A gesture is a swipe once it has travelled further than one key *and* its
/// horizontal reach crosses at least two key columns — a long vertical wiggle over
/// a single column stays a tap.
public struct SwipeTracker {
    private var points: [GesturePoint] = []

    public init() {}

    /// Start a fresh gesture at `p`, discarding any prior path.
    public mutating func begin(at p: GesturePoint) { points = [p] }

    /// Append a moved-to point in order.
    public mutating func move(to p: GesturePoint) { points.append(p) }

    /// The path so far: the begin point followed by each moved-to point, in order.
    public var path: [GesturePoint] { points }

    /// True once the gesture has both travelled a total arc-length greater than
    /// `keyPitch` and spanned a horizontal range greater than `keyPitch` (an
    /// approximation for "crosses ≥ 2 key columns"). Both conditions are required,
    /// so neither a short jiggle nor a single-column vertical drag counts.
    public func isSwipe(keyPitch: Float) -> Bool {
        guard points.count >= 2 else { return false }
        var arc: Float = 0
        var minX = points[0].x, maxX = points[0].x
        for i in 1..<points.count {
            let dx = points[i].x - points[i - 1].x
            let dy = points[i].y - points[i - 1].y
            arc += (dx * dx + dy * dy).squareRoot()
            minX = min(minX, points[i].x)
            maxX = max(maxX, points[i].x)
        }
        return arc > keyPitch && (maxX - minX) > keyPitch
    }
}

import Foundation

/// Maps a raw screen-space swipe point into the core's logical coordinate frame.
/// iOS taps decode by button identity, so there is no existing screen→logical map;
/// swipe needs one. It is a per-axis affine (`logical = a·screen + b`) fitted by
/// least squares over correspondences between rendered letter-button screen centres
/// and their logical `EngineKey` centres. Continuous by construction — an off-grid
/// point projects to the interpolated logical coordinate, never snapped to a key.
public struct LayoutProjection {
    private let ax: Float, bx: Float
    private let ay: Float, by: Float

    /// `pairs` are (screen centre, logical centre) for each letter key. Needs ≥ 2
    /// points spanning x and ≥ 2 spanning y for a determined fit; a degenerate
    /// (zero-variance) axis falls back to identity for that axis rather than crash.
    public init(pairs: [(screen: GesturePoint, logical: GesturePoint)]) {
        let fx = Self.fit(pairs.map { ($0.screen.x, $0.logical.x) })
        let fy = Self.fit(pairs.map { ($0.screen.y, $0.logical.y) })
        (ax, bx) = fx
        (ay, by) = fy
    }

    /// Project a raw screen point into logical space via the fitted per-axis affine.
    public func toLogical(_ screen: GesturePoint) -> GesturePoint {
        GesturePoint(x: ax * screen.x + bx, y: ay * screen.y + by)
    }

    /// Ordinary least-squares slope/intercept for `y = a·x + b`. Falls back to
    /// identity (`a = 1, b = 0`) when the inputs have no x-variance, so a degenerate
    /// axis is finite rather than a divide-by-zero.
    private static func fit(_ pts: [(Float, Float)]) -> (a: Float, b: Float) {
        let n = Float(pts.count)
        guard n > 0 else { return (1, 0) }
        let sumX = pts.reduce(Float(0)) { $0 + $1.0 }
        let sumY = pts.reduce(Float(0)) { $0 + $1.1 }
        let meanX = sumX / n, meanY = sumY / n
        var sxx: Float = 0, sxy: Float = 0
        for (x, y) in pts {
            sxx += (x - meanX) * (x - meanX)
            sxy += (x - meanX) * (y - meanY)
        }
        guard sxx > 0 else { return (1, 0) }
        let a = sxy / sxx
        return (a, meanY - a * meanX)
    }
}

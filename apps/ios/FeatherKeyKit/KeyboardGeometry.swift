import Foundation

/// The extent of the layout's logical coordinate space.
public struct LogicalSize: Equatable {
    public let width: Float
    public let height: Float
    public init(width: Float, height: Float) { self.width = width; self.height = height }
}

/// Pure keyboard geometry — no UIKit types, so it is unit-testable off-device.
/// Mirrors Android's `KeyboardGeometry.kt`.
public enum KeyboardGeometry {
    /// The logical coordinate space is the union extent of the layout's keys —
    /// the same space `FfiKey`/`decode` use, so drawing and decoding agree.
    public static func logicalBounds(_ keys: [EngineKey]) -> LogicalSize {
        let w = keys.map { $0.x + $0.width }.max() ?? 0
        let h = keys.map { $0.y + $0.height }.max() ?? 0
        return LogicalSize(width: w, height: h)
    }

    /// Map a view-pixel point into logical space with independent x/y scale, the
    /// exact inverse of how keys are drawn (logical→view). Self-consistent by
    /// construction: what is drawn is what `decode` resolves against.
    public static func toLogical(viewX: Float, viewY: Float,
                                 viewWidth: Float, viewHeight: Float,
                                 logical: LogicalSize) -> (x: Float, y: Float) {
        let sx = viewWidth  > 0 ? logical.width  / viewWidth  : 0
        let sy = viewHeight > 0 ? logical.height / viewHeight : 0
        return (viewX * sx, viewY * sy)
    }
}

import UIKit

/// A long-press accent/alternate popup, styled like the native iOS keyboard.
/// Presents a horizontal row of alternate glyphs in a rounded container floated
/// above the pressed key; the glyph under the finger is highlighted, and the
/// selected one is committed on release.
final class AccentPopupView: UIView {

    /// Called with the alternate the finger was over when `hide()` runs after a commit.
    var onCommit: ((String) -> Void)?

    private var alternates: [String] = []
    private var cells: [UILabel] = []
    private var selectedIndex: Int?
    private var theme: KeyboardTheme?

    private let stack = UIStackView()

    // Native-feel metrics.
    private let cellWidth: CGFloat = 40
    private let cellHeight: CGFloat = 46
    private let vInset: CGFloat = 6
    private let hInset: CGFloat = 6
    private let gapAboveKey: CGFloat = 8

    // MARK: - Lifecycle

    override init(frame: CGRect) {
        super.init(frame: frame)
        isUserInteractionEnabled = false
        layer.shadowColor = UIColor.black.cgColor
        layer.shadowOpacity = 0.28
        layer.shadowRadius = 6
        layer.shadowOffset = CGSize(width: 0, height: 2)

        stack.axis = .horizontal
        stack.alignment = .fill
        stack.distribution = .fillEqually
        stack.spacing = 0
        stack.isUserInteractionEnabled = false
        addSubview(stack)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }

    // MARK: - Public API

    func show(alternates: [String], over keyFrame: CGRect, in container: UIView, theme: KeyboardTheme) {
        self.alternates = alternates
        self.theme = theme
        self.selectedIndex = alternates.isEmpty ? nil : 0

        backgroundColor = theme.popupBackground
        layer.cornerRadius = theme.cornerRadius + 3

        buildCells(font: theme.popupFont, textColor: theme.keyText)

        let count = max(alternates.count, 1)
        let width = CGFloat(count) * cellWidth + hInset * 2
        let height = cellHeight + vInset * 2

        // Center horizontally over the key, clamped inside the container; float above it.
        var originX = keyFrame.midX - width / 2
        let minX: CGFloat = 2
        let maxX = container.bounds.width - width - 2
        originX = min(max(originX, minX), max(maxX, minX))
        var originY = keyFrame.minY - gapAboveKey - height
        if originY < 2 { originY = keyFrame.maxY + gapAboveKey } // fall below if no room above

        frame = CGRect(x: originX, y: originY, width: width, height: height)
        stack.frame = bounds.insetBy(dx: hInset, dy: vInset)

        if superview !== container {
            removeFromSuperview()
            container.addSubview(self)
        } else {
            container.bringSubviewToFront(self)
        }
        applyHighlight()
    }

    /// Highlights the alternate under `point` (expressed in the popup's own coordinate space).
    func updateSelection(at point: CGPoint) {
        guard !alternates.isEmpty else { return }
        let local = convert(point, from: superview)
        let x = local.x - hInset
        var index = Int(floor(x / cellWidth))
        index = min(max(index, 0), alternates.count - 1)
        if index != selectedIndex {
            selectedIndex = index
            applyHighlight()
        }
    }

    func selectedAlternate() -> String? {
        guard let i = selectedIndex, alternates.indices.contains(i) else { return nil }
        return alternates[i]
    }

    func hide() {
        if let choice = selectedAlternate() {
            onCommit?(choice)
        }
        removeFromSuperview()
        alternates = []
        cells = []
        selectedIndex = nil
    }

    // MARK: - Rendering

    private func buildCells(font: UIFont, textColor: UIColor) {
        cells.forEach { $0.removeFromSuperview() }
        stack.arrangedSubviews.forEach { stack.removeArrangedSubview($0); $0.removeFromSuperview() }
        cells = alternates.map { glyph in
            let label = UILabel()
            label.text = glyph
            label.font = font
            label.textAlignment = .center
            label.textColor = textColor
            label.layer.cornerRadius = (theme?.cornerRadius ?? 5)
            label.layer.masksToBounds = true
            stack.addArrangedSubview(label)
            return label
        }
    }

    private func applyHighlight() {
        let tint = tintColor ?? UIColor.systemBlue
        for (i, cell) in cells.enumerated() {
            let on = (i == selectedIndex)
            cell.backgroundColor = on ? tint : .clear
            cell.textColor = on ? .white : (theme?.keyText ?? .label)
        }
    }
}

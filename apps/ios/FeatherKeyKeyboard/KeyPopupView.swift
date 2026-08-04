import UIKit

/// The native "magnified key" bubble shown while a letter key is pressed.
///
/// A rounded bubble that sits ABOVE the pressed key and shows the letter
/// enlarged — mirroring the stock iOS keyboard's key-preview popup. The view
/// is purely decorative: it never receives touches and simply follows the key
/// it is asked to magnify.
final class KeyPopupView: UIView {

    // MARK: - Subviews

    private let label: UILabel = {
        let l = UILabel()
        l.textAlignment = .center
        l.baselineAdjustment = .alignCenters
        l.adjustsFontSizeToFitWidth = true
        l.minimumScaleFactor = 0.6
        l.translatesAutoresizingMaskIntoConstraints = false
        return l
    }()

    // MARK: - Geometry constants (mirrors the stock preview proportions)

    /// The bubble is wider than the key on each side …
    private let horizontalOverhang: CGFloat = 11
    /// … extends above the key by (key height × this factor) …
    private let riseFactor: CGFloat = 1.05
    /// … and its own body is a little taller than the key.
    private let heightFactor: CGFloat = 1.12

    // MARK: - Init

    override init(frame: CGRect) {
        super.init(frame: frame)
        commonInit()
    }

    required init?(coder: NSCoder) {
        super.init(coder: coder)
        commonInit()
    }

    private func commonInit() {
        isUserInteractionEnabled = false
        addSubview(label)
        NSLayoutConstraint.activate([
            label.leadingAnchor.constraint(equalTo: leadingAnchor),
            label.trailingAnchor.constraint(equalTo: trailingAnchor),
            label.topAnchor.constraint(equalTo: topAnchor),
            // Keep the glyph off the very bottom so it reads as centred in the
            // bubble body rather than the full (taller) frame.
            label.bottomAnchor.constraint(
                equalTo: bottomAnchor, constant: -bottomInset()),
        ])
    }

    // MARK: - Public API

    /// Show the bubble magnifying `label`, positioned above `keyFrame`.
    ///
    /// - Parameters:
    ///   - label:     The glyph to enlarge (already the correct case).
    ///   - keyFrame:  The pressed key's frame, in `container`'s coordinate space.
    ///   - container: The view the bubble is added to and positioned within.
    ///   - theme:     Resolved colours/fonts for the current appearance.
    func show(label text: String,
              over keyFrame: CGRect,
              in container: UIView,
              theme: KeyboardTheme) {

        label.text = text
        label.font = theme.popupFont
        label.textColor = theme.keyText

        backgroundColor = theme.popupBackground
        layer.cornerRadius = theme.cornerRadius + 3
        layer.cornerCurve = .continuous

        applyShadow(theme: theme)

        frame = bubbleFrame(for: keyFrame, in: container)
        if superview !== container {
            removeFromSuperview()
            container.addSubview(self)
        }
        container.bringSubviewToFront(self)
        isHidden = false
        alpha = 1
    }

    /// Remove the bubble from view.
    func hide() {
        isHidden = true
        removeFromSuperview()
    }

    // MARK: - Layout

    private func bubbleFrame(for keyFrame: CGRect, in container: UIView) -> CGRect {
        let width = keyFrame.width + horizontalOverhang * 2
        let height = keyFrame.height * heightFactor
        let x = keyFrame.midX - width / 2
        // Bubble bottom overlaps the key top slightly; body rises above it.
        let y = keyFrame.minY - keyFrame.height * riseFactor

        var rect = CGRect(x: x, y: y, width: width, height: height)

        // Clamp horizontally so an edge key's bubble stays on screen.
        let bounds = container.bounds
        if rect.minX < 0 {
            rect.origin.x = 0
        } else if rect.maxX > bounds.width {
            rect.origin.x = bounds.width - rect.width
        }
        return rect
    }

    /// The portion of the frame reserved below the visible body, so the glyph
    /// centres within the rounded top rather than the whole overlap.
    private func bottomInset() -> CGFloat {
        return 6
    }

    // MARK: - Shadow

    private func applyShadow(theme: KeyboardTheme) {
        layer.shadowColor = theme.keyShadow.cgColor
        layer.shadowOpacity = 1
        layer.shadowRadius = 3
        layer.shadowOffset = CGSize(width: 0, height: 1)
        layer.masksToBounds = false
    }

    override func traitCollectionDidChange(_ previous: UITraitCollection?) {
        super.traitCollectionDidChange(previous)
        // Keep the CGColor shadow correct across light/dark transitions.
        if traitCollection.hasDifferentColorAppearance(comparedTo: previous) {
            let theme = KeyboardTheme.resolved(for: traitCollection)
            layer.shadowColor = theme.keyShadow.cgColor
            label.textColor = theme.keyText
            backgroundColor = theme.popupBackground
        }
    }
}

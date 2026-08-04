import UIKit

/// Shared native-key presentation: the metrics and the styling that make a plain
/// `UIButton` look like a stock iOS key. Used by both the alpha layout
/// (`KeyboardViewController`) and the number/symbol pages (`SymbolPageView`) so the
/// two never drift.
enum KeyCap {
    // Native-feel metrics (portrait iPhone), shared across every page.
    static let hMargin: CGFloat = 3
    static let hGap: CGFloat = 6
    static let vGap: CGFloat = 11
    static let topPad: CGFloat = 6

    /// Styles `b` as a character key (`special == false`) or a function key.
    static func style(_ b: UIButton, title: String, special: Bool, theme: KeyboardTheme) {
        b.setTitle(title, for: .normal)
        b.titleLabel?.font = title.count > 1 ? .systemFont(ofSize: 16) : theme.keyFont
        b.setTitleColor(theme.keyText, for: .normal)
        b.backgroundColor = special ? theme.specialKeyBackground : theme.keyBackground
        b.layer.cornerRadius = theme.cornerRadius
        b.layer.cornerCurve = .continuous
        b.layer.shadowColor = theme.keyShadow.cgColor
        b.layer.shadowOpacity = 1
        b.layer.shadowRadius = 0
        b.layer.shadowOffset = CGSize(width: 0, height: 1)
        b.layer.masksToBounds = false
    }

    /// Replaces a function key's title with an SF Symbol glyph (e.g. delete/globe).
    static func setSymbol(_ b: UIButton, _ name: String, theme: KeyboardTheme) {
        b.setTitle(nil, for: .normal)
        b.setImage(UIImage(systemName: name,
                           withConfiguration: UIImage.SymbolConfiguration(pointSize: 19, weight: .regular)),
                   for: .normal)
        b.tintColor = theme.keyText
    }
}

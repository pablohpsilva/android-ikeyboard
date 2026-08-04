import UIKit

/// Visual palette for the FeatherKey keyboard, tuned to match the stock iOS
/// system keyboard in both light and dark appearances.
///
/// All colors are dynamic (resolved from a `UITraitCollection`) so a single
/// value renders correctly as the user switches between light and dark mode.
struct KeyboardTheme {
    let keyBackground: UIColor        // letter keys
    let keyText: UIColor
    let specialKeyBackground: UIColor // shift / delete / 123 / globe / return
    let specialKeyText: UIColor
    let keyboardBackground: UIColor
    let popupBackground: UIColor
    let keyShadow: UIColor
    let cornerRadius: CGFloat
    let keyFont: UIFont
    let popupFont: UIFont

    // MARK: - Resolution

    /// Returns the palette for the appearance described by `traits`.
    ///
    /// The returned colors are still dynamic `UIColor`s: inspecting the trait's
    /// `userInterfaceStyle` only selects which concrete values are baked in for
    /// callers that read `.cgColor` (which cannot itself adapt).
    static func resolved(for traits: UITraitCollection) -> KeyboardTheme {
        let dark = traits.userInterfaceStyle == .dark

        return KeyboardTheme(
            keyBackground: dark ? Palette.keyBackgroundDark : Palette.keyBackgroundLight,
            keyText: Palette.text,
            specialKeyBackground: dark ? Palette.specialKeyBackgroundDark
                                       : Palette.specialKeyBackgroundLight,
            specialKeyText: Palette.text,
            keyboardBackground: dark ? Palette.keyboardBackgroundDark
                                     : Palette.keyboardBackgroundLight,
            popupBackground: dark ? Palette.keyBackgroundDark : Palette.keyBackgroundLight,
            keyShadow: Palette.shadow,
            cornerRadius: 5,
            keyFont: .systemFont(ofSize: 23, weight: .regular),
            popupFont: .systemFont(ofSize: 30, weight: .regular)
        )
    }

    // MARK: - Concrete color values

    /// Stock-keyboard color constants for light and dark, plus the two adaptive
    /// system colors used unchanged across both appearances.
    private enum Palette {
        // Keyboard tray background.
        static let keyboardBackgroundLight =
            UIColor(red: 0.82, green: 0.83, blue: 0.86, alpha: 1)
        static let keyboardBackgroundDark = UIColor(white: 0.11, alpha: 1)

        // Letter (character) keys.
        static let keyBackgroundLight = UIColor.white
        static let keyBackgroundDark = UIColor(white: 0.42, alpha: 1)

        // Function keys: shift / delete / 123 / globe / return.
        static let specialKeyBackgroundLight =
            UIColor(red: 0.68, green: 0.70, blue: 0.74, alpha: 1)
        static let specialKeyBackgroundDark = UIColor(white: 0.28, alpha: 1)

        // Glyph color — adapts automatically, so shared across appearances.
        static let text = UIColor.label

        // Bottom-edge key shadow: a hard 1pt drop with no blur.
        static let shadow = UIColor.black.withAlphaComponent(0.30)
    }
}

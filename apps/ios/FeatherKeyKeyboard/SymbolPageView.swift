import UIKit

/// The number and symbol pages, rendered natively and mirroring the Android shell:
/// these are UI-owned pages whose keys insert their literal character directly (no
/// core decode — the core only owns the decodable alpha grid). The view rebuilds
/// itself on `configure(page:theme:)` and toggles between its two pages internally;
/// everything else is reported through the callbacks.
final class SymbolPageView: UIView {
    enum Page { case numbers, symbols }

    var onInsert: ((String) -> Void)?
    var onBackspace: (() -> Void)?
    var onReturn: (() -> Void)?
    var onGlobe: (() -> Void)?
    var onBackToAlpha: (() -> Void)?

    // Character rows — the exact literals the Android keyboard shows (KeyboardView.kt).
    private static let numbersR1 = "1234567890".map(String.init)
    private static let numbersR2 = ["-", "/", ":", ";", "(", ")", "$", "&", "@", "\""]
    private static let symbolsR1 = ["[", "]", "{", "}", "#", "%", "^", "*", "+", "="]
    private static let symbolsR2 = ["_", "\\", "|", "~", "<", ">", "€", "£", "¥", "•"]
    private static let punctR3 = [".", ",", "?", "!", "'"]

    private var theme = KeyboardTheme.resolved(for: UITraitCollection(userInterfaceStyle: .light))
    private var page: Page = .numbers

    private var row1: [UIButton] = []
    private var row2: [UIButton] = []
    private var punct: [UIButton] = []
    private let toggleKey = UIButton(type: .system)     // "#+=" ⇄ "123"
    private let backspaceKey = UIButton(type: .system)
    private let abcKey = UIButton(type: .system)        // back to the alpha page
    private let globeKey = UIButton(type: .system)
    private let spaceKey = UIButton(type: .system)
    private let returnKey = UIButton(type: .system)

    /// (Re)builds the page for `page` in `theme`, preserving the callbacks.
    func configure(page: Page, theme: KeyboardTheme) {
        self.page = page
        self.theme = theme
        rebuild()
        setNeedsLayout()
    }

    /// Re-applies `theme` to the page currently shown (e.g. on a light/dark change).
    func retheme(_ theme: KeyboardTheme) { configure(page: page, theme: theme) }

    // MARK: - Build

    private func rebuild() {
        row1.forEach { $0.removeFromSuperview() }
        row2.forEach { $0.removeFromSuperview() }
        punct.forEach { $0.removeFromSuperview() }
        let r1 = page == .numbers ? Self.numbersR1 : Self.symbolsR1
        let r2 = page == .numbers ? Self.numbersR2 : Self.symbolsR2
        row1 = r1.map { charKey($0) }
        row2 = r2.map { charKey($0) }
        punct = Self.punctR3.map { charKey($0) }

        special(toggleKey, title: page == .numbers ? "#+=" : "123") { [weak self] in
            self?.configure(page: self?.page == .numbers ? .symbols : .numbers, theme: self!.theme)
        }
        special(backspaceKey, symbol: "delete.left") { [weak self] in self?.onBackspace?() }
        special(abcKey, title: "ABC") { [weak self] in self?.onBackToAlpha?() }
        special(globeKey, symbol: "globe") { [weak self] in self?.onGlobe?() }
        special(spaceKey, title: "space") { [weak self] in self?.onInsert?(" ") }
        special(returnKey, title: "return") { [weak self] in self?.onReturn?() }
    }

    private func charKey(_ label: String) -> UIButton {
        let b = UIButton(type: .system)
        KeyCap.style(b, title: label, special: false, theme: theme)
        b.addAction(UIAction { [weak self] _ in self?.onInsert?(label) }, for: .touchUpInside)
        addSubview(b)
        return b
    }

    private func special(_ b: UIButton, title: String? = nil, symbol: String? = nil,
                         _ action: @escaping () -> Void) {
        b.removeTarget(nil, action: nil, for: .allEvents)
        KeyCap.style(b, title: title ?? "", special: true, theme: theme)
        if let symbol { KeyCap.setSymbol(b, symbol, theme: theme) }
        b.addAction(UIAction { _ in action() }, for: .touchUpInside)
        if b.superview == nil { addSubview(b) }
    }

    // MARK: - Layout (4 rows; row 3 flanked by toggle/⌫; row 4 = ABC/globe/space/return)

    override func layoutSubviews() {
        super.layoutSubviews()
        let W = bounds.width
        let top = KeyCap.topPad
        let zoneH = bounds.height - 2 * KeyCap.topPad
        guard zoneH > 0, W > 0 else { return }
        let rowH = (zoneH - 3 * KeyCap.vGap) / 4
        let unit = (W - 2 * KeyCap.hMargin - 9 * KeyCap.hGap) / 10
        let keyH = min(rowH, unit * 1.4)
        let vInset = (rowH - keyH) / 2
        func y(_ r: Int) -> CGFloat { top + CGFloat(r) * (rowH + KeyCap.vGap) + vInset }

        layoutCharRow(row1, y: y(0), unit: unit, h: keyH)
        layoutCharRow(row2, y: y(1), unit: unit, h: keyH)
        layoutRow3(y: y(2), unit: unit, h: keyH, W: W)
        layoutRow4(y: y(3), unit: unit, h: keyH, W: W)
    }

    private func layoutCharRow(_ buttons: [UIButton], y: CGFloat, unit: CGFloat, h: CGFloat) {
        var x = KeyCap.hMargin
        for b in buttons { b.frame = CGRect(x: x, y: y, width: unit, height: h); x += unit + KeyCap.hGap }
    }

    private func layoutRow3(y: CGFloat, unit: CGFloat, h: CGFloat, W: CGFloat) {
        let side = unit * 1.3
        let punctW = (W - 2 * KeyCap.hMargin - 2 * side - 6 * KeyCap.hGap) / CGFloat(punct.count)
        var x = KeyCap.hMargin
        toggleKey.frame = CGRect(x: x, y: y, width: side, height: h); x += side + KeyCap.hGap
        for b in punct { b.frame = CGRect(x: x, y: y, width: punctW, height: h); x += punctW + KeyCap.hGap }
        backspaceKey.frame = CGRect(x: W - KeyCap.hMargin - side, y: y, width: side, height: h)
    }

    private func layoutRow4(y: CGFloat, unit: CGFloat, h: CGFloat, W: CGFloat) {
        let abcW = unit * 1.4
        let retW = unit * 2.0
        abcKey.frame = CGRect(x: KeyCap.hMargin, y: y, width: abcW, height: h)
        globeKey.frame = CGRect(x: abcKey.frame.maxX + KeyCap.hGap, y: y, width: unit, height: h)
        returnKey.frame = CGRect(x: W - KeyCap.hMargin - retW, y: y, width: retW, height: h)
        let spaceX = globeKey.frame.maxX + KeyCap.hGap
        spaceKey.frame = CGRect(x: spaceX, y: y, width: returnKey.frame.minX - KeyCap.hGap - spaceX, height: h)
    }
}

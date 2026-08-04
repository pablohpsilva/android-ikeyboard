import UIKit
import FeatherKeyKit

/// The keyboard extension: renders the core's key layout in a native iOS row
/// structure and routes taps. It owns ONLY platform concerns — every character
/// and every suggestion comes from the shared core via `KeyboardEngine`.
final class KeyboardViewController: UIInputViewController {
    private var engine: KeyboardEngine?
    private var rows: [[EngineKey]] = []            // letters grouped into rows by y
    private var shifted = false
    private var initError: String?

    // Letter buttons keyed back to their EngineKey (for core-decode by identity).
    private var letterButtons: [(button: UIButton, key: EngineKey)] = []
    private var shiftKey = UIButton(type: .system)
    private var backspaceKey = UIButton(type: .system)
    private var numbersKey = UIButton(type: .system)
    private var globeKey = UIButton(type: .system)
    private var returnKey = UIButton(type: .system)
    private var spaceKey = UIButton(type: .system)

    /// Test seam: overrides the engine's store container (each test render needs
    /// its own, since redb locks the store file).
    var containerDirOverride: URL?

    private var theme = KeyboardTheme.resolved(for: UITraitCollection(userInterfaceStyle: .light))
    /// Armed for exactly one event after an autocorrect: the next backspace reverts
    /// it, any other key accepts it (both clear the slot). See `WordBoundary`.
    private var pendingRevert: WordBoundary.Revert?
    private let strip = SuggestionStripView()
    private let popup = KeyPopupView()
    private let symbolPage = SymbolPageView()        // number/symbol pages (hidden on alpha)
    private var symbolsVisible = false
    private let stripHeight = SuggestionStripView.preferredHeight

    // Swipe/glide typing (BR-41). The decode is the shared core's; the shell only
    // captures the path, projects it into the logical frame, and commits the word.
    private var swipeTracker = SwipeTracker()
    private var projection: LayoutProjection?
    private var swipeActive = false
    /// After a swipe: the just-committed word and the other candidates, shown in the
    /// strip. Picking one replaces the committed word. Cleared once the user types on.
    private var lastSwipeWord: String?
    private var swipeAlternatives: [String] = []

    // Native-feel metrics — shared with the number/symbol pages via KeyCap.
    private let hMargin = KeyCap.hMargin
    private let hGap = KeyCap.hGap
    private let vGap = KeyCap.vGap
    private let topPad = KeyCap.topPad

    override func viewDidLoad() {
        super.viewDidLoad()
        theme = KeyboardTheme.resolved(for: traitCollection)
        do {
            let dir = containerDirOverride
                ?? FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
            // The bundled word lists live in FeatherKeyKit's linking bundle (this
            // extension, or the test bundle under snapshot). English only for now;
            // multi-language arrives with the language-switching slice.
            let langs = BundledLexicon.load(tags: ["en"],
                                            from: Bundle(for: CoreKeyboardEngine.self))
            let e = try CoreKeyboardEngine(containerDir: dir, languages: langs)
            engine = e
            rows = Dictionary(grouping: e.layoutKeys(), by: { $0.y })
                .sorted { $0.key < $1.key }
                .map { $0.value.sorted { $0.x < $1.x } }
        } catch {
            initError = "\(error)"
            NSLog("FeatherKey: engine init failed: \(error)")
        }
        buildViews()
    }

    override func viewWillAppear(_ animated: Bool) {
        super.viewWillAppear(animated)
        refreshSuggestions()
    }

    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()
        layoutKeyboard()
    }

    // MARK: - Build

    private func buildViews() {
        view.backgroundColor = theme.keyboardBackground
        strip.onPick = { [weak self] i in self?.pickSuggestion(i) }
        view.addSubview(strip)

        if rows.isEmpty {
            let label = UILabel()
            label.text = initError.map { "FeatherKey: \($0)" } ?? "FeatherKey: no layout keys"
            label.numberOfLines = 0; label.textAlignment = .center
            label.font = .systemFont(ofSize: 12); label.textColor = .secondaryLabel
            label.translatesAutoresizingMaskIntoConstraints = false
            view.addSubview(label)
            NSLayoutConstraint.activate([
                label.centerXAnchor.constraint(equalTo: view.centerXAnchor),
                label.centerYAnchor.constraint(equalTo: view.centerYAnchor),
                label.widthAnchor.constraint(equalTo: view.widthAnchor, constant: -24),
            ])
            return
        }

        for row in rows {
            for key in row {
                let b = styleKey(UIButton(type: .system), title: key.label, special: false)
                b.addTarget(self, action: #selector(letterDown(_:)), for: .touchDown)
                b.addTarget(self, action: #selector(letterUpInside(_:)), for: .touchUpInside)
                b.addTarget(self, action: #selector(letterUpOutside(_:)),
                            for: [.touchUpOutside, .touchCancel, .touchDragExit])
                view.addSubview(b)
                letterButtons.append((b, key))
            }
        }
        configureSpecial(shiftKey, symbol: "shift") { [weak self] in self?.toggleShift() }
        configureSpecial(backspaceKey, symbol: "delete.left") { [weak self] in self?.backspace() }
        configureSpecial(numbersKey, title: "123") { [weak self] in self?.showSymbols() }
        configureSpecial(globeKey, symbol: "globe") { [weak self] in self?.advanceToNextInputMode() }
        configureSpecial(returnKey, title: "return") { [weak self] in self?.insert("\n") }
        configureSpecial(spaceKey, title: "space") { [weak self] in self?.space() }

        symbolPage.isHidden = true
        symbolPage.onInsert = { [weak self] in $0 == " " ? self?.space() : self?.insert($0) }
        symbolPage.onBackspace = { [weak self] in self?.backspace() }
        symbolPage.onReturn = { [weak self] in self?.insert("\n") }
        symbolPage.onGlobe = { [weak self] in self?.advanceToNextInputMode() }
        symbolPage.onBackToAlpha = { [weak self] in self?.hideSymbols() }
        view.addSubview(symbolPage)

        // A pan only begins after the finger moves, so quick taps still reach the
        // letter buttons (touchUpInside) — that IS the "no conflict with quick taps"
        // guarantee (BR-41). Once it begins it cancels the button touch, so a glide
        // never also inserts its starting letter.
        let pan = UIPanGestureRecognizer(target: self, action: #selector(handleSwipe(_:)))
        pan.cancelsTouchesInView = true
        view.addGestureRecognizer(pan)
    }

    // MARK: - Page switching (alpha ⇄ number/symbol pages)

    private var alphaButtons: [UIButton] {
        letterButtons.map { $0.button } + [shiftKey, backspaceKey, numbersKey, globeKey, returnKey, spaceKey]
    }

    private func showSymbols() {
        symbolsVisible = true
        symbolPage.configure(page: .numbers, theme: theme)
        alphaButtons.forEach { $0.isHidden = true }
        symbolPage.isHidden = false
        view.setNeedsLayout()
    }

    private func hideSymbols() {
        symbolsVisible = false
        symbolPage.isHidden = true
        alphaButtons.forEach { $0.isHidden = false }
        view.setNeedsLayout()
    }

    private func configureSpecial(_ b: UIButton, title: String? = nil, symbol: String? = nil,
                                  _ action: @escaping () -> Void) {
        KeyCap.style(b, title: title ?? "", special: true, theme: theme)
        if let symbol { KeyCap.setSymbol(b, symbol, theme: theme) }
        b.addAction(UIAction { _ in action() }, for: .touchUpInside)
        view.addSubview(b)
    }

    @discardableResult
    private func styleKey(_ b: UIButton, title: String, special: Bool) -> UIButton {
        KeyCap.style(b, title: title, special: special, theme: theme)
        return b
    }

    // MARK: - Native layout (shift + backspace flank row 3; bottom row = 123/globe/space/return)

    private func layoutKeyboard() {
        strip.frame = CGRect(x: 0, y: 0, width: view.bounds.width, height: stripHeight)
        symbolPage.frame = CGRect(x: 0, y: stripHeight,
                                  width: view.bounds.width, height: view.bounds.height - stripHeight)
        guard !rows.isEmpty, rows.count == 3 else { return }
        let W = view.bounds.width
        let top = stripHeight + topPad
        let zoneH = view.bounds.height - top - topPad
        let rowH = (zoneH - 3 * vGap) / 4
        // Unit key width from the 10-key top row; cap key height for native proportions.
        let unit = (W - 2 * hMargin - CGFloat(rows[0].count - 1) * hGap) / CGFloat(rows[0].count)
        let keyH = min(rowH, unit * 1.4)
        let vInset = (rowH - keyH) / 2
        let side = unit * 1.3           // shift / backspace
        let numW = unit * 1.4           // 123
        let retW = unit * 2.0           // return (fits the word)

        func y(_ r: Int) -> CGFloat { top + CGFloat(r) * (rowH + vGap) + vInset }

        layoutRow(letterButtons(forRow: 0).map { $0.button }, y: y(0), h: keyH, keyW: unit,
                  totalWidth: CGFloat(rows[0].count) * unit + CGFloat(rows[0].count - 1) * hGap)
        layoutRow(letterButtons(forRow: 1).map { $0.button }, y: y(1), h: keyH, keyW: unit,
                  totalWidth: CGFloat(rows[1].count) * unit + CGFloat(rows[1].count - 1) * hGap)

        // Row 3: shift | 7 letters | backspace, centered.
        let mid = letterButtons(forRow: 2).map { $0.button }
        let midW = CGFloat(mid.count) * unit + CGFloat(mid.count - 1) * hGap
        let row3W = side + hGap + midW + hGap + side
        var x = (W - row3W) / 2
        shiftKey.frame = CGRect(x: x, y: y(2), width: side, height: keyH); x += side + hGap
        for b in mid { b.frame = CGRect(x: x, y: y(2), width: unit, height: keyH); x += unit + hGap }
        backspaceKey.frame = CGRect(x: x, y: y(2), width: side, height: keyH)

        // Row 4: 123 | globe | space | return.
        let by = y(3)
        numbersKey.frame = CGRect(x: hMargin, y: by, width: numW, height: keyH)
        globeKey.frame = CGRect(x: numbersKey.frame.maxX + hGap, y: by, width: unit, height: keyH)
        returnKey.frame = CGRect(x: W - hMargin - retW, y: by, width: retW, height: keyH)
        let spaceX = globeKey.frame.maxX + hGap
        spaceKey.frame = CGRect(x: spaceX, y: by, width: returnKey.frame.minX - hGap - spaceX, height: keyH)
        rebuildProjection()
    }

    /// Fit the screen→logical projection from the rendered letter buttons' centres
    /// (screen) to their `EngineKey` centres (logical). Rebuilt on every layout so it
    /// tracks rotation / size-class changes.
    private func rebuildProjection() {
        guard !letterButtons.isEmpty else { projection = nil; return }
        let pairs = letterButtons.map {
            (screen: GesturePoint(x: Float($0.button.frame.midX), y: Float($0.button.frame.midY)),
             logical: GesturePoint(x: $0.key.x + $0.key.width / 2, y: $0.key.y + $0.key.height / 2))
        }
        projection = LayoutProjection(pairs: pairs)
    }

    private func letterButtons(forRow r: Int) -> [(button: UIButton, key: EngineKey)] {
        let counts = rows.map { $0.count }
        let start = counts.prefix(r).reduce(0, +)
        return Array(letterButtons[start..<(start + counts[r])])
    }

    private func layoutRow(_ buttons: [UIButton], y: CGFloat, h: CGFloat, keyW: CGFloat, totalWidth: CGFloat) {
        var x = (view.bounds.width - totalWidth) / 2
        for b in buttons { b.frame = CGRect(x: x, y: y, width: keyW, height: h); x += keyW + hGap }
    }

    // MARK: - Key handling (letters go through the CORE, by key identity)

    @objc private func letterDown(_ sender: UIButton) {
        popup.show(label: sender.title(for: .normal) ?? "", over: sender.frame, in: view, theme: theme)
    }
    @objc private func letterUpOutside(_ sender: UIButton) { popup.hide() }

    @objc private func letterUpInside(_ sender: UIButton) {
        popup.hide()
        guard let engine, let entry = letterButtons.first(where: { $0.button === sender }) else { return }
        let k = entry.key
        // Decode at the key's own logical centre — the core still decides the char.
        let ch = (try? engine.decode(atLogicalX: k.x + k.width / 2, y: k.y + k.height / 2)) ?? ""
        guard !ch.isEmpty else { return }
        insert(shifted ? ch.uppercased() : ch)
        if shifted { toggleShift() }
    }

    // MARK: - Swipe / glide typing (the decode is the shared core's — BR-41)

    @objc private func handleSwipe(_ g: UIPanGestureRecognizer) {
        guard !symbolsVisible else { return }
        let p = g.location(in: view)
        let pt = GesturePoint(x: Float(p.x), y: Float(p.y))
        switch g.state {
        case .began:
            swipeActive = inLetterZone(p)
            if swipeActive { swipeTracker.begin(at: pt) }
        case .changed:
            if swipeActive { swipeTracker.move(to: pt) }
        case .ended:
            if swipeActive { finishSwipe() }
            swipeActive = false
        default:
            swipeActive = false
        }
    }

    /// True when `p` falls within the letter rows (not the strip or bottom bar), so a
    /// drag on space/return/backspace is not mistaken for a glide.
    private func inLetterZone(_ p: CGPoint) -> Bool {
        let frames = letterButtons.map { $0.button.frame }
        guard let minY = frames.map(\.minY).min(), let maxY = frames.map(\.maxY).max() else {
            return false
        }
        return p.y >= minY && p.y <= maxY
    }

    /// Decode the captured path through the core and commit the best word, unless the
    /// path was too small to be a swipe (then the tap path already handled it).
    private func finishSwipe() {
        popup.hide()
        guard let engine, let projection else { return }
        let pitch = Float(letterButtons.first?.button.frame.width ?? 30)
        guard swipeTracker.isSwipe(keyPitch: pitch) else { return }
        let logical = swipeTracker.path.map { projection.toLogical($0) }
        let words = engine.decodeGesture(points: logical)
        guard let word = words.first else { return }
        commitSwipe(word: shifted ? capitalizedFirst(word) : word,
                    alternatives: Array(words.dropFirst()))
        if shifted { toggleShift() }
    }

    /// Commit a swiped `word`: replace any in-progress prefix, insert the word and a
    /// trailing space, and offer the other candidates in the strip (BR-41).
    private func commitSwipe(word: String, alternatives: [String]) {
        pendingRevert = nil
        let ctx = textDocumentProxy.documentContextBeforeInput ?? ""
        let prefix = trailingWord(ctx)
        for _ in 0..<prefix.count { textDocumentProxy.deleteBackward() }
        textDocumentProxy.insertText(word + " ")
        lastSwipeWord = word
        swipeAlternatives = alternatives
        if alternatives.isEmpty {
            refreshSuggestions()
        } else {
            strip.configure(Array(alternatives.prefix(3)), theme: theme)
        }
    }

    private func capitalizedFirst(_ w: String) -> String {
        w.prefix(1).uppercased() + w.dropFirst()
    }

    /// The user typed on past a swipe, so its alternatives no longer apply.
    private func clearSwipeState() {
        lastSwipeWord = nil
        swipeAlternatives = []
    }

    /// A plain insertion (letter, symbol, newline). Typing anything after an
    /// autocorrect accepts it, so the revert slot is cleared.
    private func insert(_ s: String) {
        pendingRevert = nil
        clearSwipeState()
        textDocumentProxy.insertText(s)
        refreshSuggestions()
    }

    /// Space is a word boundary: ask the core whether to replace the just-typed word
    /// (proper-case first, then an edit-distance correction), apply it, and arm a
    /// one-tap revert — then commit the space. The decision itself is `WordBoundary`,
    /// core-driven; this method is only the field I/O around it.
    private func space() {
        clearSwipeState()
        let ctx = textDocumentProxy.documentContextBeforeInput ?? ""
        let word = trailingWord(ctx)
        pendingRevert = nil
        if !word.isEmpty, let engine {
            let preceding = String(ctx.dropLast(word.count))
            let d = WordBoundary.decide(typed: word, precedingText: preceding, engine: engine)
            if d.isCorrection {
                for _ in 0..<word.count { textDocumentProxy.deleteBackward() }
                textDocumentProxy.insertText(d.resolved)
                pendingRevert = WordBoundary.Revert(typed: word, corrected: d.resolved)
            }
        }
        textDocumentProxy.insertText(" ")
        refreshSuggestions()
    }

    /// Backspace immediately after an autocorrect restores the exact typed word
    /// (native-iOS revert); otherwise it deletes one character as usual.
    private func backspace() {
        clearSwipeState()
        let ctx = textDocumentProxy.documentContextBeforeInput ?? ""
        if let edit = WordBoundary.revert(pendingRevert, precedingText: ctx) {
            for _ in 0..<edit.delete { textDocumentProxy.deleteBackward() }
            textDocumentProxy.insertText(edit.insert)
            pendingRevert = nil
            refreshSuggestions()
            return
        }
        pendingRevert = nil
        textDocumentProxy.deleteBackward()
        refreshSuggestions()
    }

    /// The word in progress just before the cursor: the run of characters after the
    /// last space or newline (empty if the field ends in whitespace).
    private func trailingWord(_ ctx: String) -> String {
        String(ctx.reversed().prefix { $0 != " " && $0 != "\n" }.reversed())
    }

    private func toggleShift() {
        shifted.toggle()
        for (b, key) in letterButtons {
            b.setTitle(shifted ? key.label.uppercased() : key.label, for: .normal)
        }
        shiftKey.backgroundColor = shifted ? theme.keyBackground : theme.specialKeyBackground
    }

    // MARK: - Suggestions (from the shared core)

    private func context() -> (preceding: String, prefix: String) {
        let ctx = textDocumentProxy.documentContextBeforeInput ?? ""
        let endsSpace = ctx.last == " " || ctx.last == "\n"
        let words = ctx.split(whereSeparator: { $0 == " " || $0 == "\n" }).map(String.init)
        let prefix = endsSpace ? "" : (words.last ?? "")
        let preceding = endsSpace ? (words.last ?? "") : (words.count >= 2 ? words[words.count - 2] : "")
        return (preceding, prefix)
    }

    private func refreshSuggestions() {
        guard let engine else { strip.configure([], theme: theme); return }
        let c = context()
        strip.configure(Array(engine.suggestions(preceding: c.preceding, prefix: c.prefix).prefix(3)), theme: theme)
    }

    private func pickSuggestion(_ index: Int) {
        // After a swipe, the strip shows the other glide candidates: picking one
        // replaces the just-committed word (and its trailing space) in place.
        if let last = lastSwipeWord, index < swipeAlternatives.count {
            let alt = swipeAlternatives[index]
            pendingRevert = nil
            for _ in 0..<(last.count + 1) { textDocumentProxy.deleteBackward() }
            textDocumentProxy.insertText(alt + " ")
            clearSwipeState()
            refreshSuggestions()
            return
        }
        guard let engine else { return }
        let c = context()
        let sugg = engine.suggestions(preceding: c.preceding, prefix: c.prefix)
        guard index < sugg.count else { return }
        pendingRevert = nil
        for _ in 0..<c.prefix.count { textDocumentProxy.deleteBackward() }
        textDocumentProxy.insertText(sugg[index] + " ")
        refreshSuggestions()
    }

    // MARK: - Appearance

    override func traitCollectionDidChange(_ previous: UITraitCollection?) {
        super.traitCollectionDidChange(previous)
        guard traitCollection.hasDifferentColorAppearance(comparedTo: previous) else { return }
        theme = KeyboardTheme.resolved(for: traitCollection)
        view.backgroundColor = theme.keyboardBackground
        symbolPage.retheme(theme)
        let specials: Set<UIButton> = [shiftKey, backspaceKey, numbersKey, globeKey, returnKey, spaceKey]
        for b in letterButtons.map({ $0.button }) + Array(specials) {
            let special = specials.contains(b)
            b.backgroundColor = special ? theme.specialKeyBackground : theme.keyBackground
            b.setTitleColor(theme.keyText, for: .normal)
            b.layer.shadowColor = theme.keyShadow.cgColor
        }
        refreshSuggestions()
    }
}

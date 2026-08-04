import UIKit

/// Predictive-suggestion bar shaped like the native iOS QuickType strip.
///
/// Shows up to three centered suggestions separated by thin vertical
/// hairlines. An empty array yields a blank bar. Tapping a suggestion
/// fires `onPick` with that suggestion's index.
final class SuggestionStripView: UIView {

    /// Called with the index (0-based) of the tapped suggestion.
    var onPick: ((Int) -> Void)?

    /// Native predictive bar height.
    static let preferredHeight: CGFloat = 44

    private let stack: UIStackView = {
        let s = UIStackView()
        s.axis = .horizontal
        s.distribution = .fillEqually
        s.alignment = .fill
        s.spacing = 0
        s.translatesAutoresizingMaskIntoConstraints = false
        return s
    }()

    private var buttons: [UIButton] = []
    private var hairlines: [UIView] = []
    private var currentTheme: KeyboardTheme?

    override init(frame: CGRect) {
        super.init(frame: frame)
        setUp()
    }

    required init?(coder: NSCoder) {
        super.init(coder: coder)
        setUp()
    }

    private func setUp() {
        addSubview(stack)
        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: leadingAnchor),
            stack.trailingAnchor.constraint(equalTo: trailingAnchor),
            stack.topAnchor.constraint(equalTo: topAnchor),
            stack.bottomAnchor.constraint(equalTo: bottomAnchor),
        ])
    }

    override var intrinsicContentSize: CGSize {
        CGSize(width: UIView.noIntrinsicMetric, height: Self.preferredHeight)
    }

    /// Populate the bar with up to three suggestions rendered with `theme`.
    func configure(_ suggestions: [String], theme: KeyboardTheme) {
        currentTheme = theme
        backgroundColor = theme.keyboardBackground

        // Rebuild from scratch: cheap for <=3 items, avoids stale state.
        for view in stack.arrangedSubviews {
            stack.removeArrangedSubview(view)
            view.removeFromSuperview()
        }
        buttons.removeAll()
        hairlines.removeAll()

        let items = Array(suggestions.prefix(3))
        guard !items.isEmpty else { return }

        for (index, text) in items.enumerated() {
            if index > 0 {
                let line = makeHairline(theme: theme)
                hairlines.append(line)
                stack.addArrangedSubview(line)
            }
            let button = makeButton(title: text, index: index, theme: theme)
            buttons.append(button)
            stack.addArrangedSubview(button)
        }
    }

    private func makeButton(title: String, index: Int, theme: KeyboardTheme) -> UIButton {
        let button = UIButton(type: .system)
        button.setTitle(title, for: .normal)
        button.setTitleColor(theme.keyText, for: .normal)
        button.titleLabel?.font = theme.keyFont
        button.titleLabel?.adjustsFontSizeToFitWidth = true
        button.titleLabel?.minimumScaleFactor = 0.7
        button.titleLabel?.lineBreakMode = .byTruncatingTail
        button.contentEdgeInsets = UIEdgeInsets(top: 0, left: 6, bottom: 0, right: 6)
        button.tag = index
        button.addTarget(self, action: #selector(pick(_:)), for: .touchUpInside)
        return button
    }

    private func makeHairline(theme: KeyboardTheme) -> UIView {
        let container = UIView()
        container.backgroundColor = .clear
        container.translatesAutoresizingMaskIntoConstraints = false
        container.widthAnchor.constraint(equalToConstant: 1).isActive = true

        let line = UIView()
        line.backgroundColor = theme.specialKeyBackground
        line.translatesAutoresizingMaskIntoConstraints = false
        container.addSubview(line)
        NSLayoutConstraint.activate([
            line.centerXAnchor.constraint(equalTo: container.centerXAnchor),
            line.centerYAnchor.constraint(equalTo: container.centerYAnchor),
            line.widthAnchor.constraint(equalToConstant: 1),
            line.heightAnchor.constraint(equalTo: container.heightAnchor, multiplier: 0.5),
        ])
        return container
    }

    @objc private func pick(_ sender: UIButton) {
        onPick?(sender.tag)
    }

    override func traitCollectionDidChange(_ previous: UITraitCollection?) {
        super.traitCollectionDidChange(previous)
        if #available(iOS 13.0, *),
           traitCollection.hasDifferentColorAppearance(comparedTo: previous) {
            // Re-resolve dynamic colors/fonts for the new appearance.
            let titles = buttons.map { $0.title(for: .normal) ?? "" }
            configure(titles, theme: KeyboardTheme.resolved(for: traitCollection))
        }
    }
}

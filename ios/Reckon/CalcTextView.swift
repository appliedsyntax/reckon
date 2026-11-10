import UIKit

/// A text view that shows each line's result in a column on the right,
/// aligned with the line using TextKit's line fragments.
final class CalcTextView: UITextView, UIGestureRecognizerDelegate {
    var onResultTap: ((String) -> Void)?
    var results: [String] = [] {
        didSet { if results != oldValue { setNeedsLayout() } }
    }

    private let resultsWidth: CGFloat = 132
    private var labels: [UILabel] = []
    private let divider = UIView()

    init() {
        // Explicit TextKit 1 stack so we can query line fragment rects.
        let storage = NSTextStorage()
        let layout = NSLayoutManager()
        let container = NSTextContainer(size: .zero)
        container.widthTracksTextView = true
        layout.addTextContainer(container)
        storage.addLayoutManager(layout)
        super.init(frame: .zero, textContainer: container)

        font = .monospacedSystemFont(ofSize: 16, weight: .regular)  // SF Mono
        textColor = .label
        backgroundColor = .systemBackground
        textContainerInset = UIEdgeInsets(top: 14, left: 12, bottom: 60, right: resultsWidth + 22)
        // Calculator input: no autocorrect, and keep straight quotes and
        // plain hyphens (they mean comments and minus).
        autocorrectionType = .no
        autocapitalizationType = .none
        smartQuotesType = .no
        smartDashesType = .no
        smartInsertDeleteType = .no
        spellCheckingType = .no
        alwaysBounceVertical = true
        keyboardDismissMode = .interactive

        divider.backgroundColor = .separator
        addSubview(divider)

        let tapBelow = UITapGestureRecognizer(target: self, action: #selector(tappedBelowText(_:)))
        tapBelow.cancelsTouchesInView = false
        tapBelow.delegate = self
        addGestureRecognizer(tapBelow)
    }

    // Let the "tap below text" recognizer run alongside UITextView's own.
    func gestureRecognizer(_ g: UIGestureRecognizer,
                                    shouldRecognizeSimultaneouslyWith other: UIGestureRecognizer) -> Bool {
        true
    }

    /// Tapping the empty space under the last line starts editing at the end.
    @objc private func tappedBelowText(_ g: UITapGestureRecognizer) {
        let usedBottom = layoutManager.usedRect(for: textContainer).maxY + textContainerInset.top
        guard g.location(in: self).y > usedBottom else { return }
        selectedRange = NSRange(location: (text as NSString).length, length: 0)
        if !isFirstResponder { becomeFirstResponder() }
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("not used") }

    override func layoutSubviews() {
        super.layoutSubviews()
        layoutResults()
    }

    private func makeLabel() -> UILabel {
        let l = UILabel()
        l.textAlignment = .right
        l.font = .monospacedSystemFont(ofSize: 16, weight: .medium)
        l.textColor = .systemGreen
        l.adjustsFontSizeToFitWidth = true
        l.minimumScaleFactor = 0.6
        l.isUserInteractionEnabled = true
        l.addGestureRecognizer(UITapGestureRecognizer(target: self, action: #selector(tapped(_:))))
        addSubview(l)
        return l
    }

    @objc private func tapped(_ g: UITapGestureRecognizer) {
        if let text = (g.view as? UILabel)?.text, !text.isEmpty {
            onResultTap?(text)
        }
    }

    private func layoutResults() {
        let x = bounds.width - resultsWidth - 12
        divider.frame = CGRect(x: x - 8, y: 0, width: 1 / UIScreen.main.scale,
                               height: max(contentSize.height, bounds.height))

        let ns = text as NSString
        var starts = [0]
        var i = 0
        while i < ns.length {
            let r = ns.range(of: "\n", options: [], range: NSRange(location: i, length: ns.length - i))
            if r.location == NSNotFound { break }
            starts.append(r.location + 1)
            i = r.location + 1
        }
        layoutManager.ensureLayout(for: textContainer)
        while labels.count < results.count { labels.append(makeLabel()) }
        for (idx, label) in labels.enumerated() {
            guard idx < results.count, idx < starts.count, !results[idx].isEmpty else {
                label.isHidden = true
                continue
            }
            let start = starts[idx]
            let rect: CGRect
            if start < ns.length {
                let glyph = layoutManager.glyphIndexForCharacter(at: start)
                rect = layoutManager.lineFragmentRect(forGlyphAt: glyph, effectiveRange: nil)
            } else {
                rect = layoutManager.extraLineFragmentRect
            }
            label.isHidden = false
            label.text = results[idx]
            label.frame = CGRect(x: x, y: rect.minY + textContainerInset.top,
                                 width: resultsWidth, height: rect.height)
        }
    }
}

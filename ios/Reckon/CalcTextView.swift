import UIKit

/// One highlighted range from the engine, in UTF-16 units from the start of
/// its line.
struct HighlightSpan: Equatable {
    let start: Int
    let length: Int
    let kind: String
}

/// A text view that shows each line's result in a column on the right,
/// aligned with the line using TextKit's line fragments, and paints the
/// current theme: highlight colours, the results column, the theme's
/// texture and the active line.
final class CalcTextView: UITextView, UIGestureRecognizerDelegate {
    var onResultTap: ((String) -> Void)?
    var results: [String] = [] {
        didSet { if results != oldValue { setNeedsLayout() } }
    }
    /// Highlights for each line.
    var spans: [[HighlightSpan]] = [] {
        didSet { if spans != oldValue { applyHighlights() } }
    }
    var theme: ReckonTheme? {
        didSet { if theme != oldValue { applyTheme() } }
    }

    static let lineHeight: CGFloat = 28
    private let resultsWidth: CGFloat = 132
    private let baseFont = UIFont.monospacedSystemFont(ofSize: 16, weight: .regular)
    private let boldFont = UIFont.monospacedSystemFont(ofSize: 16, weight: .bold)
    private var labels: [UILabel] = []
    private let backdrop = BackdropView()

    init() {
        // Explicit TextKit 1 stack so we can query line fragment rects.
        let storage = NSTextStorage()
        let layout = NSLayoutManager()
        let container = NSTextContainer(size: .zero)
        container.widthTracksTextView = true
        layout.addTextContainer(container)
        storage.addLayoutManager(layout)
        super.init(frame: .zero, textContainer: container)

        font = baseFont  // SF Mono
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

        backdrop.isUserInteractionEnabled = false
        backdrop.contentMode = .redraw
        insertSubview(backdrop, at: 0)

        let tapBelow = UITapGestureRecognizer(target: self, action: #selector(tappedBelowText(_:)))
        tapBelow.cancelsTouchesInView = false
        tapBelow.delegate = self
        addGestureRecognizer(tapBelow)
        applyTheme()
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("not used") }

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

    // MARK: Theme

    /// Every line is exactly `lineHeight` tall, with the glyphs centred, so
    /// results, rules and the grid line up with the text.
    private var baseAttributes: [NSAttributedString.Key: Any] {
        let p = NSMutableParagraphStyle()
        p.minimumLineHeight = Self.lineHeight
        p.maximumLineHeight = Self.lineHeight
        return [
            .font: baseFont,
            .foregroundColor: theme?.color("text") ?? UIColor.label,
            .paragraphStyle: p,
            .baselineOffset: (Self.lineHeight - baseFont.lineHeight) / 2,
        ]
    }

    private func applyTheme() {
        guard let theme else { return }
        backgroundColor = theme.color("bg")
        // The caret (and selection handles) use the tint colour. iOS draws
        // the caret itself, so Phosphor's block cursor is a bar here.
        tintColor = theme.color("cursor").withAlphaComponent(1)
        keyboardAppearance = theme.dark ? .dark : .light
        indicatorStyle = theme.dark ? .white : .black
        typingAttributes = baseAttributes
        for label in labels { style(label) }
        backdrop.theme = theme
        applyHighlights()
        setNeedsLayout()
        if isFirstResponder { reloadInputViews() }
    }

    private func style(_ label: UILabel) {
        label.font = .monospacedSystemFont(ofSize: 16, weight: theme?.resultFontWeight ?? .semibold)
        label.textColor = theme?.color("result") ?? .systemGreen
    }

    /// Re-colours the whole document from `spans`. Skipped while the
    /// keyboard is composing (marked text), which attribute changes break.
    func applyHighlights() {
        guard let theme, markedTextRange == nil else { return }
        let ns = textStorage.string as NSString
        let selection = selectedRange
        textStorage.beginEditing()
        textStorage.setAttributes(baseAttributes, range: NSRange(location: 0, length: ns.length))
        var lineStart = 0
        for lineSpans in spans {
            guard lineStart <= ns.length else { break }
            for s in lineSpans {
                let range = NSRange(location: lineStart + s.start, length: s.length)
                guard NSMaxRange(range) <= ns.length else { continue }
                let color = theme.color(s.kind == "error" ? "muted" : s.kind)
                textStorage.addAttribute(.foregroundColor, value: color, range: range)
                if s.kind == "header" {
                    textStorage.addAttribute(.font, value: boldFont, range: range)
                }
            }
            let nl = ns.range(of: "\n", range: NSRange(location: lineStart, length: ns.length - lineStart))
            if nl.location == NSNotFound { break }
            lineStart = nl.location + 1
        }
        textStorage.endEditing()
        typingAttributes = baseAttributes
        if selectedRange != selection { selectedRange = selection }
    }

    // MARK: Layout

    override func layoutSubviews() {
        super.layoutSubviews()
        layoutResults()
    }

    private func makeLabel() -> UILabel {
        let l = UILabel()
        l.textAlignment = .right
        style(l)
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
        let top = textContainerInset.top
        let rectForLine = { (start: Int) -> CGRect in
            if start < ns.length {
                let glyph = self.layoutManager.glyphIndexForCharacter(at: start)
                return self.layoutManager.lineFragmentRect(forGlyphAt: glyph, effectiveRange: nil)
            }
            return self.layoutManager.extraLineFragmentRect
        }
        while labels.count < results.count { labels.append(makeLabel()) }
        for (idx, label) in labels.enumerated() {
            guard idx < results.count, idx < starts.count, !results[idx].isEmpty else {
                label.isHidden = true
                continue
            }
            let rect = rectForLine(starts[idx])
            label.isHidden = false
            label.text = results[idx]
            label.frame = CGRect(x: x, y: rect.minY + top, width: resultsWidth, height: Self.lineHeight)
        }

        // The current line (all of its wrapped fragments).
        var active: CGRect?
        if selectedRange.length == 0, isFirstResponder {
            let caret = min(selectedRange.location, ns.length)
            let row = starts.lastIndex { $0 <= caret } ?? 0
            let first = rectForLine(starts[row])
            let endIndex = row + 1 < starts.count ? starts[row + 1] - 1 : ns.length
            let last = endIndex > starts[row] ? rectForLine(max(starts[row], endIndex - 1)) : first
            active = CGRect(x: 0, y: first.minY + top, width: bounds.width,
                            height: max(last.maxY, first.maxY) - first.minY)
        }
        // The backdrop covers only what is on screen and redraws on scroll.
        backdrop.frame = bounds
        backdrop.update(
            dividerX: x - 8,
            textTop: top,
            textLeft: textContainerInset.left,
            lastTextY: (ns.length > 0 ? layoutManager.usedRect(for: textContainer).maxY : 0) + top,
            active: active
        )
        sendSubviewToBack(backdrop)
    }
}

/// Draws behind the text: the results column, the theme's texture
/// (grid, rules or scanlines), the active line and the divider.
final class BackdropView: UIView {
    var theme: ReckonTheme? { didSet { setNeedsDisplay() } }
    private var dividerX: CGFloat = 0
    private var textTop: CGFloat = 0
    private var textLeft: CGFloat = 0
    private var lastTextY: CGFloat = 0
    private var active: CGRect?

    override init(frame: CGRect) {
        super.init(frame: frame)
        isOpaque = false
        backgroundColor = .clear
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("not used") }

    func update(dividerX: CGFloat, textTop: CGFloat, textLeft: CGFloat, lastTextY: CGFloat, active: CGRect?) {
        self.dividerX = dividerX
        self.textTop = textTop
        self.textLeft = textLeft
        self.lastTextY = lastTextY
        self.active = active
        setNeedsDisplay()
    }

    override func draw(_ rect: CGRect) {
        guard let theme, let ctx = UIGraphicsGetCurrentContext() else { return }
        // `bounds` is the scroll view's visible area, in content coordinates.
        let area = bounds
        let lh = CalcTextView.lineHeight
        let double = theme.doubleDivider
        let resultsLeft = dividerX + (double ? 4 : 1)

        ctx.setFillColor(theme.color("results_bg").cgColor)
        ctx.fill(CGRect(x: resultsLeft, y: area.minY, width: area.maxX - resultsLeft, height: area.height))

        let rule = theme.color("rule")
        let minor = rule.withAlphaComponent(rule.cgColor.alpha * 0.55)
        let hair = 1 / max(UIScreen.main.scale, 1)
        func hline(_ y: CGFloat, _ c: UIColor) {
            ctx.setFillColor(c.cgColor)
            ctx.fill(CGRect(x: area.minX, y: y, width: area.width, height: hair))
        }
        switch theme.pattern {
        case "grid":
            let step = lh / 2
            var k = Int(floor((area.minY - textTop) / step))
            while true {
                let y = textTop + CGFloat(k) * step
                if y > area.maxY { break }
                hline(y, k % 2 == 0 ? rule : minor)
                k += 1
            }
            var x = textLeft
            var j = 0
            while x < area.maxX {
                ctx.setFillColor((j % 2 == 0 ? rule : minor).cgColor)
                ctx.fill(CGRect(x: x, y: area.minY, width: hair, height: area.height))
                x += step
                j += 1
            }
        case "ruled":
            // A rule under every line, continuing down the empty page.
            var k = Int(floor((area.minY - textTop) / lh))
            while true {
                let y = textTop + CGFloat(k + 1) * lh - hair
                if y > area.maxY { break }
                if y >= textTop { hline(y, rule) }
                k += 1
            }
        case "scanlines":
            var y = floor(area.minY / 3) * 3
            while y < area.maxY {
                hline(y, rule)
                y += 3
            }
        default:
            break
        }

        if let active {
            ctx.setFillColor(theme.color("active_line").cgColor)
            ctx.fill(active)
        }

        ctx.setFillColor(theme.color("divider").cgColor)
        ctx.fill(CGRect(x: dividerX, y: area.minY, width: hair * 2, height: area.height))
        if double {
            ctx.fill(CGRect(x: dividerX + 3, y: area.minY, width: hair * 2, height: area.height))
        }
    }
}

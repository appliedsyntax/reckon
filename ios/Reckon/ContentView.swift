import SwiftUI
import UIKit

private let welcome = """
# Welcome to Reckon
// Type math in plain words. Tap a result to copy it.

Price: $20 + 56 EUR
Discounted: prev - 5% discount
20% of $75

# Units
1 meter 20 cm
100 °F in celsius
60 mph in km/h

# Time
today + 2 weeks
New York time

# Budget
Rent: $1,200
Food: $450
Total: sum
"""

@MainActor
final class CalcModel: ObservableObject {
    @Published var text: String
    @Published var results: [String] = []
    @Published var spans: [[HighlightSpan]] = []
    @Published var status: String = ""

    private let engine = ReckonEngine()
    private let docURL: URL = FileManager.default
        .urls(for: .documentDirectory, in: .userDomainMask)[0]
        .appendingPathComponent("document.reckon")

    init() {
        text = (try? String(contentsOf: docURL, encoding: .utf8)) ?? welcome
        recompute()
        status = "Rates: \(engine.ratesSource)"
        if engine.ratesStale { refreshRates() }
    }

    func update(_ newText: String) {
        text = newText
        recompute()
        try? newText.write(to: docURL, atomically: true, encoding: .utf8)
    }

    private func recompute() {
        let out = engine.evaluateHighlighted(text)
        results = out.map(\.result)
        spans = out.map(\.spans)
    }

    func copy(_ value: String) {
        UIPasteboard.general.string = value
        UIImpactFeedbackGenerator(style: .light).impactOccurred()
        status = "Copied \(value)"
    }

    func refreshRates() {
        guard let fiatURL = engine.fiatURL, let cryptoURL = engine.cryptoURL else { return }
        status = "Updating exchange rates…"
        Task {
            do {
                let (fiat, _) = try await URLSession.shared.data(from: fiatURL)
                let crypto = try? await URLSession.shared.data(from: cryptoURL).0
                if let err = engine.applyRates(
                    fiat: String(decoding: fiat, as: UTF8.self),
                    crypto: crypto.map { String(decoding: $0, as: UTF8.self) }
                ) {
                    status = "Rates not updated: \(err)"
                } else {
                    recompute()
                    status = "Rates: \(engine.ratesSource)"
                }
            } catch {
                status = "Rates not updated (offline?)"
            }
        }
    }
}

struct CalcEditor: UIViewRepresentable {
    @ObservedObject var model: CalcModel
    let theme: ReckonTheme

    func makeUIView(context: Context) -> CalcTextView {
        let view = CalcTextView()
        view.delegate = context.coordinator
        view.theme = theme
        view.text = model.text
        view.results = model.results
        view.spans = model.spans
        view.applyHighlights()
        view.onResultTap = { [weak model] in model?.copy($0) }
        return view
    }

    func updateUIView(_ view: CalcTextView, context: Context) {
        view.theme = theme
        if view.text != model.text {
            view.text = model.text
            view.spans = model.spans
            view.applyHighlights()  // setting `text` drops the attributes
        } else {
            view.spans = model.spans
        }
        view.results = model.results
    }

    func makeCoordinator() -> Coordinator { Coordinator(model: model) }

    final class Coordinator: NSObject, UITextViewDelegate {
        let model: CalcModel
        init(model: CalcModel) { self.model = model }

        func textViewDidChange(_ textView: UITextView) {
            let text = textView.text ?? ""
            MainActor.assumeIsolated { model.update(text) }
        }

        // Moves the active-line highlight.
        func textViewDidChangeSelection(_ textView: UITextView) {
            textView.setNeedsLayout()
        }

        func textViewDidBeginEditing(_ textView: UITextView) { textView.setNeedsLayout() }
        func textViewDidEndEditing(_ textView: UITextView) { textView.setNeedsLayout() }
    }
}

struct ContentView: View {
    @StateObject private var model = CalcModel()
    @StateObject private var themes = ThemeStore()
    @Environment(\.colorScheme) private var scheme
    @State private var showThemes = false

    var body: some View {
        let theme = themes.resolve(systemDark: scheme == .dark)
        VStack(spacing: 0) {
            CalcEditor(model: model, theme: theme)
            Rectangle().fill(theme.swiftUI("divider")).frame(height: 1)
            HStack(spacing: 12) {
                Text(model.status)
                    .font(.caption)
                    .foregroundStyle(theme.swiftUI("muted"))
                    .lineLimit(1)
                Spacer()
                Button {
                    showThemes = true
                } label: {
                    Label(theme.name, systemImage: "paintpalette")
                        .font(.caption.weight(.medium))
                        .foregroundStyle(theme.swiftUI("muted"))
                        .padding(.vertical, 6)
                        .padding(.leading, 12)
                        .contentShape(Rectangle())
                }
                .accessibilityLabel("Theme: \(theme.name)")
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 2)
            .background(theme.swiftUI("status_bg").ignoresSafeArea(edges: .bottom))
        }
        .background(theme.swiftUI("bg").ignoresSafeArea())
        // A fixed theme sets light or dark mode for the system bars and
        // keyboard; "Match System" leaves it to the system.
        .preferredColorScheme(themes.followSystem ? nil : (theme.dark ? .dark : .light))
        .sheet(isPresented: $showThemes) {
            ThemePicker(store: themes)
                .presentationDetents([.medium, .large])
                // No dimming at half height, so the document behind shows
                // each theme as it is tapped.
                .presentationBackgroundInteraction(.enabled(upThrough: .medium))
                .presentationDragIndicator(.visible)
        }
    }
}

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
    @Published var status: String = ""

    private let engine = ReckonEngine()
    private let docURL: URL = FileManager.default
        .urls(for: .documentDirectory, in: .userDomainMask)[0]
        .appendingPathComponent("document.reckon")

    init() {
        text = (try? String(contentsOf: docURL, encoding: .utf8)) ?? welcome
        results = engine.evaluate(text)
        status = "Rates: \(engine.ratesSource)"
        if engine.ratesStale { refreshRates() }
    }

    func update(_ newText: String) {
        text = newText
        results = engine.evaluate(newText)
        try? newText.write(to: docURL, atomically: true, encoding: .utf8)
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
                    results = engine.evaluate(text)
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

    func makeUIView(context: Context) -> CalcTextView {
        let view = CalcTextView()
        view.delegate = context.coordinator
        view.text = model.text
        view.results = model.results
        view.onResultTap = { [weak model] in model?.copy($0) }
        return view
    }

    func updateUIView(_ view: CalcTextView, context: Context) {
        if view.text != model.text { view.text = model.text }
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
    }
}

struct ContentView: View {
    @StateObject private var model = CalcModel()

    var body: some View {
        VStack(spacing: 0) {
            CalcEditor(model: model)
            Divider()
            HStack {
                Text(model.status)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                Spacer()
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 6)
        }
    }
}

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
    @Published var text: String = ""
    @Published var results: [String] = []
    @Published var spans: [[HighlightSpan]] = []
    @Published var status: String = ""
    @Published var sheets: [SheetInfo] = []
    /// File name of the open sheet.
    @Published var current: String = ""
    /// The last deleted sheet's path in the trash, for Undo.
    @Published var trashed: String?
    /// Set by newSheet: the editor focuses and selects the "Untitled" title
    /// so typing replaces it.
    @Published var selectTitle = false

    private let engine = ReckonEngine()
    let sheetsDir = SheetStore.directory()
    private var seenModified: Date?
    private var poll: Timer?

    init() {
        let legacy = FileManager.default
            .urls(for: .documentDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("document.reckon")
        let first = SheetStore.prepare(sheetsDir, legacy: legacy, welcome: welcome) ?? ""
        let saved = UserDefaults.standard.string(forKey: "currentSheet")
        current = saved.flatMap { FileManager.default.fileExists(atPath: url($0).path) ? $0 : nil } ?? first
        text = (try? String(contentsOf: url(current), encoding: .utf8)) ?? welcome
        seenModified = modified(current)
        sheets = SheetStore.list(sheetsDir)
        recompute()
        status = "Rates: \(engine.ratesSource)"
        if engine.ratesStale { refreshRates() }
        // Pick up edits that iCloud Drive brings in from other devices.
        poll = Timer.scheduledTimer(withTimeInterval: 2, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated { self?.checkForOutsideChanges() }
        }
    }

    var title: String { sheets.first { $0.file == current }?.title ?? "Sheet" }

    private func url(_ file: String) -> URL { sheetsDir.appendingPathComponent(file) }

    private func modified(_ file: String) -> Date? {
        (try? FileManager.default.attributesOfItem(atPath: url(file).path))?[.modificationDate] as? Date
    }

    func sheetText(_ file: String) -> String {
        (try? String(contentsOf: url(file), encoding: .utf8)) ?? ""
    }

    func update(_ newText: String) {
        text = newText
        recompute()
        guard !current.isEmpty else { return }
        try? newText.write(to: url(current), atomically: true, encoding: .utf8)
        seenModified = modified(current)
        if let i = sheets.firstIndex(where: { $0.file == current }) {
            let s = sheets.remove(at: i)
            let lines = newText.split(separator: "\n").map { $0.trimmingCharacters(in: .whitespaces) }.filter { !$0.isEmpty }
            let title = lines.first.map { $0.trimmingCharacters(in: CharacterSet(charactersIn: "#/ ")) } ?? "Untitled"
            sheets.insert(SheetInfo(file: s.file, title: title.isEmpty ? "Untitled" : title,
                                    preview: lines.dropFirst().first ?? "", modified: s.modified), at: 0)
        }
    }

    /// Switches sheets, first renaming the one being left to match its title.
    func open(_ file: String) {
        guard file != current else { return }
        if !current.isEmpty { _ = SheetStore.tidy(sheetsDir, current) }
        current = file
        UserDefaults.standard.set(file, forKey: "currentSheet")
        text = sheetText(file)
        seenModified = modified(file)
        sheets = SheetStore.list(sheetsDir)
        recompute()
    }

    func newSheet() {
        guard let file = SheetStore.create(sheetsDir, text: "# Untitled\n") else { return }
        sheets = SheetStore.list(sheetsDir)
        open(file)
        selectTitle = true
    }

    func delete(_ file: String) {
        let title = sheets.first { $0.file == file }?.title ?? "sheet"
        guard let path = SheetStore.trash(sheetsDir, file) else { return }
        trashed = path
        sheets = SheetStore.list(sheetsDir)
        if file == current {
            current = ""
            if let next = sheets.first { open(next.file) } else { newSheet() }
        }
        status = "Deleted “\(title)”"
    }

    func undoDelete() {
        guard let path = trashed, let file = SheetStore.restore(sheetsDir, path) else { return }
        trashed = nil
        sheets = SheetStore.list(sheetsDir)
        open(file)
        status = "Restored “\(title)”"
    }

    private func checkForOutsideChanges() {
        guard !current.isEmpty else { return }
        let now = modified(current)
        if now == nil {
            // Renamed or deleted on another device.
            sheets = SheetStore.list(sheetsDir)
            if let first = sheets.first { current = ""; open(first.file) }
            return
        }
        if now != seenModified {
            seenModified = now
            let fresh = sheetText(current)
            if fresh != text {
                text = fresh
                recompute()
                status = "Updated from another device"
            }
            sheets = SheetStore.list(sheetsDir)
        }
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
        if model.selectTitle {
            DispatchQueue.main.async {
                model.selectTitle = false
                view.becomeFirstResponder()
                let first = (view.text as NSString).range(of: "\n").location
                let end = first == NSNotFound ? (view.text as NSString).length : first
                view.selectedRange = NSRange(location: 2, length: max(0, end - 2))
            }
        }
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
    @State private var showSheets = false

    var body: some View {
        let theme = themes.resolve(systemDark: scheme == .dark)
        VStack(spacing: 0) {
            CalcEditor(model: model, theme: theme)
            Rectangle().fill(theme.swiftUI("divider")).frame(height: 1)
            HStack(spacing: 12) {
                Button {
                    showSheets = true
                } label: {
                    Label(model.title, systemImage: "doc.text")
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(theme.swiftUI("text"))
                        .lineLimit(1)
                        .padding(.vertical, 6)
                        .contentShape(Rectangle())
                }
                .frame(maxWidth: 150, alignment: .leading)
                .accessibilityLabel("Sheets. Current: \(model.title)")
                Text(model.status)
                    .font(.caption)
                    .foregroundStyle(theme.swiftUI("muted"))
                    .lineLimit(1)
                if model.trashed != nil && model.status.hasPrefix("Deleted") {
                    Button("Undo") { model.undoDelete() }
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(theme.swiftUI("text"))
                }
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
        .sheet(isPresented: $showSheets) {
            SheetsList(model: model, theme: theme)
                .presentationDetents([.medium, .large])
        }
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

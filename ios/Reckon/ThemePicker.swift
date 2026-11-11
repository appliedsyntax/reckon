import SwiftUI

/// A small preview of a theme: its background with a number-coloured dot and
/// a result-coloured bar. "Match System" shows its light and dark halves.
struct ThemeSwatch: View {
    let halves: [ReckonTheme]
    let border: Color

    var body: some View {
        HStack(spacing: 0) {
            ForEach(halves) { t in
                ZStack {
                    t.swiftUI("bg")
                    HStack(spacing: 3) {
                        Circle().fill(t.swiftUI("number")).frame(width: 6, height: 6)
                        Capsule().fill(t.swiftUI("result")).frame(width: 11, height: 5)
                    }
                }
            }
        }
        .frame(width: 44, height: 26)
        .clipShape(RoundedRectangle(cornerRadius: 6, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 6, style: .continuous).stroke(border, lineWidth: 1))
    }
}

/// The theme list. Tapping a theme applies and saves it at once; the sheet
/// opens at half height so the document behind shows the change.
struct ThemePicker: View {
    @ObservedObject var store: ThemeStore
    @Environment(\.colorScheme) private var scheme
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        let theme = store.resolve(systemDark: scheme == .dark)
        NavigationStack {
            List {
                Section {
                    row(
                        name: "Match System",
                        blurb: "\(store.light.name) by day, \(store.dark.name) by night",
                        swatch: [store.light, store.dark],
                        current: store.followSystem,
                        theme: theme
                    ) { store.matchSystem() }
                }
                section("Light", ReckonTheme.light, theme)
                section("Dark", ReckonTheme.darkThemes, theme)
            }
            .scrollContentBackground(.hidden)
            .background(theme.swiftUI("surface"))
            .navigationTitle("Theme")
            .navigationBarTitleDisplayMode(.inline)
            .toolbarBackground(theme.swiftUI("surface"), for: .navigationBar)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                        .foregroundStyle(theme.swiftUI("result"))
                }
            }
        }
        .presentationBackground(theme.swiftUI("surface"))
    }

    private func section(_ title: String, _ themes: [ReckonTheme], _ theme: ReckonTheme) -> some View {
        Section {
            ForEach(themes) { t in
                row(name: t.name, blurb: t.blurb, swatch: [t], current: store.isCurrent(t), theme: theme) {
                    store.pick(t)
                }
            }
        } header: {
            Text(title)
                .font(.caption.weight(.semibold))
                .foregroundStyle(theme.swiftUI("muted"))
        }
    }

    private func row(
        name: String,
        blurb: String,
        swatch: [ReckonTheme],
        current: Bool,
        theme: ReckonTheme,
        action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            HStack(spacing: 12) {
                ThemeSwatch(halves: swatch, border: theme.swiftUI("border"))
                VStack(alignment: .leading, spacing: 2) {
                    Text(name)
                        .font(.body.weight(.medium))
                        .foregroundStyle(theme.swiftUI("text"))
                    Text(blurb)
                        .font(.caption)
                        .foregroundStyle(theme.swiftUI("muted"))
                        .lineLimit(1)
                }
                Spacer(minLength: 8)
                if current {
                    Image(systemName: "checkmark")
                        .font(.body.weight(.bold))
                        .foregroundStyle(theme.swiftUI("result"))
                }
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .listRowBackground(current ? theme.swiftUI("highlight") : theme.swiftUI("bg"))
        .accessibilityAddTraits(current ? .isSelected : [])
    }
}

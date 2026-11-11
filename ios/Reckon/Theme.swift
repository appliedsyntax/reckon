import SwiftUI
import UIKit

/// One colour theme. The themes are defined once, in the Rust engine
/// (`reckon_core::themes`), and read here through `reckon_themes()`.
struct ReckonTheme: Identifiable, Equatable {
    let index: Int
    let name: String
    let blurb: String
    let dark: Bool
    /// "none", "grid", "ruled" or "scanlines".
    let pattern: String
    let doubleDivider: Bool
    let resultWeight: Int
    let blockCursor: Bool
    private let colors: [String: UIColor]

    var id: Int { index }

    static func == (a: ReckonTheme, b: ReckonTheme) -> Bool { a.index == b.index }

    /// A colour by its engine name: "bg", "text", "result", "number", ….
    func color(_ key: String) -> UIColor { colors[key] ?? .label }
    func swiftUI(_ key: String) -> Color { Color(uiColor: color(key)) }

    var resultFontWeight: UIFont.Weight {
        switch resultWeight {
        case 700...: return .bold
        case 600..<700: return .semibold
        case 500..<600: return .medium
        default: return .regular
        }
    }

    static let all: [ReckonTheme] = load()
    static var light: [ReckonTheme] { all.filter { !$0.dark } }
    static var darkThemes: [ReckonTheme] { all.filter { $0.dark } }

    static func named(_ name: String?) -> ReckonTheme? {
        guard let name else { return nil }
        return all.first { $0.name.caseInsensitiveCompare(name) == .orderedSame }
    }

    private static func load() -> [ReckonTheme] {
        let json = ReckonEngine.themesJSON()
        guard let data = json.data(using: .utf8),
              let items = try? JSONSerialization.jsonObject(with: data) as? [[String: Any]]
        else { return [] }
        return items.enumerated().map { i, item in
            let raw = item["colors"] as? [String: String] ?? [:]
            return ReckonTheme(
                index: i,
                name: item["name"] as? String ?? "Theme \(i + 1)",
                blurb: item["blurb"] as? String ?? "",
                dark: item["dark"] as? Bool ?? false,
                pattern: item["pattern"] as? String ?? "none",
                doubleDivider: item["double_divider"] as? Bool ?? false,
                resultWeight: item["result_weight"] as? Int ?? 600,
                blockCursor: item["block_cursor"] as? Bool ?? false,
                colors: raw.compactMapValues(UIColor.init(rgbaHex:))
            )
        }
    }
}

extension UIColor {
    /// "#rrggbbaa".
    convenience init?(rgbaHex: String) {
        let hex = rgbaHex.hasPrefix("#") ? String(rgbaHex.dropFirst()) : rgbaHex
        guard hex.count == 8, let v = UInt32(hex, radix: 16) else { return nil }
        self.init(
            red: CGFloat((v >> 24) & 0xff) / 255,
            green: CGFloat((v >> 16) & 0xff) / 255,
            blue: CGFloat((v >> 8) & 0xff) / 255,
            alpha: CGFloat(v & 0xff) / 255
        )
    }
}

/// Which theme to show, saved in UserDefaults. Same model as the desktop
/// app: follow the system with a light and a dark theme, or always use one.
@MainActor
final class ThemeStore: ObservableObject {
    @Published private(set) var followSystem: Bool
    @Published private(set) var fixed: ReckonTheme
    @Published private(set) var light: ReckonTheme
    @Published private(set) var dark: ReckonTheme

    private let defaults = UserDefaults.standard

    init() {
        let all = ReckonTheme.all
        let paper = ReckonTheme.named("Paper") ?? all[0]
        let graphite = ReckonTheme.named("Graphite") ?? all[all.count - 1]
        followSystem = defaults.string(forKey: "themeMode") != "fixed"
        light = ReckonTheme.named(defaults.string(forKey: "lightTheme")) ?? paper
        dark = ReckonTheme.named(defaults.string(forKey: "darkTheme")) ?? graphite
        fixed = ReckonTheme.named(defaults.string(forKey: "theme")) ?? graphite
    }

    func resolve(systemDark: Bool) -> ReckonTheme {
        followSystem ? (systemDark ? dark : light) : fixed
    }

    /// Always use `theme`; it also becomes the light or dark theme used when
    /// following the system again.
    func pick(_ theme: ReckonTheme) {
        followSystem = false
        fixed = theme
        if theme.dark { dark = theme } else { light = theme }
        save()
    }

    func matchSystem() {
        followSystem = true
        save()
    }

    func isCurrent(_ theme: ReckonTheme) -> Bool { !followSystem && fixed == theme }

    private func save() {
        defaults.set(followSystem ? "system" : "fixed", forKey: "themeMode")
        defaults.set(fixed.name, forKey: "theme")
        defaults.set(light.name, forKey: "lightTheme")
        defaults.set(dark.name, forKey: "darkTheme")
    }
}

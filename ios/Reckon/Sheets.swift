import SwiftUI

/// One sheet in the folder (see `reckon_sheets_list` in reckon.h).
struct SheetInfo: Identifiable, Equatable {
    let file: String
    let title: String
    let preview: String
    let modified: Int
    var id: String { file }
}

/// Sheet storage through the engine, so every app names, lists and trashes
/// sheets the same way.
enum SheetStore {
    /// Where sheets live: the app's iCloud Drive folder when the app has the
    /// iCloud Documents capability and the user is signed in, otherwise
    /// Documents/Sheets (visible in the Files app under On My iPhone).
    static func directory() -> URL {
        let fm = FileManager.default
        if fm.ubiquityIdentityToken != nil,
           let cloud = fm.url(forUbiquityContainerIdentifier: nil) {
            return cloud.appendingPathComponent("Documents", isDirectory: true)
        }
        return fm.urls(for: .documentDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("Sheets", isDirectory: true)
    }

    static func list(_ dir: URL) -> [SheetInfo] {
        let json = take(reckon_sheets_list(dir.path))
        guard let data = json.data(using: .utf8),
              let items = try? JSONSerialization.jsonObject(with: data) as? [[String: Any]]
        else { return [] }
        return items.map {
            SheetInfo(
                file: $0["file"] as? String ?? "",
                title: $0["title"] as? String ?? "Untitled",
                preview: $0["preview"] as? String ?? "",
                modified: $0["modified"] as? Int ?? 0
            )
        }
    }

    static func prepare(_ dir: URL, legacy: URL, welcome: String) -> String? {
        optional(reckon_sheets_prepare(dir.path, legacy.path, welcome))
    }

    static func create(_ dir: URL, text: String) -> String? { optional(reckon_sheet_create(dir.path, text)) }
    static func tidy(_ dir: URL, _ file: String) -> String? { optional(reckon_sheet_tidy(dir.path, file)) }
    static func trash(_ dir: URL, _ file: String) -> String? { optional(reckon_sheet_trash(dir.path, file)) }
    static func restore(_ dir: URL, _ trashed: String) -> String? { optional(reckon_sheet_restore(dir.path, trashed)) }

    private static func optional(_ p: UnsafeMutablePointer<CChar>?) -> String? {
        guard let p else { return nil }
        defer { reckon_string_free(p) }
        return String(cString: p)
    }

    private static func take(_ p: UnsafeMutablePointer<CChar>?) -> String { optional(p) ?? "" }
}

/// The list of sheets: search, open, create and delete (swipe).
struct SheetsList: View {
    @ObservedObject var model: CalcModel
    let theme: ReckonTheme
    @Environment(\.dismiss) private var dismiss
    @State private var query = ""

    private var shown: [SheetInfo] {
        let q = query.trimmingCharacters(in: .whitespaces).lowercased()
        guard !q.isEmpty else { return model.sheets }
        return model.sheets.filter {
            $0.title.lowercased().contains(q) || model.sheetText($0.file).lowercased().contains(q)
        }
    }

    var body: some View {
        NavigationStack {
            List {
                ForEach(shown) { sheet in
                    Button {
                        model.open(sheet.file)
                        dismiss()
                    } label: {
                        HStack(spacing: 12) {
                            VStack(alignment: .leading, spacing: 2) {
                                Text(sheet.title)
                                    .font(.body.weight(sheet.file == model.current ? .semibold : .regular))
                                    .foregroundStyle(theme.swiftUI("text"))
                                if !sheet.preview.isEmpty {
                                    Text(sheet.preview)
                                        .font(.caption.monospaced())
                                        .foregroundStyle(theme.swiftUI("muted"))
                                }
                            }
                            .lineLimit(1)
                            Spacer(minLength: 8)
                            if sheet.file == model.current {
                                Image(systemName: "checkmark")
                                    .font(.body.weight(.bold))
                                    .foregroundStyle(theme.swiftUI("result"))
                            }
                        }
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .listRowBackground(sheet.file == model.current ? theme.swiftUI("highlight") : theme.swiftUI("bg"))
                    .swipeActions {
                        Button(role: .destructive) { model.delete(sheet.file) } label: {
                            Label("Delete", systemImage: "trash")
                        }
                    }
                }
            }
            .overlay {
                if shown.isEmpty {
                    Text(query.isEmpty ? "No sheets" : "No matching sheets")
                        .foregroundStyle(theme.swiftUI("muted"))
                }
            }
            .searchable(text: $query, prompt: "Search sheets")
            .scrollContentBackground(.hidden)
            .background(theme.swiftUI("surface"))
            .navigationTitle("Sheets")
            .navigationBarTitleDisplayMode(.inline)
            .toolbarBackground(theme.swiftUI("surface"), for: .navigationBar)
            .toolbar {
                ToolbarItem(placement: .topBarLeading) {
                    Button {
                        model.newSheet()
                        dismiss()
                    } label: {
                        Label("New Sheet", systemImage: "square.and.pencil")
                    }
                    .foregroundStyle(theme.swiftUI("result"))
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                        .foregroundStyle(theme.swiftUI("result"))
                }
            }
        }
        .tint(theme.swiftUI("result"))
        .presentationBackground(theme.swiftUI("surface"))
    }
}

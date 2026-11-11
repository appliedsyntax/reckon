import Foundation

/// Swift wrapper around the Rust engine's C interface (see reckon.h).
final class ReckonEngine {
    private let handle: OpaquePointer

    init() {
        let dir = FileManager.default
            .urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("Reckon", isDirectory: true)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        handle = reckon_engine_new(dir.path)
        let tz = TimeZone.current
        reckon_set_local_zone(handle, tz.identifier, Int32(tz.secondsFromGMT()))
    }

    deinit {
        reckon_engine_free(handle)
    }

    /// One result per input line ("" when a line has no result).
    func evaluate(_ document: String) -> [String] {
        take(reckon_evaluate(handle, document)).components(separatedBy: "\n")
    }

    /// Results and highlights for each line, from one evaluation.
    func evaluateHighlighted(_ document: String) -> [(result: String, spans: [HighlightSpan])] {
        take(reckon_evaluate_highlighted(handle, document))
            .components(separatedBy: "\n")
            .map { line in
                let parts = line.split(separator: "\t", maxSplits: 1, omittingEmptySubsequences: false)
                let spans = (parts.count > 1 ? parts[1] : "").split(separator: ";").compactMap { s -> HighlightSpan? in
                    let f = s.split(separator: ",")
                    guard f.count == 3, let start = Int(f[0]), let len = Int(f[1]) else { return nil }
                    return HighlightSpan(start: start, length: len, kind: String(f[2]))
                }
                return (parts.first.map(String.init) ?? "", spans)
            }
    }

    /// The colour themes as JSON (see `reckon_themes` in reckon.h).
    static func themesJSON() -> String {
        guard let p = reckon_themes() else { return "[]" }
        defer { reckon_string_free(p) }
        return String(cString: p)
    }

    var ratesStale: Bool { reckon_rates_stale(handle) != 0 }
    var ratesSource: String { take(reckon_rates_source(handle)) }
    var fiatURL: URL? { URL(string: take(reckon_fiat_url())) }
    var cryptoURL: URL? { URL(string: take(reckon_crypto_url())) }

    /// Applies downloaded rate JSON. Returns an error message on failure.
    func applyRates(fiat: String, crypto: String?) -> String? {
        let err: UnsafeMutablePointer<CChar>?
        if let crypto {
            err = crypto.withCString { reckon_apply_rates(handle, fiat, $0) }
        } else {
            err = reckon_apply_rates(handle, fiat, nil)
        }
        return err.map { take($0) }
    }

    /// Copies and frees a string returned by the engine.
    private func take(_ p: UnsafeMutablePointer<CChar>?) -> String {
        guard let p else { return "" }
        defer { reckon_string_free(p) }
        return String(cString: p)
    }
}

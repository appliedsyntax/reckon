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

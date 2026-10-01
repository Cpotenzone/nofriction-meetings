// Shared helpers for the C ABI.
//
// Async results come back through a C callback `(ctx, json)`. `json` is a
// UTF-8 JSON object that is only valid during the call; Rust copies it.
// Every result has `"ok": true|false`; failures carry `"error"`.

import Foundation

public typealias NFCallback = @convention(c) (UnsafeMutableRawPointer?, UnsafePointer<CChar>?) -> Void

/// Carries the Rust context pointer across `Task` boundaries.
struct NFContext: @unchecked Sendable {
    let ptr: UnsafeMutableRawPointer?
    let cb: NFCallback

    func send(_ object: [String: Any]) {
        let json = nfJSON(object)
        json.withCString { cb(ptr, $0) }
    }

    func fail(_ error: Error) {
        send(["ok": false, "error": "\(error)"])
    }
}

func nfJSON(_ object: [String: Any]) -> String {
    guard JSONSerialization.isValidJSONObject(object),
          let data = try? JSONSerialization.data(withJSONObject: object, options: []),
          let s = String(data: data, encoding: .utf8)
    else {
        return "{\"ok\":false,\"error\":\"could not encode result\"}"
    }
    return s
}

/// Synchronous results are returned as a malloc'd C string that Rust frees
/// with `nf_free`.
func nfCString(_ object: [String: Any]) -> UnsafeMutablePointer<CChar>? {
    strdup(nfJSON(object))
}

@_cdecl("nf_free")
public func nf_free(_ p: UnsafeMutablePointer<CChar>?) {
    free(p)
}

let nfISO: ISO8601DateFormatter = {
    let f = ISO8601DateFormatter()
    f.formatOptions = [.withInternetDateTime]
    return f
}()

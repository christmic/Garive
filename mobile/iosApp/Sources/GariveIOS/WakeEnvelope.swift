import Foundation

enum WakeEnvelope {
    static func routeToken(from userInfo: [AnyHashable: Any]) -> String? {
        guard let envelope = userInfo["garive"] as? [String: Any],
              Set(envelope.keys) == Set(["schema_version", "route_token", "category", "collapse_key"]),
              (envelope["schema_version"] as? NSNumber)?.intValue == 1,
              let token = envelope["route_token"] as? String, token.count == 43,
              token.allSatisfy({ $0.isASCII && ($0.isLetter || $0.isNumber || $0 == "-" || $0 == "_") }),
              let category = envelope["category"] as? String,
              ["attention", "completed", "failed", "connection_security"].contains(category),
              envelope["collapse_key"] as? String == category else { return nil }
        return token
    }
}

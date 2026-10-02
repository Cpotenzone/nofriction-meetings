import Foundation

enum AppLinks {
    /// PLACEHOLDER: publish the privacy policy (see docs/APP_STORE_RELEASE.md §5.3 step 9)
    /// and make sure this URL resolves before submitting. Also enter it in App Store Connect.
    static let privacyPolicy = URL(string: "https://nofriction.ai/privacy")!
    /// Apple's standard EULA (guideline 3.1.2).
    static let terms = URL(string: "https://www.apple.com/legal/internet-services/itunes/dev/stdeula/")!
    static let supportEmail = "support@nofriction.ai"
    static let supportMail = URL(string: "mailto:support@nofriction.ai?subject=noFriction%20support")!

    static var versionString: String {
        let info = Bundle.main.infoDictionary
        let v = info?["CFBundleShortVersionString"] as? String ?? "?"
        let b = info?["CFBundleVersion"] as? String ?? "?"
        return "\(v) (\(b))"
    }
}

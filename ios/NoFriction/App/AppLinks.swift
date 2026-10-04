import Foundation

enum AppLinks {
    /// The owner website policy still needs the meetings-app supplement before submission.
    static let privacyPolicy = URL(string: "https://nofriction.io/privacy")!
    /// Apple's standard EULA (guideline 3.1.2).
    static let terms = URL(string: "https://www.apple.com/legal/internet-services/itunes/dev/stdeula/")!
    static let supportEmail = "casey@nofriction.io"
    static let supportMail = URL(string: "mailto:casey@nofriction.io?subject=noFriction%20support")!

    static var versionString: String {
        let info = Bundle.main.infoDictionary
        let v = info?["CFBundleShortVersionString"] as? String ?? "?"
        let b = info?["CFBundleVersion"] as? String ?? "?"
        return "\(v) (\(b))"
    }
}

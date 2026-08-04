import SwiftUI

@main
struct FeatherKeyHostApp: App {
    var body: some Scene {
        WindowGroup {
            VStack(spacing: 16) {
                Text("FeatherKey (iOS foundation slice)").font(.headline)
                Text("Enable in Settings → General → Keyboard → Keyboards → Add New Keyboard → FeatherKey, then type below.")
                    .font(.footnote).multilineTextAlignment(.center).padding(.horizontal)
                TextField("Type here to test", text: .constant("")).textFieldStyle(.roundedBorder).padding()
                Spacer()
            }.padding()
        }
    }
}

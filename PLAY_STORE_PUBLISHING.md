# FeatherKey — Google Play Publishing Guide

Reference for publishing the FeatherKey Android keyboard to the Google Play Console.
Facts below are pulled from the project (`apps/android/app/build.gradle.kts`,
`AndroidManifest.xml`, `strings.xml`) as of version `0.1.0`.

> Note: this repo is a monorepo — the Android app lives at `apps/android/` and the Rust core at `core/`.

## 1. App facts (enter these in Play Console)

| Field | Value | Notes |
|---|---|---|
| App name | **FeatherKey** | from `strings.xml` |
| Package name (`applicationId`) | **`com.featherkey`** | ⚠️ **permanent once published — cannot be changed** |
| Version code | `1` | must increase on every upload |
| Version name | `0.1.0` | user-visible; consider `1.0.0` for first public release |
| Min SDK | 26 (Android 8.0) | |
| Target SDK | 35 (Android 15) | meets Play's target-API requirement |
| ABIs shipped | `arm64-v8a`, `armeabi-v7a` | arm64 satisfies the 64-bit requirement |
| Suggested category | **Tools** (keyboard / input method) | |
| Permissions | `RECORD_AUDIO` | mic / voice-typing key (system `SpeechRecognizer`) |
| Backup | `allowBackup=false` | appropriate for a keyboard |
| Launcher activity | `SettingsActivity` (settings + first-run consent) | |

## 2. Blockers to clear before uploading

### a) Create a release signing (upload) key — none exists yet
The `release` build type currently has **no `signingConfig`**, so `bundleRelease`
produces an *unsigned* bundle. Create an upload key (run it yourself so you own the
password):

```bash
keytool -genkeypair -v -keystore ~/featherkey-upload.jks \
  -alias featherkey -keyalg RSA -keysize 2048 -validity 10000
```

- **Back up this `.jks` and its password securely.** Losing it blocks all future
  updates (unless enrolled in Play App Signing, recommended — let Google hold the
  real signing key while you sign uploads with this upload key).

### b) Upload an App Bundle (`.aab`), not an APK
Required for new apps.

### c) Verify the minified release build runs on-device
`release` has R8 (`isMinifyEnabled = true`). The keyboard loads native code via
JNA/UniFFI reflection, which R8 can strip — debug working does **not** guarantee
release works. Install and smoke-test the release variant before trusting it.

### d) Native `.so` libraries are git-ignored
They exist locally (built via cargo-ndk) and a *local* release build bundles them.
A CI machine without cargo-ndk would ship a broken bundle — ensure the native build
runs in any automated pipeline.

## 3. Build the signed bundle

After the signing config is wired into `app/build.gradle.kts` (reading credentials
from a git-ignored `keystore.properties`):

```bash
cd apps/android
./gradlew --no-daemon -Pkotlin.compiler.execution.strategy=in-process bundleRelease
# output: apps/android/app/build/outputs/bundle/release/app-release.aab
```

(The `--no-daemon -Pkotlin.compiler.execution.strategy=in-process` flags are only
needed in restricted sandboxes; a normal dev machine can run `./gradlew bundleRelease`.)

### Signing config snippet (to add under `android { }`)

```kotlin
signingConfigs {
    create("release") {
        val props = java.util.Properties().apply {
            val f = rootProject.file("keystore.properties")
            if (f.exists()) f.inputStream().use { load(it) }
        }
        storeFile = props.getProperty("storeFile")?.let { file(it) }
        storePassword = props.getProperty("storePassword")
        keyAlias = props.getProperty("keyAlias")
        keyPassword = props.getProperty("keyPassword")
    }
}
buildTypes {
    release {
        signingConfig = signingConfigs.getByName("release")
        // …existing minify/proguard…
    }
}
```

`keystore.properties` (git-ignored):
```
storeFile=/absolute/path/to/featherkey-upload.jks
storePassword=…
keyAlias=featherkey
keyPassword=…
```

## 4. Store-listing assets to prepare (not in the repo)

- **Privacy policy URL** — **required** (mic permission + a keyboard that sees typed
  text). Must be a public URL. Key message: FeatherKey learns **on-device** and does
  not upload typed text.
- **Data safety form** — required. For an on-device keyboard this is essentially
  "**no data collected, no data shared**"; declare the mic/voice-typing path
  accurately.
- **Graphics:** app icon **512×512**, feature graphic **1024×500**, at least **2**
  phone screenshots.
- **Descriptions:** short (≤80 chars) + full (≤4000 chars). Ready-to-paste copy in
  [§8 Store descriptions](#8-store-descriptions-ready-to-paste).
- **Content rating** questionnaire, target-audience declaration, ads declaration
  (no ads).

## 5. Keyboard-specific policy note

Google reviews input-method apps under the User Data policy: a keyboard that
transmits keystrokes off-device needs prominent disclosure. FeatherKey processes
input on-device, so it complies — but the privacy policy and Data Safety form must
state this truthfully.

## 6. Permanent decisions (lock these in before first publish)

- **`applicationId = com.featherkey`** — cannot change after publish. Confirmed OK.
  Must be globally unique on Play; verify it isn't already taken.
- **Upload/signing key** — keep the keystore + password backed up forever.

## 7. First-release checklist

- [ ] Create + back up the upload keystore
- [ ] Wire the release `signingConfig`
- [ ] (Optional) bump `versionName` to `1.0.0`
- [ ] Build `app-release.aab`
- [ ] Install + smoke-test the **release** (minified) variant on a device
- [ ] Write the privacy policy and host it at a public URL
- [ ] Prepare graphics + descriptions
- [ ] Complete Data Safety + content-rating forms
- [ ] Create the app in Play Console, upload the `.aab` to internal testing first

## 8. Store descriptions (ready to paste)

Copy aligned with the app's actual positioning (`strings.xml`, `README.md`, and the
`apps/web` landing page). Keep the privacy claims truthful and consistent with the
privacy policy and Data Safety form (§4, §5): FeatherKey learns **on-device**, in
encrypted storage, and uploads nothing.

### Short description (Play limit: 80 characters)

```
A fast, private keyboard that learns how you write — all on-device.
```

(66 characters. Alternative if you want the promise more explicit:
`Private keyboard that learns how you write — on-device, nothing uploaded.` — 72.)

### Full description (Play limit: 4000 characters)

```
FeatherKey is a fast, private keyboard that learns how you write — and keeps everything on your phone.

Most keyboards get smarter by sending what you type to the cloud. FeatherKey does the opposite. It learns your words, your habits and your next-word patterns entirely on-device, in encrypted storage. Nothing is uploaded, shared or synced. Ever. Passwords and payment fields are always skipped.

WHY FEATHERKEY

• Truly private — learning happens on your phone, in encrypted storage. Your typing never leaves the device.
• Fast — a lightweight native engine keeps typing responsive, with no network round-trips.
• It adapts to you — key targets and autocorrect thresholds adjust to your accuracy over time, so it gets more accurate the more you type.
• You're in control — turn learning on or off whenever you like, and clear everything the keyboard has learned with one tap.

FEATURES

• Adaptive tap & autocorrect — key targets and correction thresholds tune themselves to how you actually type.
• Swipe to type — glide across the keys and pick from a strip of suggestions as you go.
• Several languages at once — blend multiple languages without switching mid-sentence.
• Accents & long-press variants — accented characters ordered to match your languages.
• Voice & emoji — hand off to dictation and reach an emoji layer, one tap from the symbol row.
• Next-word suggestions — learned from your own writing, on your own device.

PRIVACY BY DESIGN

FeatherKey processes everything you type on-device. There are no accounts, no tracking, and no keystroke upload. The optional learning that personalises your suggestions is stored encrypted on your phone and can be cleared at any time from Settings. Passwords and payment fields are never learned from.

Type freely. Your words stay yours.
```

(~1,760 characters — well under the 4000 limit, leaving room to expand the language
list or feature bullets as those ship.)

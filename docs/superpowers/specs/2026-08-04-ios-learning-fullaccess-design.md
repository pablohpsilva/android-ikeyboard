# iOS On-Device Learning + Persistence — the Full-Access Decision — Design

**Status:** design phase (gated by `/r-u-sure`).
**Slug:** `ios-learning-fullaccess`
**Date:** 2026-08-04
**Depends on:** `2026-08-04-ios-foundation-slice-design.md` (the extension exists and
types end-to-end through the shared core). This is deferred item **§8.4** of that
slice ("On-device learning + persistence") and wave **7** of `ios-parity-design.md`.

**Goal (one sentence):** Decide how FeatherKey's iOS keyboard extension enables
on-device **learning + persistence** — reusing the core's `learn_word` / `persist`
/ `import_*` and the BR-26 `SensitiveField` gate — under the hard iOS constraint
that a keyboard extension **without "Allow Full Access"** can use neither the
Keychain (reliably) nor a host-shared **App Group** container.

This is a **decision + architecture** design. No code.

---

## 1. Problem & the hard constraint

The foundation slice proved the extension types through the core. It already had to
satisfy the core's sole constructor:

```rust
KeyboardCore::open(db_path: String, device_key: Vec<u8> /*32*/, languages)
```

— which **always** opens an encrypted `RedbSecureStore`; there is no decode-only
path. So a 32-byte device key and an encrypted DB path already exist on iOS.

The foundation slice's design (§5.2) *planned* to put the device key in the iOS
**Keychain**, mirroring Android's Keystore-backed key (BR-62). **That did not hold
in practice.** The lived constraint, which this design takes as ground truth:

> A keyboard extension **without "Allow Full Access"** cannot reliably use the
> Keychain, and **cannot** read or write a **host-shared App Group container** or a
> **shared keychain-access-group**. We hit exactly this: the device key had to move
> out of the Keychain into a **file inside the extension's own container**.

Three iOS platform facts follow, and they shape every option below:

1. **App Group container access requires Full Access.** Without it, the host app and
   the extension are two disjoint sandboxes. Host-app *settings* (a consent toggle,
   a "clear learned data" button) cannot reach the extension, and learned data in
   the extension cannot be surfaced or managed by the host.
2. **Keychain is unreliable without Full Access** — including any shared
   keychain-access-group. The dependable at-rest primitive the extension always has
   is **iOS Data Protection on files in its own container** (the file-protection
   class keys are wrapped by the Secure Enclave's device UID key, so a
   Data-Protection file is still device-bound — just not a Keychain item and not
   shareable with the host).
3. **Full Access is a heavyweight, scary permission.** Enabling it surfaces the
   system warning that the keyboard "can access all the data you type, including
   passwords." For a product whose entire thesis is privacy (BR-20/21/23,
   "minimum permissions, clearly justified" BR-27), **requiring** Full Access to get
   basic learning is a direct contradiction of the positioning.

### The good news: BR-26 is *not* on the Full-Access axis

The BR-26 sensitive-field signal is **local to the extension** and needs neither
Full Access nor an App Group (see §4.3). And iOS gives BR-26 a *partial structural
guarantee* the Android side never had: **for secure text fields the OS swaps in the
system keyboard and our extension is never instantiated at all** — a password box
literally cannot reach our learner. So the Full-Access decision is about **consent
plumbing (BR-22) and the device key + data location (BR-62/63)** — not about BR-26.

---

## 2. Requirements in scope

| BR | Statement (abridged) | How iOS touches it |
|---|---|---|
| **BR-22** | Plain-language consent visibility; withdrawable at any time. | *The* problem: without an App Group the host toggle can't reach the extension. |
| **BR-26** | Sensitive contexts (passwords) never learned/stored/predicted. | Solved locally per §4.3 in **all** options; not gated by Full Access. |
| **BR-62** | All personal data at rest encrypted on device. | Device key + encrypted store; how the key is secured differs per option. |
| **BR-63** | Learned data excluded from cloud backup by default. | `isExcludedFromBackup` on the store + key files (both options that persist). |
| **BR-27** | Minimum permissions, each justified. | The lens for the recommendation: is Full Access *minimum*? |
| **BR-70** | iOS reuses the shared Rust core; no logic reimplemented in Swift. | Learning uses `learn_word`/`persist`/`import_*` verbatim; Swift only plumbs. |

No new BR is introduced; this design closes the iOS side of existing ones.

---

## 3. Existing code consulted (CLAUDE.md §2)

Queried `CODEMAP.md` and the Android shell before designing. This reuses; it does
**not** duplicate typing/learning/gating logic.

| Existing | Role here |
|---|---|
| `KeyboardCore::learn_word / observe_strip_pick / observe_delete_retype / observe_tap / observe_proper_noun` (`core/crates/featherkey-core/src/ffi.rs`) | The learning surface iOS calls. Each already takes a `SensitiveField` and is **core-gated** on sensitivity — unchanged. |
| `KeyboardCore::persist` | Flush the encrypted model to disk. iOS calls it debounced, off the input path (mirrors Android). |
| `KeyboardCore::import_context / import_frequencies` | One-time legacy migration hooks. **No iOS legacy exists**, so iOS does not call them — recorded so we don't build a migrator with nothing to migrate. |
| `crate sensitive-context` — `SensitivityPolicy::should_suppress` (`core/crates/sensitive-context/src/lib.rs`) | The BR-26 gate. Pure predicate over `SensitiveContextSource`; the composition root already consults it **before** any learn/predict (the E-2 ordering invariant). iOS supplies the source, nothing more. |
| `SensitiveField` UniFFI callback interface (`is_sensitive() -> bool`) | Implemented **on the platform side**. Android implements it from `EditorInfo`; iOS implements it from `UITextDocumentProxy` traits (§4.3). |
| **Android reference (behavioural parity, re-expressed in Swift):** | |
| `FeatherKeyImeService.kt` — `learningEnabled` + `field.isSensitive()` double-gate on `learnWord`, `schedulePersist`, `observeGate()` | The exact call-site discipline iOS re-expresses. |
| `ConsentStore.kt` — DataStore `learning_enabled` (default **true**), withdrawable | The BR-22 model iOS must reproduce; the App-Group gap is what makes it hard. |
| `EditorInfoSensitivity.kt` — password variations + `IME_FLAG_NO_PERSONALIZED_LEARNING` | The Android sensitivity source; §4.3 maps each signal to its iOS analogue (and notes what has **no** analogue). |
| `KeystoreKeyProvider.provisionDataKey()` — 32 bytes, Keystore-backed (BR-62) | The key-provisioning contract iOS must meet by a different mechanism. |

No new core code and **no core change**. Learning on iOS is a **wiring** task over the
frozen FFI surface — consistent with BR-70 and the CLAUDE.md §5 smell test (learning
logic in Swift would be the smell; there is none here).

---

## 4. The invariants that hold in every option

Before the options diverge, three things are fixed — they are *not* choices.

### 4.1 Learning always goes through the core, double-gated

Every learn/observe call is guarded, exactly as Android does it, by **two** gates in
this order:

```
if !consentEnabled { return }                 // BR-22 — platform gate (cheap, avoids FFI)
core.learn_word(preceding, word, sensitiveField)   // BR-26 — core re-gates internally
```

The core re-checks sensitivity itself (`FieldSource` → `SensitivityPolicy`), so the
platform gate is defence-in-depth, never the sole guard. What differs per option is
**only** where `consentEnabled` is read from and where the encrypted store + key live.

### 4.2 Persistence is debounced and off the input path

`persist()` is scheduled after a quiet interval (mirroring Android's
`schedulePersist`), never called per keystroke. The store file and key file both get
`isExcludedFromBackup = true` (BR-63) and file protection
`.completeUntilFirstUserAuthentication` (readable when the keyboard runs after first
unlock; BR-62). This is identical across options; only the *directory* changes.

### 4.3 BR-26 is solved locally — identically in A, B, and C

The iOS `SensitiveField` is constructed **once per field** (E-2) from
`textDocumentProxy`, which conforms to `UITextInputTraits`:

| Android signal (`EditorInfoSensitivity`) | iOS analogue | Verdict |
|---|---|---|
| password text/number variations | Secure fields never reach us — **OS shows the system keyboard**, the extension is not instantiated. Defensive `isSecureTextEntry` check if ever surfaced. | Structural + belt-and-braces |
| `TYPE_TEXT_VARIATION_WEB_PASSWORD` etc. | same OS swap | Structural |
| `keyboardType` classes (email/number/URL) | `textDocumentProxy.keyboardType` (`.emailAddress`, `.numberPad`, `.URL`, `.phonePad`) | Available; treat as sensitive-leaning, conservative default |
| `IME_FLAG_NO_PERSONALIZED_LEARNING` | **No iOS equivalent** — recorded as a gap. Weak proxy: `autocorrectionType == .no`. | Degrade conservative: when uncertain, suppress |

Because this reads only from the proxy the extension already holds, **BR-26 needs no
Full Access, no App Group, no host**. This is why the sensitive gate is the same in
every option and drops out of the decision. The one honest caveat — iOS has no
faithful `NO_PERSONALIZED_LEARNING` signal — is a BR-26 *fidelity* note, not a
Full-Access tradeoff, and it is identical whichever option we pick.

---

## 5. The three options

### Option A — Request Full Access (Keychain + App-Group settings sync)

Declare `RequestsOpenAccess = YES`; the user must toggle **Allow Full Access** on.

| Aspect | With Option A |
|---|---|
| **Consent (BR-22)** | **Best.** The host app owns a real consent screen (plain-language, withdrawable) writing `learning_enabled` to an **App-Group `UserDefaults`**; the extension reads it live — a faithful port of `ConsentStore`. One source of truth, host-managed, exactly the Android model. Host can also offer "clear all learned data." |
| **Sensitive gate (BR-26)** | Same local mechanism as §4.3 (unchanged by Full Access). |
| **Device key (BR-62)** | The real parity: 32 bytes in the **Keychain** under a **shared keychain-access-group**, `kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly`, `ThisDeviceOnly` (BR-63). Host and extension read the *same* key → a **single shared encrypted store** in the App-Group container, so learning done in the host's test field and in the keyboard is one model. |
| **Data location** | App-Group container — shared, host-manageable, backup-excluded. |
| **Cost** | The system "can access everything you type, including passwords" warning. **Directly contradicts** BR-27 (minimum permissions) and the privacy positioning. Network capability becomes *possible* (we won't use it, but users can't tell that from the toggle). A hard sell for a privacy keyboard; measurably depresses adoption of the very learning feature it enables. |

**Verdict:** technically the cleanest; strategically the most expensive. Full Access
buys host-sync and Keychain, but the price is the exact permission our brand tells
users to distrust. Not the default.

---

### Option B — App-Group + keychain-access-group *entitlements*, extension-local *behaviour* (no Full Access) — **RECOMMENDED**

Declare the App-Group **and** shared keychain-access-group entitlements on both
targets — but **do not require Full Access**, and architect the extension to run
entirely on **its own container** until Full Access is (optionally) granted.

The key realisation: **the entitlements are free to declare; the shared container
they point at is simply inaccessible from the extension while Full Access is off.**
So we build for graceful degradation, and the *same binary* transparently upgrades to
Option A's host-sync the moment a user flips Full Access on. We never *ask* them to.

| Aspect | Without Full Access (default state) | If the user later grants Full Access |
|---|---|---|
| **Consent (BR-22)** | The host toggle **cannot** reach the extension. So consent is expressed **inside the keyboard** — a small in-keyboard settings affordance ("Learn from what I type" on/off, plus a "Learn on this keyboard" plain-language line and a link to the host for the full explanation), persisted to the **extension's own `UserDefaults`/file**. Default **true**, matching Android. The host screen still explains BR-22 in plain language and links out, but the *authoritative* toggle for the extension lives in the extension. Honest limitation: two surfaces, not one. | The extension detects App-Group availability and **switches to reading the host's App-Group toggle** — collapsing to Option A's single source of truth. |
| **Sensitive gate (BR-26)** | §4.3, local. Unchanged. | Unchanged. |
| **Device key (BR-62)** | 32 random bytes in a **file in the extension's own container**, iOS **Data Protection** `.completeUntilFirstUserAuthentication`, `isExcludedFromBackup`. Device-bound via the Secure-Enclave-wrapped file-protection key — **not** a Keychain item, **not** hardware-attested like Android Keystore, but strong enough for a symmetric at-rest key that never leaves the device. This is exactly the fallback the foundation slice already landed on. | The provisioner **prefers the shared keychain-access-group** Keychain key when reachable; a one-time migration re-keys the store from the file key to the Keychain key so host + extension converge on one store. |
| **Data location** | **Extension container only.** Learned model is real and persistent, but **siloed** — the host app cannot read, display, export, or clear it. A "clear learned data" action must therefore also live in the keyboard (or be a full re-install). | App-Group container; host can manage/clear. |
| **Cost** | No scary permission (BR-27 satisfied — we request the *minimum*, and Full Access is genuinely optional). The price is the split consent surface and a host app that can't manage the extension's data. | Zero additional prompt beyond the Full-Access the user chose. |

**What B can and cannot do without Full Access — stated plainly:**

- **Can:** learn, persist, encrypt-at-rest, gate on consent, gate on sensitivity,
  exclude from backup, survive relaunch, and deliver the full learning value — all
  within the extension.
- **Cannot:** share one model/key/toggle with the host app; let the host display or
  clear learned data; or use the Keychain. The App-Group/keychain-access-group
  entitlements are **declared but dormant**, lighting up only under Full Access.

**Verdict:** delivers on-device learning **today** without the permission that
undermines the brand, and makes Option A a *user-elected upgrade* rather than a
gate. The cost (split consent + host can't manage data) is real but bounded, and it
is the honest price of iOS's sandbox — not a design shortcut.

---

### Option C — Keep learning OFF on iOS for now

Ship the iOS keyboard as decode-only (foundation + UI slices), never call
`learn_word`/`observe_*`/`persist`-of-personal-data.

| Aspect | With Option C |
|---|---|
| **Consent (BR-22)** | Trivially satisfied — nothing is collected, so there is nothing to consent to. Host shows "on-device learning is coming to iOS." |
| **Sensitive gate (BR-26)** | Moot for learning (nothing learned); the gate still governs prediction suppression, which is local (§4.3). |
| **Device key (BR-62)** | The core still *requires* a 32-byte key for `open()`, so the **foundation slice's extension-container key stays** — but it protects only the (essentially empty) store. |
| **Data location** | Extension container, near-empty. |
| **Cost** | iOS is visibly less smart than Android — no personalization, no learned frequencies, no next-word model improvement. Feature regression versus the shipped Android product; the neural roadmap (apps #1–4) has no personal signal to train on. |

**Verdict:** the safe non-answer. Correct only as a **fallback** if B's in-keyboard
consent surface or BR-26 fidelity proves unacceptable in review — not as the
destination.

---

## 6. Recommendation

**Adopt Option B.** Ship on-device learning on iOS using the **extension's own
Data-Protection-backed container** and an **in-keyboard consent toggle**, while
**declaring** (but not requiring) the App-Group + shared-keychain-access-group
entitlements so that a user who *chooses* Full Access transparently upgrades to
Option A's host-synced single store — with no code fork and no second prompt.

Why B over A: Full Access is precisely the permission FeatherKey's privacy thesis
tells users to distrust (BR-20/21/23/27). Making it a **requirement** for basic
learning would trade the brand for host-side convenience. B gives the full learning
value at the *minimum* permission, and keeps A on the table as an opt-in.

Why B over C: C concedes the personalization that is FeatherKey's whole reason to
exist over the stock keyboard, and starves the neural roadmap of on-device signal.
B delivers learning now; the only concessions are a split consent surface and a host
that can't manage the extension's data — both bounded, both honest consequences of
the iOS sandbox, both erased the moment Full Access is granted.

**Guardrails carried into the plan phase:**

- BR-26 gate identical to §4.3 in all states; record the missing
  `NO_PERSONALIZED_LEARNING` analogue as a known fidelity gap.
- Consent default **true** (parity with Android's `ConsentStore`), withdrawable
  in-keyboard; host explains BR-22 in plain language and links out.
- Device key: 32 bytes, Data Protection `.completeUntilFirstUserAuthentication`,
  `isExcludedFromBackup`; a `KeyProvider` seam that **prefers Keychain/App-Group when
  reachable** and falls back to the container file otherwise (one seam, two backends).
- No core change; learning is wiring over the frozen FFI (BR-70).
- Ship a "clear learned data" affordance **in the keyboard** for the no-Full-Access
  state (the host can't do it).

**Fallback:** if review rejects the in-keyboard consent surface, fall back to C
(learning off) rather than A (Full Access) — never buy learning with the permission
the brand disowns.

---

## 7. Alternatives rejected

| Alternative | Why rejected |
|---|---|
| **Require Full Access to enable learning (A as default)** | Trades the privacy brand (BR-27) for host-sync; depresses adoption of the feature it enables; contradicts "minimum permissions." Kept only as a user-elected upgrade. |
| **Pasteboard / URL-scheme handshake to sync host consent → extension without an App Group** | Fragile, racy, and itself a privacy smell (pasteboard is observable). Rejected; the in-keyboard toggle is simpler and honest. |
| **Reimplement a Swift learner / persistence to sidestep the core** | Violates BR-70 and the CLAUDE.md §5 smell test; two learners drift. The core already gates and encrypts. |
| **Call `import_context`/`import_frequencies` on iOS** | No iOS legacy data exists; building a migrator for nothing is speculative (KISS). Deferred until an iOS-format migration ever exists. |
| **Store the device key in `UserDefaults` / plist unprotected** | Fails BR-62 (must be encrypted at rest); Data-Protection file is the correct primitive without the Keychain. |
| **Treat every field as sensitive to be "safe" (never learn)** | That is Option C by the back door; defeats the feature. The §4.3 gate is conservative *when uncertain*, not always-off. |

---

## 8. Deferred (recorded, not built — KISS)

1. **Full-Access upgrade path implementation** (the Keychain/App-Group backend of the
   `KeyProvider` seam + store re-key migration) — designed here, built when a user
   demand or store-manageability need justifies it.
2. **Host-side "manage / clear learned data" UI** — only meaningful under Full Access
   (App Group); the no-Full-Access "clear" lives in the keyboard.
3. **iOS→iOS or Android→iOS model migration** (`import_*`) — no legacy exists yet.
4. **Extending BR-26 fidelity** if Apple ever surfaces a personalized-learning hint.

---

## Audit log

_(Appended on every `/r-u-sure` gate run, per CLAUDE.md §1.1.)_

### Pass 1 — design self-audit (subagent) — ✅ Complete for a design artifact

Audited against CLAUDE.md §1.2 (a design must name the problem, the BR IDs it
closes, the modules involved **and whether they already exist**, the port traits,
the invariants, and the alternatives rejected with reasons) and against the task's
explicit asks (all three options laid out with trade-offs; consent BR-22 + the BR-26
gate traced through **each** option; device-key security covered per option; a
recommendation).

- **Problem + hard constraint** — §1, grounded in the lived App-Group/Keychain
  failure the foundation slice hit, not asserted. ✅
- **Requirements (BR IDs)** — §2 table (BR-22/26/27/62/63/70). ✅
- **Existing code named + confirmed existing** — §3 cites `ffi.rs` learn/persist/
  import surface, `sensitive-context::SensitivityPolicy`, the `SensitiveField`
  callback interface, and the Android references (`ConsentStore`,
  `EditorInfoSensitivity`, `KeystoreKeyProvider`, `FeatherKeyImeService`) read this
  session — verbatim signatures, not guessed. No core change. ✅
- **Port trait / seam** — the `SensitiveField` callback (implemented from
  `UITextInputTraits`, §4.3) and the `KeyProvider` two-backend seam (§6). ✅
- **Invariants** — §4: double-gate order, debounced off-path persist, backup
  exclusion, and BR-26-is-local-in-all-options. ✅
- **All three options with trade-offs** — §5 (A/B/C), each with consent, sensitive
  gate, device key, data location, and cost. ✅
- **Consent (BR-22) reach + device-key security per option** — explicit in every §5
  row; the crux (no App Group ⇒ no host→extension consent) is stated plainly, and
  the recommendation (§6) resolves it with an in-keyboard toggle + dormant
  entitlements that upgrade to host-sync. ✅
- **Recommendation with reasons + fallback** — §6, B over A (privacy/BR-27) and B
  over C (feature/roadmap), fallback to C not A. ✅
- **Alternatives rejected** — §7, six entries. ✅

Honesty notes recorded rather than smoothed over: iOS has **no** faithful
`NO_PERSONALIZED_LEARNING` analogue (§4.3); the file-based device key is device-bound
but **not** hardware-attested like Android Keystore (§5B); Option B's consent surface
is genuinely **split** until Full Access is granted (§5B). These are stated as
limitations, not hidden.

This is a **design** artifact: "complete" means audited against §1.2, the BRD, and
the task, and grounded in code read this session — behavioural verification belongs
to the (future) build gate. No implementation exists or is claimed. The Full-Access
platform facts in §1 are taken from the foundation slice's lived experience and the
task's stated constraint; if the build phase finds Keychain works for the
extension's *own* items without Full Access, §5B's key-storage choice is revisited
(it would only strengthen B, not change the recommendation).

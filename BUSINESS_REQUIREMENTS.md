# Business Requirements Document (BRD)

**Project (working name):** FeatherKey — A Fast, Private, Modular Android Keyboard
**Document type:** Business Requirements Document
**Version:** 0.7 (Draft)
**Date:** 2026-07-24
**Status:** Draft — for review
**Owner:** Product Sponsor (TBD)

> **Scope of this document:** This BRD defines *what* the business needs and *why*. It intentionally does **not** specify technical design, architecture, algorithms, or implementation. Those belong in downstream documents (Product Requirements, Technical Design, Architecture Decision Records).

---

## Table of Contents

1. [Document Control](#1-document-control)
2. [Executive Summary](#2-executive-summary)
3. [Business Objectives & Goals](#3-business-objectives--goals)
4. [Problem Statement](#4-problem-statement)
5. [Project Scope](#5-project-scope)
6. [Stakeholders](#6-stakeholders)
7. [User Personas](#7-user-personas-illustrative)
8. [Business Requirements](#8-business-requirements) — §8.1 Performance · §8.2 Accuracy & Learning · §8.3 Prediction/Autocorrect · §8.4 Multi-Language · §8.5 Privacy · §8.6 Security · §8.7 Reliability · §8.8 Design/Usability/Accessibility · §8.9 Modularity/Footprint · §8.10 iOS Parity · §8.11 Table-Stakes Typing · §8.12 Script Coverage · §8.13 Accessibility (Extended) · §8.14 Adoption/Trust/Measurement · §8.15 Business Model
9. [Success Metrics & KPIs](#9-success-metrics--kpis)
10. [Assumptions](#10-assumptions)
11. [Constraints](#11-constraints)
12. [Dependencies](#12-dependencies)
13. [Risks & Mitigations](#13-risks--mitigations)
14. [Compliance & Regulatory Considerations](#14-compliance--regulatory-considerations)
15. [Competitive Analysis & Reference Benchmark](#15-competitive-analysis--reference-benchmark) — iOS quality benchmark (§15.1–15.4) · Android direct rivals (§15.5) · market white space (§15.6)
16. [Market & Go-to-Market](#16-market--go-to-market)
17. [Release Phasing & Roadmap](#17-release-phasing--roadmap)
18. [Open Questions](#18-open-questions)
19. [Glossary](#19-glossary)
20. [Approval](#20-approval)
- [Appendix A — Embedded iOS Keyboard Research Notes](#appendix-a--embedded-ios-keyboard-research-notes)
- [Appendix B — Embedded Android Keyboard Research Notes](#appendix-b--embedded-android-keyboard-research-notes)

---

## 1. Document Control

| Field | Value |
|---|---|
| Author | TBD |
| Sponsor | TBD |
| Reviewers | Product, Engineering, Security, Design, Legal/Privacy |
| Approval status | Pending |
| Related documents | PRD (TBD), Privacy Policy (TBD), EULA (TBD), Play Data-Safety declaration (TBD), Vulnerability Disclosure Policy (TBD), Open-Source License (TBD), Design System (TBD) |

### Revision History

| Version | Date | Author | Summary |
|---|---|---|---|
| 0.1 | 2026-07-24 | TBD | Initial draft |
| 0.2 | 2026-07-24 | TBD | Added concrete iOS keyboard benchmark research (Section 15), iOS feature-parity requirements (§8.10, BR-41–46), and updated multi-language target to iOS's 3-language capability |
| 0.3 | 2026-07-24 | TBD | Gap-analysis pass: added table-stakes typing features (§8.11), script/RTL coverage (§8.12), extended accessibility (§8.13), adoption/trust/measurement incl. privacy-vs-KPI resolution (§8.14), business model (§8.15), and security-ops + legal-deliverable requirements (BR-62–67); decided open-source + donations model |
| 0.4 | 2026-07-24 | TBD | End-to-end completion: added Table of Contents, Market & Go-to-Market (§16), and Release Phasing & Roadmap (§17) mapping all requirements to MVP/v1.x/v2+; reconciled Scope section with new requirements; renumbered tail sections |
| 0.5 | 2026-07-24 | TBD | Closed the last gap: added Android competitive analysis (§15.5–15.6, direct rivals + market white space) and embedded Android research (Appendix B); broadened Section 15 title; updated ToC |
| 0.6 | 2026-07-24 | TBD | Reconciled MVP accuracy bar with the Technical Design: MVP exit criteria now distinguish keystroke/touch accuracy (MVP, beat-iOS) from AI prediction quality (competitive at MVP; beat-iOS is a v1.x goal gated on the neural LM). Prevents over/under-scoping MVP |
| 0.7 | 2026-07-24 | TBD | Resolved BR-17 (a Must previously scheduled v1.x): promoted instant manual language switching to **MVP** — near-free since all active languages are preloaded (Technical Design §6.1). Now every Must ships at MVP (removes the MoSCoW inconsistency) |

---

## 2. Executive Summary

Existing Android keyboards consistently disappoint users on the fundamentals: they feel slow, mistype the intended key, fail to learn the user's personal style, occasionally crash silently (sometimes forcing a phone restart), offer weak autocomplete, handle multiple languages poorly, aggressively "autocorrect" words the user actually wanted, and — most seriously — many harvest analytics or keystrokes without meaningful consent and are not hardened against a compromised device.

FeatherKey aims to deliver a keyboard that is **fast, accurate, adaptive, private-by-design, security-hardened, beautifully designed, effortlessly multilingual, and dead-simple to use** — meeting or exceeding the quality bar set by the latest iOS keyboard while being tiny in footprint and built from small, single-responsibility modules.

The central business bet: users will switch to — and stay loyal to — a keyboard that *earns their trust* (privacy + security) while *respecting their time* (speed + accuracy + learning).

---

## 3. Business Objectives & Goals

| # | Objective | Why it matters |
|---|---|---|
| OBJ-1 | Deliver best-in-class **typing accuracy** that improves over time by adapting to the individual user | Accuracy is the #1 driver of perceived keyboard quality and daily satisfaction |
| OBJ-2 | Achieve **speed and responsiveness** equal to or better than the latest iOS keyboard | Latency directly correlates with user frustration and churn |
| OBJ-3 | Be **private by design** — no data collection without explicit, informed, opt-in consent; never behave as a keylogger | Trust is the core differentiator in a market full of data-harvesting keyboards |
| OBJ-4 | Be **security-hardened** so that even a compromised device cannot easily exfiltrate keystrokes via the keyboard | Keyboards are a high-value attack surface; security is a headline promise |
| OBJ-5 | Support **multiple languages simultaneously** (at least two concurrently, e.g. Portuguese + English) with zero-friction switching | Multilingual users are underserved and highly motivated to switch |
| OBJ-6 | Deliver a **premium, smooth, modern design** that feels effortless and looks beautiful | Look-and-feel drives adoption, retention, and word-of-mouth |
| OBJ-7 | Be **tiny and modular** — small install/runtime footprint, built from well-scoped single-responsibility modules | Enables reliability, auditability, performance, and maintainability |
| OBJ-8 | Be **exceptionally reliable** — no silent failures; graceful recovery without requiring the user to restart their phone | A keyboard that disappears makes the phone unusable; reliability is non-negotiable |
| OBJ-9 | Be **dead-simple to use** — minimal setup, obvious controls, low learning curve | Simplicity broadens the addressable market and reduces support cost |

---

## 4. Problem Statement

Users report that current Android keyboards suffer from the following recurring problems. Each problem below is traced to a business requirement in Section 8.

| # | Problem (user-reported pain) | Business impact |
|---|---|---|
| P-1 | **Slow / laggy** typing experience | Frustration, errors, abandonment |
| P-2 | **Misses the intended key** — poor touch-target accuracy; feels indecisive, as if the accuracy logic is out of sync | Constant corrections, low trust |
| P-3 | **Does not learn the user's personal typing style**, so accuracy never improves | Keyboard feels generic and static |
| P-4 | **Silent exceptions / crashes** — keyboard vanishes, sometimes requiring a full phone restart to type again | Renders the phone temporarily unusable; severe |
| P-5 | **Weak autocomplete/prediction** — suggestions are often nowhere near what the user intended | Wasted taps, no value delivered |
| P-6 | **Single active language**; switching languages is a multi-step struggle (tap, hold, pick, wait to load, then "ready") | Painful for bilingual/multilingual users |
| P-7 | **Aggressive autocorrect** replaces intended (out-of-dictionary) words with something unrelated; **whitelisting new words is non-trivial** | Actively corrupts the user's intent |
| P-8 | **Privacy violations** — analytics shared without consent; some keyboards effectively act as keyloggers | Ethical and legal risk; erodes trust |
| P-9 | **Weak security posture** — on a compromised device, the keyboard can be leveraged to collect data | High-severity risk to user data |
| P-10 | **Poor look-and-feel** — ugly, cramped, keys too small, not smooth | Low adoption, poor retention |

---

## 5. Project Scope

### 5.1 In Scope

- A soft (on-screen) keyboard input method for Android phones.
- Core typing experience: layout, key input, touch accuracy, and adaptive personalization.
- Autocorrect, autocomplete, and next-word prediction.
- Simultaneous multi-language typing (minimum two concurrent languages) with frictionless switching.
- User dictionary management (adding, whitelisting, and removing custom words) that is trivial to use.
- On-device personalization/learning of the user's typing style.
- Privacy-first data handling with explicit opt-in consent for any data leaving the device.
- Security hardening of the keyboard as an input method.
- Reliability and graceful failure recovery (no silent failures; no phone restart required).
- Premium visual design, theming, and smooth interactions.
- A tiny footprint and a modular internal structure (single-responsibility modules).
- Core table-stakes typing features (number/symbol layouts, smart-typing behaviors, cursor/text editing, clipboard, ergonomic modes, haptics).
- Script coverage including right-to-left/bidirectional text, and an architecture for complex scripts.
- Fully offline operation of all core features.
- Accessibility support, including screen-reader (TalkBack) compatibility.
- An open-source product funded by donations/sponsorship/grants.

### 5.2 Out of Scope (for this release / this document)

- Physical/hardware keyboards.
- Non-Android platforms (iOS, desktop, web) — reference targets only, not deliverables.
- Handwriting recognition and stylus input (candidate for future roadmap).
- Cloud-synced personalization across devices (candidate for future roadmap; gated on privacy design).
- Third-party plugin/extension marketplace (future consideration).
- Detailed funding operations (specific donation platforms, sponsorship agreements, grant applications) — the *model* is decided (open-source + donations, §8.15), but its operational specifics are handled separately.
- Technical/architectural design, algorithm selection, and implementation details.

### 5.3 Explicit Non-Goals

- The product will **not** monetize user keystrokes or typing content.
- The product will **not** enable analytics or telemetry by default.
- The product will **not** require account creation to deliver core value.
- The product will be **open-source** and funded by donations/sponsorship/grants — **not** by monetizing user data, keystrokes, or attention (see BR-67).

---

## 6. Stakeholders

| Stakeholder | Interest / Role |
|---|---|
| End users (primary) | Fast, accurate, private, beautiful daily typing |
| Multilingual users | Seamless concurrent multi-language typing |
| Privacy-conscious users | Verifiable no-tracking, no-keylogging guarantees |
| Product Sponsor | Business outcomes, funding, prioritization |
| Product Management | Requirements, roadmap, success metrics |
| Engineering | Feasibility, delivery |
| Security team | Threat modeling, hardening, review |
| Design/UX team | Visual and interaction quality |
| Legal / Privacy / Compliance | Consent, regulatory alignment |
| Support | Reliability, defect reduction |

---

## 7. User Personas (Illustrative)

- **Maria — The Bilingual Professional.** Types daily in Portuguese and English, often mixing both in one message. Needs zero-friction concurrent language support and accurate suggestions in both.
- **Alex — The Privacy Advocate.** Refuses keyboards that phone home. Will only adopt a keyboard with verifiable privacy and security guarantees.
- **Sam — The Speed Typist.** Types fast on a small screen; hates lag and mistyped keys; wants a keyboard that keeps up and learns their habits.
- **Jordan — The Frustrated Switcher.** Has abandoned multiple keyboards due to crashes, ugly design, and dumb autocorrect. Wants something that "just works" and looks great.

---

## 8. Business Requirements

> Requirements are stated at the business level (**what** and **why**), not as technical solutions. Priority uses **MoSCoW**: **M** = Must, **S** = Should, **C** = Could, **W** = Won't (this release). Each requirement traces to a problem (P-#) and/or objective (OBJ-#).

### 8.1 Performance & Responsiveness

| ID | Requirement | Priority | Traces to |
|---|---|---|---|
| BR-1 | The keyboard must feel instantly responsive, with input latency at or below the perceived responsiveness of the latest iOS keyboard. | M | P-1, OBJ-2 |
| BR-2 | The keyboard must launch/appear quickly whenever a text field is focused, with no noticeable "loading" delay. | M | P-1, OBJ-2 |
| BR-3 | Responsiveness must be maintained on low-to-mid-range Android devices, not only flagship hardware. | S | P-1, OBJ-2 |
| BR-4 | The keyboard must maintain its footprint as *tiny* (small install size and low memory/CPU/battery usage) as a first-class product quality. | M | OBJ-7 |

### 8.2 Typing Accuracy & Adaptive Learning

| ID | Requirement | Priority | Traces to |
|---|---|---|---|
| BR-5 | The keyboard must accurately register the key the user intended to press, minimizing "missed key" errors. | M | P-2, OBJ-1 |
| BR-6 | Touch/accuracy behavior must be consistent and decisive (no perception of an out-of-sync or indecisive accuracy system). | M | P-2, OBJ-1 |
| BR-7 | The keyboard must learn the individual user's typing style over time and become measurably more accurate for that user. | M | P-3, OBJ-1 |
| BR-8 | All personalization/learning must occur on-device by default, with no typing content leaving the device without explicit consent. | M | P-3, P-8, OBJ-3 |
| BR-9 | The user must be able to view, reset, or delete what the keyboard has learned about them. | S | P-3, OBJ-3 |

### 8.3 Prediction, Autocomplete & Autocorrect

| ID | Requirement | Priority | Traces to |
|---|---|---|---|
| BR-10 | Autocomplete and next-word predictions must be genuinely relevant to what the user is typing. | M | P-5, OBJ-1 |
| BR-11 | Prediction quality must improve as the keyboard learns the user's vocabulary and habits. | S | P-3, P-5, OBJ-1 |
| BR-12 | Autocorrect must not replace a word the user clearly intended (especially out-of-dictionary words) with an unrelated word. | M | P-7, OBJ-1 |
| BR-13 | Adding a new/custom word to the user's dictionary (whitelisting) must be trivial and obvious — ideally a single, clear action. | M | P-7, OBJ-9 |
| BR-14 | The user must be able to easily review, edit, and remove words from their personal dictionary. | S | P-7, OBJ-9 |
| BR-15 | Autocorrect aggressiveness must be user-adjustable (including the ability to reduce or disable it). | C | P-7, OBJ-9 |

### 8.4 Multi-Language Support

| ID | Requirement | Priority | Traces to |
|---|---|---|---|
| BR-16 | The keyboard must support typing in at least two languages simultaneously without requiring the user to manually switch (e.g., Portuguese and English at the same time). | M | P-6, OBJ-5 |
| BR-17 | When manual language switching is used, it must be immediate and effortless — no multi-step tap-hold-pick-wait sequence and no perceptible load time. | M | P-6, OBJ-5, OBJ-9 |
| BR-18 | Predictions, autocomplete, and autocorrect must work correctly across the concurrently active languages. | M | P-5, P-6, OBJ-5 |
| BR-19 | The product must be architected to add additional languages over time; the initial supported set is TBD but must include at least the concurrent-pair capability at launch. | S | OBJ-5 |
| BR-19a | The concurrent-language target should match or exceed iOS, which combines **up to three languages** in a single keyboard for some combinations (e.g., English + two others) with automatic language detection — not only two. | S | P-6, OBJ-5 |
| BR-19b | When multiple languages are active, the keyboard must automatically detect which language is being typed and apply the correct suggestions/autocorrect, without the user tagging each word (matching iOS behavior). | M | P-6, P-7, OBJ-5 |

### 8.5 Privacy

| ID | Requirement | Priority | Traces to |
|---|---|---|---|
| BR-20 | The keyboard must never function as a keylogger or transmit keystrokes/typing content to any third party. | M | P-8, OBJ-3 |
| BR-21 | No analytics, telemetry, or data collection may occur without the user's explicit, informed, opt-in consent. Default state is no collection. | M | P-8, OBJ-3 |
| BR-22 | The user must have clear, plain-language visibility into what (if anything) is collected, and be able to withdraw consent at any time. | M | P-8, OBJ-3 |
| BR-23 | Core functionality (typing, accuracy, learning, prediction) must fully work with zero data leaving the device. | M | P-8, OBJ-3 |
| BR-24 | Any privacy claims should be independently verifiable (e.g., via transparency, audit, or openness) to build trust. | S | OBJ-3 |

### 8.6 Security

| ID | Requirement | Priority | Traces to |
|---|---|---|---|
| BR-25 | The keyboard must be security-hardened so that a compromised device cannot easily use the keyboard as a channel to collect or exfiltrate user data. | M | P-9, OBJ-4 |
| BR-26 | Sensitive input contexts (e.g., passwords) must be handled with heightened protection and must not be learned, stored, or predicted. | M | P-9, OBJ-4 |
| BR-27 | The keyboard must request the minimum permissions necessary and clearly justify each. | M | P-8, P-9, OBJ-4 |
| BR-28 | The product must undergo security review/threat modeling before launch and on an ongoing basis. | S | OBJ-4 |
| BR-62 | All personal data at rest (learned typing model, user dictionary, clipboard history) must be encrypted on the device. | M | P-8, P-9, OBJ-4 |
| BR-63 | Personal/learned data must be excluded from device cloud backups by default, so it cannot leak via a backup; inclusion requires explicit opt-in. | M | P-8, P-9, OBJ-3, OBJ-4 |
| BR-64 | The product must publish a vulnerability disclosure policy (and should offer a bug-bounty program) so security issues can be reported responsibly. | S | OBJ-4 |
| BR-65 | Third-party dependencies must be minimal, vetted, and auditable; supply-chain risk must be actively managed (reinforced by the modular design). | S | OBJ-4, OBJ-7 |
| BR-66 | The product must have a secure, timely update mechanism so security fixes reach users quickly. | M | P-4, OBJ-4, OBJ-8 |

### 8.7 Reliability & Failure Recovery

| ID | Requirement | Priority | Traces to |
|---|---|---|---|
| BR-29 | The keyboard must not fail silently; if an internal error occurs, it must recover gracefully and remain usable. | M | P-4, OBJ-8 |
| BR-30 | A keyboard failure must never require the user to restart their phone in order to type again. | M | P-4, OBJ-8 |
| BR-31 | The keyboard must remain available and functional across app switches, device states, and extended use. | M | P-4, OBJ-8 |

### 8.8 Design, Usability & Accessibility

| ID | Requirement | Priority | Traces to |
|---|---|---|---|
| BR-32 | The keyboard must have a premium, modern, visually beautiful design that meets or exceeds the latest iOS keyboard's look-and-feel. | M | P-10, OBJ-6 |
| BR-33 | Interactions must feel smooth and polished (fluid animations, comfortable key sizing, satisfying feedback). | M | P-10, OBJ-6 |
| BR-34 | Keys must be appropriately sized and spaced for comfortable, accurate typing (not cramped or too small). | M | P-2, P-10, OBJ-6 |
| BR-35 | The keyboard must be dead-simple to set up and use, with an obvious, low-friction first-run experience. | M | OBJ-9 |
| BR-36 | The product should offer theming/appearance options (e.g., light/dark) consistent with a premium design system. | S | OBJ-6 |
| BR-37 | The keyboard should meet baseline accessibility expectations for text input. | S | OBJ-9 |

### 8.9 Modularity & Footprint (Product Quality Requirements)

| ID | Requirement | Priority | Traces to |
|---|---|---|---|
| BR-38 | The product must be built as a set of small, well-scoped, single-responsibility modules (each module does one thing well). | M | OBJ-7 |
| BR-39 | Modularity must support auditability, reliability, and independent evolution of capabilities. | S | OBJ-4, OBJ-7, OBJ-8 |
| BR-40 | The overall product must remain tiny in size and resource usage as a sustained, measured quality attribute. | M | OBJ-7, OBJ-2 |

### 8.10 Feature Parity with iOS (Reference Benchmark)

> These requirements name specific capabilities of the current iOS keyboard so "parity-or-better" is concrete rather than aspirational. See Section 15 for the researched feature list and sources.

| ID | Requirement | Priority | Traces to |
|---|---|---|---|
| BR-41 | The keyboard should support gesture/swipe typing (equivalent to iOS "Slide to Type") as an input method, without it conflicting with quick single-key taps. | S | OBJ-2, OBJ-9 |
| BR-42 | The keyboard should offer inline predictive text (an inline, accept-with-one-gesture completion), matching iOS inline prediction. | S | P-5, OBJ-1 |
| BR-43 | If dictation (voice-to-text) is offered, it must honor the same privacy/on-device guarantees as typing — no audio or transcript leaves the device without explicit consent. iOS 27 builds dictation into the keyboard; parity here must not compromise BR-20–23. | C | OBJ-3, OBJ-9 |
| BR-44 | Emoji entry and search must be fast and easy to reach, matching modern iOS emoji-keyboard convenience. | C | OBJ-6, OBJ-9 |
| BR-45 | Autocorrect should be able to offer alternative-word choices (not a single forced replacement), aligning with the iOS 27 "expanded autocorrect / alternative words" direction and directly serving BR-12. | S | P-7, OBJ-1 |
| BR-46 | The keyboard must **not** reproduce iOS's known regressions — specifically the "characters missed when typing quickly" defect (present in iOS 26, fixed only in iOS 26.4) and predictive/slide-to-type features conflicting with quick taps. These are explicit "must-beat," not just "match," targets. | M | P-1, P-2, P-4, OBJ-1, OBJ-2 |

### 8.11 Core Typing Features (Table Stakes)

> These are baseline features every credible keyboard must have. They are listed so the "great fundamentals" bar is explicit and not assumed. Missing any of these is a product failure regardless of the differentiators above.

| ID | Requirement | Priority | Traces to |
|---|---|---|---|
| BR-47 | The keyboard must provide number, symbol, and punctuation layouts that are quick to reach. | M | OBJ-9 |
| BR-48 | The keyboard must support standard smart-typing behaviors: auto-capitalization, double-space-to-period, and automatic/smart punctuation — each user-toggleable. | M | OBJ-1, OBJ-9 |
| BR-49 | The keyboard must provide cursor control and text-editing controls (e.g., spacebar/gesture cursor movement, and select / cut / copy / paste). | M | OBJ-9 |
| BR-50 | The keyboard should offer a clipboard history/manager; because the clipboard is sensitive, it must handle sensitive content carefully (e.g., exclude password fields, support auto-expiry and easy clearing) and keep clipboard data on-device and encrypted. | S | P-8, P-9, OBJ-4, OBJ-9 |
| BR-51 | The keyboard should offer ergonomic modes for large phones and one-handed use (e.g., one-handed, resizable, floating, or split layouts). | S | P-10, OBJ-6, OBJ-9 |
| BR-52 | The keyboard should provide haptic and sound feedback on key presses, user-configurable (including off). | S | P-10, OBJ-6 |

### 8.12 Script & Language Coverage

| ID | Requirement | Priority | Traces to |
|---|---|---|---|
| BR-53 | The keyboard must correctly support right-to-left (RTL) and bidirectional text (e.g., Arabic, Hebrew), including mixed RTL/LTR input, for any RTL language it ships. | M | OBJ-5 |
| BR-54 | The product must be architected to support complex scripts (e.g., Indic scripts, CJK input methods) as languages are added over time. | S | OBJ-5 |

### 8.13 Accessibility (Extended)

> Extends the baseline BR-37. For a product used in nearly every text field, accessibility is both a quality bar and, in many markets, a legal requirement (e.g., EN 301 549, ADA-class expectations).

| ID | Requirement | Priority | Traces to |
|---|---|---|---|
| BR-55 | The keyboard must be compatible with the platform screen reader (Android TalkBack) so non-visual users can type. | M | OBJ-9 |
| BR-56 | The keyboard should support high-contrast and large-text/adjustable key sizing, and be usable via accessibility input methods (e.g., switch access) for motor-impaired users. | S | OBJ-9 |

### 8.14 Adoption, Trust & Measurement

> Addresses keyboard-specific adoption friction and the tension between privacy-by-default and the need to measure/improve the product.

| ID | Requirement | Priority | Traces to |
|---|---|---|---|
| BR-57 | The keyboard should let users import their existing personal dictionary / custom words (and, where possible, preferences) from their previous keyboard, to reduce switching cost. | S | P-3, P-7, OBJ-9 |
| BR-58 | Onboarding must transparently address the Android system warning shown when enabling any third-party keyboard ("this keyboard may be able to collect all the text you type") — proactively explaining the product's privacy stance at that exact moment, so the OS warning builds trust rather than deterring adoption. | M | P-8, OBJ-3, OBJ-9 |
| BR-59 | All core functionality (typing, accuracy, learning, prediction, autocorrect) must work fully offline, with no network connectivity required. | M | P-8, OBJ-3 |
| BR-60 | **Privacy-preserving measurement principle:** any product analytics or quality measurement must be opt-in, aggregated, and free of typing content; the product must be improvable and its KPIs assessable **without** collecting per-user typing content. This resolves the tension between BR-21 (no default collection) and Section 9 (KPIs). | M | P-8, OBJ-3 |
| BR-61 | Reliability/crash diagnostics (needed to eliminate the silent-failure problem) must themselves be opt-in and content-free — never a backdoor to keystroke data. | S | P-4, P-8, OBJ-3, OBJ-8 |

### 8.15 Business Model & Verifiability

> Reflects the decided direction: an open-source product funded by donations/sponsorship rather than user data.

| ID | Requirement | Priority | Traces to |
|---|---|---|---|
| BR-67 | The product will be **open-source**, so its privacy and security claims can be independently inspected and verified (directly satisfying BR-24), and will be funded through donations/sponsorship/grants rather than monetizing user data. | S | OBJ-3, OBJ-4 |
| BR-68 | The user must be able to choose their alphabetic key layout (QWERTY, QWERTZ, AZERTY, …) independently of the selected language(s), and that layout is used for all Latin-script typing. The default matches the system's layout where detectable, falling back to the selected language's default (QWERTY for most). | S | OBJ-9 |

---

## 9. Success Metrics & KPIs

> Targets are placeholders to be finalized with the sponsor. Per **BR-60**, every metric here must be obtainable **without** violating the privacy requirements — i.e., only via opt-in, aggregated, content-free signals or on-device measurement. Metrics that cannot be gathered that way must be dropped or redesigned, not collected covertly.

| Area | Example KPI | Target (TBD) |
|---|---|---|
| Speed | Perceived input latency vs. latest iOS keyboard | ≤ iOS baseline |
| Accuracy | Reduction in user corrections/backspaces over time per user (on-device) | Improves week-over-week |
| Learning | Measurable per-user accuracy gain after N days of use | Positive trend |
| Prediction | Suggestion acceptance rate | ↑ vs. baseline keyboards |
| Multi-language | Time/steps to switch or type across two languages | Near-zero friction |
| Reliability | Crash/silent-failure rate; incidents requiring restart | ≈ 0 |
| Privacy | % of users who can complete core tasks with zero data egress | 100% |
| Adoption | Set-as-default rate after install | TBD |
| Retention | 30-day retention | TBD |
| Satisfaction | App rating / NPS | TBD |
| Trust | % users citing privacy/security as reason to adopt | TBD |

---

## 10. Assumptions

- Users are willing to switch keyboards if the alternative is meaningfully better on speed, accuracy, privacy, and design.
- A tiny, modular, on-device approach can meet the accuracy and prediction bar without cloud processing.
- The latest iOS keyboard is a valid quality/feature reference point for parity-or-better goals.
- Delivering core value on-device (no accounts, no cloud) is feasible for the target scope.
- Target platform is modern Android phone versions (specific minimum version TBD).

---

## 11. Constraints

- **Platform:** Android input-method framework and OS constraints apply.
- **On-device first:** Core features must work without server dependency, constraining what can rely on cloud compute.
- **Footprint:** "Tiny" is a hard product constraint, bounding size and resource usage.
- **Privacy-by-default:** No default data collection constrains how the product is measured and improved.
- **Timeline & budget:** TBD.

---

## 12. Dependencies

- Android input-method platform capabilities and their evolution.
- Language resources/dictionaries for each supported language (sourcing and licensing TBD).
- Design system and asset production.
- Security review and privacy/legal review capacity.

---

## 13. Risks & Mitigations

| Risk | Likelihood | Impact | Mitigation direction |
|---|---|---|---|
| On-device-only limits prediction/accuracy quality | Med | High | Prioritize strong on-device personalization; validate against iOS bar early |
| "Tiny + modular" tension with rich features | Med | Med | Enforce single-responsibility modules; measure footprint continuously |
| Achieving iOS-parity design on Android is hard | Med | Med | Invest in design early; treat look-and-feel as a first-class requirement |
| Reliability/silent-failure defects damage trust | Low | High | Make graceful recovery a launch gate; never require phone restart |
| Privacy/security promises are hard to prove | Med | High | Pursue verifiable/transparent approaches to earn trust |
| Multi-language complexity increases footprint & bugs | Med | Med | Scope concurrent-language support carefully; add languages incrementally |
| Market skepticism (users burned by prior keyboards) | High | Med | Lead marketing with trust + demonstrable quality differentiators |

---

## 14. Compliance & Regulatory Considerations

- Applicable data-protection regulations (e.g., consent, data minimization, right to deletion) must be honored; specific jurisdictions TBD with Legal.
- Platform (app store) policies for keyboards, permissions, and data disclosure must be met — including the Google Play **Data Safety** declaration (which must truthfully reflect the no-collection stance).
- Clear, plain-language privacy disclosures are required.
- **Children/minors:** if the product may be used by children, applicable protections (e.g., COPPA and equivalents) must be assessed and honored.
- **Accessibility law:** conformance with applicable accessibility standards (e.g., EN 301 549 / ADA-class) must be assessed (see BR-55, BR-56).
- **Required legal/policy deliverables:** Privacy Policy, End-User License Agreement (EULA), Play Store Data-Safety declaration, vulnerability disclosure policy (BR-64), and the open-source license (BR-67).

*(Detailed compliance mapping to be completed by Legal/Privacy.)*

---

## 15. Competitive Analysis & Reference Benchmark

The **latest iOS keyboard** is the primary *quality* reference for FeatherKey (§15.1–15.4). The **leading Android keyboards** are the direct *market* rivals FeatherKey must displace (§15.5). §15.6 identifies the market white space this project targets. All facts were researched July 2026; raw notes with sources are embedded in Appendices A (iOS) and B (Android).

### 15.1 Version Context

Apple uses year-based OS naming. As of July 2026: **iOS 26** is the shipping release (launched 2025), and **iOS 27** — announced at WWDC on 2026-06-09 — is in developer beta, expected to ship around September 2026. FeatherKey should benchmark against **iOS 27** (the "latest of the latest"), with iOS 26 as the current-shipping baseline.

### 15.2 iOS Keyboard Capabilities to Match or Beat

| Capability | What iOS does | FeatherKey stance | Requirement |
|---|---|---|---|
| **Autocorrect** | Transformer/ML-based autocorrect; iOS 27 "expands autocorrect by offering alternative words" rather than one forced replacement | **Beat** — never clobber intended/out-of-dictionary words; offer alternatives; trivial whitelisting | BR-12, BR-13, BR-45 |
| **Predictive text** | Above-keyboard word/emoji suggestions **plus inline predictive text** (gray inline completion you accept) | **Match** | BR-10, BR-42 |
| **Slide to Type** | Swipe-across-keys gesture typing | **Match** (without conflicting with quick taps) | BR-41 |
| **Multilingual typing** | A single combined keyboard for **up to three languages** with automatic per-word language detection (no manual switching); e.g., English + two others | **Match/beat** — ≥2 concurrent at launch, architected toward 3 | BR-16, BR-18, BR-19a, BR-19b |
| **Dictation** | iOS 27 builds dictation into the keyboard, auto-correcting spelling, punctuation, and capitalization | **Match only if privacy-preserving** (on-device / consented) | BR-43 |
| **Language breadth** | iOS 27 adds many new keyboards (e.g., Afrikaans, Basque, Galician, Luxembourgish, Xhosa, Zulu, and Indigenous languages such as Cree, Kiowa, Comanche) | **Roadmap** — architect for incremental language addition | BR-19 |
| **Emoji entry** | Dedicated emoji keyboard with quick access/scrolling | **Match** | BR-44 |
| **Look & feel** | Clean, smooth, comfortable key sizing; polished animations | **Match/beat** | BR-32–34 |
| **Writing assistance** | Grammarly-style writing upgrade rumored for iOS 27 | **Consider (future)** — only if on-device/privacy-safe | Roadmap |

### 15.3 iOS Weaknesses — FeatherKey's Opening

Crucially, iOS is **not** flawless on the exact issues that motivated this project — which both validates the problem and defines where FeatherKey must clearly win:

- **iOS 26 shipped a "characters missed when typing quickly" bug** — letters appeared tapped but were not inserted, causing autocorrect to mangle words. It was fixed only in **iOS 26.4** (March 2026). This is essentially the same "misses the key I aimed for" complaint (P-2) — from the market leader. → FeatherKey targets this as a **must-beat** (BR-5, BR-6, BR-46).
- **Predictive text / Slide-to-Type conflicting with quick taps** — a reported source of mismatches. → BR-41, BR-46.
- **Autocorrect frustration** persists across iOS versions (a long-standing, widely reported complaint). → BR-12, BR-45.

### 15.4 FeatherKey Differentiators Beyond iOS Parity

Areas where FeatherKey aims to exceed iOS outright, not merely match:

- **Privacy-by-default** — no telemetry/keylogging; core features fully work with zero data egress (BR-20–24).
- **Security hardening against a compromised device** (BR-25–28).
- **Tiny footprint** and a **modular, auditable** single-responsibility structure (BR-4, BR-38–40).
- **User-controlled, adjustable autocorrect** and effortless custom-word whitelisting (BR-13–15).

### 15.5 Android Competitive Landscape — Direct Rivals

These are the keyboards FeatherKey must actually win users away from. The pattern is stark: the **mainstream leaders have great features but hostile privacy and are closed-source**, while the **open-source/privacy options are trustworthy but trail on prediction quality, design polish, and effortless multilingual typing**.

| Keyboard | Owner | Strengths | Weaknesses / gaps FeatherKey exploits | Privacy posture |
|---|---|---|---|---|
| **Gboard** | Google | Market leader; free, no ads; highly accurate swipe; voice input (accuracy reported ~97%, secondary source); clipboard; Translate row; Gemini AI (2026); themes | Closed-source; telemetry **on by default**; sends usage metadata + Android ID | **Poor.** Sends app-typed-in, word length, typing duration, languages, Android ID. *Secondary reporting* also claims federated-learning updates could allow keystroke reconstruction — **not yet verified against the primary academic paper (see caveat below); treat as unconfirmed until checked.** |
| **SwiftKey** | Microsoft | Best-in-class multilingual — up to **5 languages** at once, 700+ supported, real-time code-switching; strong personal-style learning (~68% next-word); Copilot AI | Closed-source; **login only via Microsoft account** from 2026-05-31; account-centric; cloud backup of dictionary | **Mixed.** Learns on-device but optionally backs up personal dictionary to Microsoft cloud |
| **Samsung Keyboard** | Samsung | Galaxy default; mostly offline; solid predictions; reported EFF privacy scorecard ~9/10 | **Galaxy-only** (not available to most Android users); "smartest features" can call a server; closed-source | **Good-ish** but device-locked and not verifiable |
| **HeliBoard** | Open-source (OpenBoard/AOSP fork) | 100% offline; **no internet permission**; clipboard; multilingual/multi-layout; lightweight; on F-Droid | Prediction/autocorrect quality and design polish trail Gboard; smaller ecosystem | **Excellent** (offline, open, auditable) |
| **AnySoftKeyboard** | Open-source | Extensive languages; customization; themes; gesture typing; no cloud servers; offline | Dated UX/design; prediction trails mainstream | **Excellent** |
| **FlorisBoard** | Open-source | Privacy-focused; modular layouts; customization; multilingual; clipboard; gestures; no trackers | Long-running beta maturity; prediction/polish still catching up | **Excellent** |
| **Typewise** | Typewise | Privacy-first; fully on-device autocorrect/prediction; collects nothing you type; strong learning | Niche; unconventional honeycomb/hexagonal layout raises the learning curve (conflicts with "dead-simple") | **Excellent** |

### 15.6 Market White Space — Where FeatherKey Wins

The competitive map reveals a clear, unoccupied position:

- **Mainstream (Gboard, SwiftKey):** excellent accuracy, prediction, design, and multilingual — but **privacy-hostile, closed-source, and increasingly account-gated**. They cannot credibly promise what FeatherKey promises.
- **Open-source/privacy (HeliBoard, AnySoftKeyboard, FlorisBoard):** genuinely private and auditable — but **trail on prediction quality, design polish, adaptive learning, and effortless concurrent-language typing**. They win the trust argument and lose the delight argument.
- **Samsung Keyboard:** good and mostly private — but **locked to Galaxy hardware**, so unavailable to most users.
- **Typewise:** private *and* good at autocorrect — but its **unconventional layout undercuts "dead-simple,"** keeping it niche.

**No existing keyboard combines all of:** mainstream-grade accuracy and adaptive learning · iOS-class design and smoothness · effortless 2–3 concurrent languages · **verifiable open-source privacy** · security hardening · tiny footprint · dead-simple UX. **That intersection is precisely FeatherKey's thesis** — it takes the *delight* of the mainstream and the *trust* of the open-source field, and refuses the compromise each side currently makes. This directly validates OBJ-1 through OBJ-9.

> **Strategic caution:** matching the mainstream on prediction/accuracy while staying fully on-device (no cloud, tiny footprint) is exactly where the open-source field has fallen short. This is FeatherKey's hardest bet and its make-or-break differentiator — see Risk "On-device-only limits prediction/accuracy quality" (Section 13).

### 15.7 Follow-up

*The direct-platform competitive field (above) is now covered alongside the iOS quality benchmark. A periodic refresh is recommended, as this is a fast-moving space (AI features and account/privacy policies changed materially in 2026).*

**Sources:**
- [iOS 27 official — WWDC 2026 (Tom's Guide)](https://www.tomsguide.com/phones/iphones/ios-27-is-official-all-the-new-upgrades-and-features-announced-at-wwdc-2026)
- [Apple lists 250 changes across iOS 27 (MacRumors)](https://www.macrumors.com/2026/06/10/apple-lists-250-changes-ios-27-and-more/)
- [iOS 27 Grammarly-style keyboard upgrade (TechRadar)](https://www.techradar.com/phones/iphone/your-iphone-could-be-getting-a-grammarly-style-upgrade-for-its-keyboard-when-ios-27-launches)
- [iOS 26.4 fixes iPhone keyboard accuracy bug (MacRumors)](https://www.macrumors.com/2026/03/18/ios-26-4-iphone-keyboard-bug-fix/)
- [iOS 26.4 typing accuracy bug quashed (AppleInsider)](https://appleinsider.com/articles/26/03/18/keyboard-accuracy-bug-quashed-in-ios-264)
- [iPhone keyboard is a mess in iOS 26 (The Mac Observer)](https://www.macobserver.com/news/ios-keyboard-is-a-mess-in-ios-26-and-users-have-had-enough/)
- [Manage bilingual/multilingual keyboards on iPhone, iOS 18 (macReports)](https://macreports.com/how-to-manage-bilingual-and-multilingual-keyboards-on-iphone-in-ios-18/)
- [Use three languages in one keyboard, iOS 18 (Apple Support)](https://support.apple.com/en-am/121233)
- [Best keyboard apps for Android 2026 — Gboard vs SwiftKey (GetFree.APP)](https://getfree.app/blog/best-keyboard-apps-2026)
- [Gboard is a privacy nightmare — evidence & Trinity College Dublin study (Android Police)](https://www.androidpolice.com/gboard-is-a-privacy-nightmare-what-you-can-do-about-it/)
- [How Private Are Android Keyboards? — academic study (Trinity College Dublin, D. Leith)](https://www.scss.tcd.ie/Doug.Leith/pubs/gboard_kamil.pdf)
- [4 open-source alternatives to Gboard, tested (MakeUseOf)](https://www.makeuseof.com/best-open-source-gboard-alternatives-tested/)
- [HeliBoard — privacy-conscious open-source keyboard (GitHub)](https://github.com/HeliBorg/HeliBoard)
- [SwiftKey Microsoft-account login change (PhoneArena)](https://www.phonearena.com/news/swiftkey-will-soon-allow-login-only-via-microsoft-account_id179027)
- [Privacy at Typewise — on-device keyboard (Typewise)](https://www.typewise.app/blog/privacy-typewise-keyboard-secure)
- [Best privacy-focused Android keyboards 2026, ranked (Factually)](https://factually.co/product-reviews/electronics-tech/best-privacy-focused-android-keyboards-2026-ranked-997083)

---

## 16. Market & Go-to-Market

> Business-level market framing. Specific numbers/geographies are placeholders for the sponsor; sensible defaults are noted given the project's multilingual emphasis.

### 16.1 Target Market

- **Primary:** privacy- and quality-conscious smartphone users frustrated by mainstream keyboards — especially **multilingual users** (the Portuguese + English use case is representative) and **privacy/security-conscious users** who distrust data-harvesting keyboards.
- **Beachhead suggestion:** bilingual markets where two languages are used interchangeably (e.g., Brazil/Portugal + English, US Latino/bilingual, India's English + regional languages) — an underserved segment with high motivation to switch (see personas, Section 7).
- **Secondary:** the broad base of users who simply want a faster, prettier, more reliable keyboard than what ships by default.

### 16.2 Positioning

FeatherKey = *the keyboard that respects your time and your privacy.* Four pillars, none of which incumbents market together: **fast & accurate**, **private-by-default & open-source**, **security-hardened**, **beautiful & simple**. The open-source nature (BR-67) is itself the proof of the privacy/security claims — a marketing asset, not just an engineering choice.

### 16.3 Distribution Channels

| Channel | Rationale |
|---|---|
| Google Play Store | Primary reach; requires truthful Data-Safety declaration (Section 14) |
| F-Droid / IzzyOnDroid | Natural fit for an open-source, privacy-first app; reaches the trust-focused audience |
| Direct APK from project site | Transparency and control for advanced users |
| Source repository | Public code for verifiability, contribution, and audits (BR-24, BR-67) |

### 16.4 Adoption Strategy

- Convert the scary Android "this keyboard can collect all you type" enablement warning into a trust moment (BR-58).
- Lower switching cost via dictionary/preferences import (BR-57).
- Lead with demonstrable, verifiable trust (open source) and visible quality (speed, accuracy, design) — the differentiators incumbents can't easily copy.
- Community-driven growth (word of mouth, privacy communities, open-source contributors) aligned with the donation-funded model.

### 16.5 Open GTM Decisions

Launch geographies and initial language set, marketing budget/channels, community/support infrastructure, and the sustainability/funding plan are to be finalized with the sponsor (see Section 18).

---

## 17. Release Phasing & Roadmap

> **Purpose:** show that the full feature set is achievable by sequencing it into deliverable releases, and make clear which requirements are non-negotiable at launch. Phasing uses the MoSCoW priorities from Section 8.
>
> **Guiding rule:** the product's core *promises* — fast, accurate, private, secure, reliable, simple, beautiful — must all hold **at MVP**. Privacy and security are **not** deferrable to a later version; a keyboard that is "private later" is not private. Breadth (more languages, extra input modes) is what phases in over time.

### 17.1 Phase Overview

| Phase | Theme | Goal |
|---|---|---|
| **MVP (v1.0)** | Fast, private, accurate core | A trustworthy, delightful daily keyboard that already beats incumbents on the fundamentals |
| **v1.x** | Parity & polish | Match/beat iOS feature parity; deepen accessibility; add convenience features |
| **v2+** | Breadth & reach | Wider language/script coverage and additional input modes |

### 17.2 MVP (v1.0) — Must Ship

Contains **every "Must" (M-priority) requirement** — with no Must deferred to a later release. Represents the minimum that can honestly be called fast, private, secure, accurate, reliable, and simple.

- **Performance & footprint:** BR-1, BR-2, BR-4
- **Accuracy & on-device learning:** BR-5, BR-6, BR-7, BR-8; must-beat iOS regressions BR-46
- **Prediction & autocorrect:** BR-10, BR-12, BR-13
- **Multi-language:** BR-16, BR-17, BR-18, BR-19b (≥2 concurrent with automatic detection; instant manual switch between the already-loaded languages — near-free since all active languages are preloaded, per Technical Design §6.1)
- **Privacy:** BR-20, BR-21, BR-22, BR-23; measurement principle BR-60; offline BR-59
- **Security:** BR-25, BR-26, BR-27, BR-62, BR-63, BR-66
- **Reliability:** BR-29, BR-30, BR-31
- **Design & simplicity:** BR-32, BR-33, BR-34, BR-35
- **Modularity & footprint:** BR-38, BR-40
- **Table-stakes typing:** BR-47, BR-48, BR-49, BR-68
- **Scripts:** BR-53 (if an RTL language is in the launch set)
- **Accessibility:** BR-55 (TalkBack)
- **Trust & model:** BR-58 (onboarding trust), BR-67 (open-source)

**MVP exit criteria:** meets/beats the latest iOS keyboard on **latency** and **keystroke/touch accuracy** — the "misses the key I aimed for" pain, including fast-typing (BR-46) — and is **competitive (good, relevant)** on autocomplete/prediction quality (BR-10). *Note:* **beating iOS on AI prediction/autocorrect quality is a v1.x goal**, gated on the neural language model (see the Technical Design, ADR-3), because the tiny statistical MVP engine is expected to match iOS on keystroke accuracy but not necessarily on predictive intelligence. Additionally: zero silent-failure/restart incidents in test; core works fully offline; independent confirmation that no typing content leaves the device.

### 17.3 v1.x — Parity & Polish (Should)

- **Performance:** BR-3 (low/mid-range devices)
- **Learning/prediction:** BR-9, BR-11, BR-14
- **Language switching depth:** BR-19a (toward 3 concurrent languages). *Note: instant manual switching itself now ships at MVP (moved from this phase).*
- **iOS parity features:** BR-41 (swipe), BR-42 (inline prediction), BR-45 (alternative-word autocorrect)
- **Prediction-quality parity/beat vs iOS:** delivered here via the neural language model (Technical Design ADR-3). MVP ships *competitive* statistical prediction; **beating iOS on AI prediction/autocorrect quality is a v1.x target**, not an MVP gate.
- **Convenience:** BR-15 (autocorrect aggressiveness), BR-50 (clipboard), BR-51 (ergonomic modes), BR-52 (haptics), BR-36 (theming)
- **Accessibility depth:** BR-37, BR-56
- **Adoption:** BR-57 (import), BR-61 (opt-in diagnostics)
- **Security/verifiability maturity:** BR-24, BR-28, BR-39, BR-64, BR-65

### 17.4 v2+ — Breadth & Reach (Could / Future)

- **Input modes:** BR-43 (privacy-preserving dictation), BR-44 (emoji/sticker breadth)
- **Scripts & languages:** BR-54 (complex scripts), BR-19 (broader language catalogue)
- **Beyond current scope (future roadmap):** cross-device sync (privacy-gated), writing assistance, handwriting/stylus — see Section 5.2.

### 17.5 Achievability Note

The phasing is designed so the "tiny + modular" constraint (BR-38, BR-40) is protected: each module does one thing, features are added as modules rather than by bloating a monolith, and MVP deliberately limits breadth (language count, extra input modes) while guaranteeing depth (accuracy, privacy, security, reliability). Key achievability risks and mitigations are tracked in Section 13.

---

## 18. Open Questions

- What is the minimum supported Android version and device tier?
- Which languages are in the initial supported set, and which pair(s) must be concurrent at launch?
- ~~What is the business/monetization model?~~ **Decided:** open-source, funded by donations/sponsorship/grants (BR-67). Remaining sub-question: what specific funding channels and sustainability plan (e.g., foundation, recurring sponsors, grants)?
- What are the concrete numeric targets for the KPIs in Section 9?
- What level of privacy verifiability (audit, transparency, openness) is committed for launch?
- Is cross-device personalization sync a future goal, and under what privacy constraints?

---

## 19. Glossary

| Term | Definition |
|---|---|
| Autocomplete | Completing the current word as the user types |
| Autocorrect | Automatically replacing a typed word with a corrected form |
| Next-word prediction | Suggesting the likely next word |
| On-device | Processing that occurs on the user's phone, with no data sent externally |
| Concurrent languages | Two or more languages usable at once without manual switching |
| User dictionary | The user's personal set of custom/whitelisted words |
| Footprint | Install size and runtime resource usage (memory, CPU, battery) |
| Single-responsibility module | A component scoped to do exactly one thing well |
| MoSCoW | Prioritization scheme: Must / Should / Could / Won't |
| IME | Input Method Editor — Android's term for a keyboard/input app |
| RTL / bidirectional | Right-to-left scripts (e.g., Arabic, Hebrew) and mixed RTL/LTR text |
| At-rest encryption | Encrypting stored data on the device so it's unreadable if extracted |
| Data Safety declaration | Google Play's required disclosure of what data an app collects/shares |
| TalkBack | Android's built-in screen reader |

---

## 20. Approval

| Role | Name | Decision | Date |
|---|---|---|---|
| Product Sponsor | TBD | ☐ Approve ☐ Revise | |
| Product Management | TBD | ☐ Approve ☐ Revise | |
| Engineering Lead | TBD | ☐ Approve ☐ Revise | |
| Security Lead | TBD | ☐ Approve ☐ Revise | |
| Design Lead | TBD | ☐ Approve ☐ Revise | |
| Legal / Privacy | TBD | ☐ Approve ☐ Revise | |

---

## Appendix A — Embedded iOS Keyboard Research Notes

> **Purpose:** This appendix embeds the raw researched facts about the iOS keyboard (gathered July 2026) directly in the document, so readers — human or AI — have the benchmark data on hand without querying external websites. These are captured facts as of the dates noted; iOS 27 was in beta at capture time and may change before general availability. Sources are listed at the end of Section 15 and repeated per item below.

### A.1 iOS Version Timeline

- Apple uses **year-based OS naming**.
- **iOS 26** — shipping release, launched 2025. Current baseline as of July 2026.
- **iOS 27** — announced at **WWDC on 2026-06-09**; developer betas underway (beta 2 seeded 2026-06-22); expected general availability ~September 2026. This is the "latest of the latest" benchmark. *(Sources: MacRumors WWDC/250-changes; MacRumors beta 2; Tom's Guide.)*

### A.2 iOS 26 Keyboard — Shipping Baseline

- **Predictive text**: word and emoji suggestions shown above the keyboard as you type, plus **inline predictive text** (an inline gray completion you can accept).
- **Slide to Type**: swipe-across-keys gesture typing.
- **Autocorrect**: ML/transformer-based (introduced iOS 17), widely criticized across versions.
- **Known accuracy defect (directly relevant to this project):** characters were missed when typing quickly — *"some characters to be missed when a user was typing quickly. The character appeared to be tapped in the keyboard, but was ultimately not inserted."* This cascaded into autocorrect failures because the engine couldn't interpret intended words when letters went missing. It drew substantial complaints (e.g., Reddit). **Fixed in iOS 26.4** (released the week of 2026-03-18): Apple's notes cite *"improved keyboard accuracy when typing quickly."* *(Sources: MacRumors iOS 26.4 bug fix, by Juli Clover, 2026-03-18; AppleInsider; The Mac Observer.)*
- **Reported friction:** predictive text and Slide-to-Type conflicting with quick single-key taps, introducing mismatches. *(Source: search summary of iOS 26 keyboard complaints.)*

### A.3 iOS 27 Keyboard — Latest (Beta at Capture Time)

**Typing & autocorrect**
- **Enhanced autocorrect** that *"expands autocorrect by offering alternative words"* (choices rather than a single forced replacement).
- *"Automatic punctuation when typing on multilingual keyboards."*
- Improved **multilingual grammar checking**.
- *"Faster multilingual text processing for handwriting in multiple languages."*
- **Rumored**: a Grammarly-style writing-assistance upgrade to the keyboard. *(Source: TechRadar — rumor, not confirmed.)*

**Predictive text & suggestions**
- *"Smart language and keyboard configuration suggestions."*
- *"Improved conversion from phonetic scripts like Pinyin and Kana when typing in Simplified Chinese and Japanese."*
- *"Punctuation suggestions as you type in Chinese."*
- *"Onscreen context for more relevant typing suggestions for Chinese and Japanese."*
- *"QuickPath and typing suggestions for Vietnamese VNI keyboard."*

**Dictation**
- New **systemwide dictation built into the keyboard**, which can correct spelling, punctuation, and capitalization.

**Emoji & stickers**
- *"Faster loading of emoji and sticker keyboards."*
- Emoji keyboard gains a small scroll bar at the bottom.

**New keyboard layouts / languages**
- New layouts: **Slovenian, Estonian**.
- Indigenous languages: **Blackfoot, Comanche, Cree, Kiowa, Tsuut'ina**.
- Additional languages: **Afrikaans, Basque, Baybayin, English (Philippines), Galician, Guarani, Luxembourgish, Xhosa, Zulu**.
- **Châizi** Chinese input method; **Scribble** (Apple Pencil handwriting) expanded to **Hindi and Marathi**.

*(Sources: 9to5Mac "everything new in iOS 27"; MacRumors 250 changes; Tom's Guide; TechRadar.)*

### A.4 iOS Multilingual / Bilingual Keyboard (iOS 18+, carries into 26/27)

- A **single keyboard can combine up to three languages**. Two languages = a **"bilingual"** keyboard; three = a **"multilingual"** keyboard.
- **Three-language** keyboards are limited to specific languages — *"English, Bangla, Gujarati, Hindi, Marathi, Punjabi, Tamil, and Telugu"* — and require **iPhone 12 or later**.
- The system *"intelligently detects the language being used and adjusts suggestions, autocorrect, and predictive text accordingly,"* so the user types across languages **without manual switching**.
- **Setup:** Settings → General → Keyboard → Keyboards → Add New Keyboard → select the additional language → when prompted, choose **"Add to [primary] Keyboard"** → pick a layout (QWERTY, AZERTY, etc.). The combined keyboard is named after both languages (e.g., *"English & Turkish"*), with language indicators shown on the spacebar.
- On upgrade, pre-iOS 18 separate keyboards may be **automatically merged** into bilingual keyboards; users can recreate separate ones via "Add New Keyboard."

*(Sources: macReports — bilingual/multilingual keyboards in iOS 18; Apple Support HT121233 "Use three languages in one keyboard.")*

### A.5 Net Read for FeatherKey

1. The market leader ships the *same class of defect* (missed keys while typing fast) this project exists to eliminate — a credible, verified opening (→ BR-5, BR-6, BR-46).
2. iOS's multilingual bar is **three languages with automatic detection**, higher than the "two" originally assumed (→ BR-16, BR-19a, BR-19b).
3. iOS is moving toward **alternative-word autocorrect** and **built-in dictation** — FeatherKey should match the former outright and match the latter *only* under its privacy guarantees (→ BR-43, BR-45).
4. iOS's advantage is breadth of languages and polish; FeatherKey's counter-advantages are **privacy, security, tiny footprint, and modularity** — none of which iOS markets.

---

## Appendix B — Embedded Android Keyboard Research Notes

> **Purpose:** embeds the raw researched facts about the leading Android keyboards (gathered July 2026) so the competitive analysis in §15.5–15.6 is self-contained and needs no external querying. Captured facts as of July 2026; the AI-feature and account/privacy landscape is moving quickly. Sources are in the list at the end of Section 15.

### B.1 Gboard (Google)

- Market-leading Android keyboard; **free, no ads, no locked features**.
- **2026:** Google integrated **Gemini AI** for smarter suggestions and contextual replies.
- Features: highly accurate swipe typing; **voice input ~97% accuracy** even in noise; built-in **clipboard manager**; Google **Translate row**; theme/size customization.
- **Privacy (the core problem):** Google states it doesn't send what you type except searches and voice input. **Independent research shows more:** Gboard transmits telemetry/metadata — *"which app it was opened in, the length of the typed words (excluding passwords), how long it took to write them, and the languages used"* — plus the phone's **Android ID** (linkable to real identity via Google account). Uses **federated learning**.
- **⚠️ Verification caveat (important before any public use):** the stronger claim — that a Trinity College Dublin man-in-the-middle study **reconstructed original keystrokes** from Gboard's federated-learning updates (*"surprisingly accurate for the most part"*) — comes **only from the Android Police secondary article**. The linked primary PDF (Leith, TCD) **could not be parsed during research, so this claim is NOT confirmed against the source.** The metadata/Android-ID findings above are corroborated by the article text; the keystroke-reconstruction claim must be checked against the primary paper before it is repeated in any public/marketing/legal context, since it names a specific company.
- Telemetry is **on by default**; users can disable "Share usage statistics," "Personalize for you," and "Improve for everyone."
- *(Sources: GetFree.APP; Android Police; Trinity College Dublin study.)*

### B.2 SwiftKey (Microsoft)

- **Best-in-class multilingual:** up to **5 languages simultaneously**, **700+ languages**, real-time automatic code-switching.
- **Strong personal-style learning**; ~**68% next-word prediction accuracy** in testing.
- **2026 AI overhaul:** Microsoft **Copilot** integration — contextual rewriting, professional email templates.
- **Privacy:** learns on-device but **optionally backs up personal dictionary to Microsoft cloud**; from **2026-05-31, login only via a Microsoft account** (Apple/Google sign-in discontinued).
- *(Sources: GetFree.APP; techwench; PhoneArena.)*

### B.3 Samsung Keyboard

- Default on Galaxy devices; **operates mostly offline** in typical use; solid built-in predictions.
- Reported **EFF privacy scorecard ~9/10**; but its "smartest features" can send data to a server, and it is **Galaxy-only** and closed-source.
- *(Sources: Factually privacy rankings; LearnProTips.)*

### B.4 Open-Source / Privacy Keyboards

- **HeliBoard** (OpenBoard/AOSP fork): **100% offline, no internet permission**; good clipboard management; multilingual/multi-layout; adjustable UI; lightweight; on **F-Droid**. Recommended when the threat model is "no keystrokes leaving the device" with open-source auditability.
- **AnySoftKeyboard:** free/open-source; extensive language support; customization; themes; gesture typing; **no cloud/proprietary servers**; offline.
- **FlorisBoard:** privacy-focused, open-source; modular layouts; rich customization; multilingual; clipboard; gestures; **no ads/trackers** (long-running beta maturity).
- **Common trade-off:** all lead on privacy/auditability but generally **trail Gboard/SwiftKey on prediction quality, adaptive learning, and design polish**.
- *(Sources: MakeUseOf; HowToGeek; HeliBoard GitHub; Factually.)*

### B.5 Typewise

- Privacy-first: *"doesn't collect any data you type… All of Typewise's smart autocorrect and text prediction takes place on your device."*
- Known for strong autocorrect that learns the user's patterns, but a **honeycomb/hexagonal layout** that is unconventional and raises the learning curve.
- *(Source: Typewise; Factually.)*

### B.6 Net Read for FeatherKey

The Android field splits cleanly into **"great but not private"** (Gboard, SwiftKey) and **"private but not as polished"** (HeliBoard, AnySoftKeyboard, FlorisBoard), with Samsung (Galaxy-locked) and Typewise (niche layout) as partial exceptions. FeatherKey's opportunity is the **unoccupied intersection**: mainstream-grade accuracy/design **and** verifiable open-source privacy — the compromise no incumbent currently offers (see §15.6).

---

*End of Business Requirements Document (Draft v0.7).*

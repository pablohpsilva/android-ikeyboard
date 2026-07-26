# FeatherKey — Web

The FeatherKey marketing site. A static, self-contained set of pages — no build
step, no framework, no runtime dependencies. Shared look lives in `styles.css`
(tokens, nav, footer, buttons, document/legal layout); each page adds only its own
layout.

## Pages

| File | Status |
|---|---|
| `index.html` | **Landing — built.** |
| `faq.html` | **Built** (from the Claude Design "FeatherKey FAQ" page). |
| `privacy.html` | **Built — DRAFT.** Doubles as the Play Store "privacy policy URL" (`../../PLAY_STORE_PUBLISHING.md`). Content is accurate to the app; **fill the `[contact email]` / `[governing region]` placeholders and review before publishing.** |
| `terms.html` | **Built — DRAFT.** Fill `[repository URL]` / `[licence]` / `[governing region]` / `[contact email]` and review. |

> **Before publishing privacy/terms:** search the folder for `[` to find every
> bracketed placeholder, and have the legal text reviewed. FAQ, privacy, and terms
> were authored/ported here — privacy & terms have **no** design-tool source.

## Preview

It's plain static HTML — open `index.html` in a browser, or serve the folder:

```bash
cd apps/web
python3 -m http.server 8000
# → http://127.0.0.1:8000/index.html
```

## Design source

Implemented from the Claude Design project **"FeatherKey Landing"**
(`FeatherKey Landing.dc.html`) on top of the `eurofi` design system's tokens.
The shipped page strips the design-tool runtime (`x-dc`/`helmet`/`support.js`/
`_ds_bundle.js`) and reproduces the visuals as clean, semantic, responsive CSS.

- **Palette (deliberate):** obsidian `#17131C` canvas, **deep-claret `#7A1F3D`**
  CTAs, **antique-gold `#B8893B`** accents (overlines, hairlines, italic emphasis).
  This matches the landing design's own inline tokens and the design-system
  readme's prose — *not* the design system's current `colors.css`, whose `--brand`
  was migrated to an all-gold palette for its origin product ("EuroFi"). Loading
  that file verbatim would turn the claret CTAs gold with low-contrast text. To
  switch to the gold-brand version, repoint the `:root` color vars in `index.html`.
- **Type:** Cormorant Garamond (display), Schibsted Grotesk (body), IBM Plex Mono
  (numerals).

## Follow-ups

- **Self-host the fonts.** They currently load from the Google Fonts CDN. For a
  privacy product it's on-brand to serve the three OFL families as local `.woff2`
  via `@font-face` so the site makes no third-party requests either.
- **Build `faq.html` / `privacy.html` / `terms.html`** (privacy doubles as the
  Play Store privacy-policy URL).
- **Wire the real store link.** "Get FeatherKey" / "Get it on Android" point to the
  in-page `#get` anchor until the app is published (`applicationId com.featherkey`).
- **Optimize the icon.** `assets/featherkey-icon.png` is the 1254×1254 source
  (~192 KB); a downscaled/`.webp` version + a real favicon would trim page weight.

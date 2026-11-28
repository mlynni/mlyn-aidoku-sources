# Mlyn Aidoku Sources

<sub>English · [Українська](README.uk.md)</sub>

This repository hosts a curated personal list of sources ported from Mihon (`keiyoushi/extensions-source`) and community implementations into Rust/WebAssembly for [Aidoku](https://github.com/Aidoku/Aidoku). It focuses strictly on the 17 extensions I actively use, rather than mirroring hundreds of unused sources.

## Usage

On a device with Aidoku installed (`0.7+`, `0.8.4+` recommended for Comix and MangaDotNet v20, `0.9.0+` for Mangadotnet v10), open [https://mlynni.github.io/mlyn-aidoku-sources/](https://mlynni.github.io/mlyn-aidoku-sources/) and tap the **Add Sources to Aidoku** button to add the list directly to the app.

Otherwise, navigate to **Settings → Source Lists** inside Aidoku and add:

```text
https://mlynni.github.io/mlyn-aidoku-sources/index.min.json
```

Raw mirror (`gh-pages`):

```text
https://raw.githubusercontent.com/mlynni/mlyn-aidoku-sources/gh-pages/index.min.json
```

## Sources (17)

| Source | ID | Version | Languages | Min. Aidoku | Base URL | Notes |
|---|---|---:|---|---:|---|---|
| Atsumaru | `en.atsumaru` | `v12` | `en` | `0.7.1` | https://atsu.moe | Direct JSON API |
| Comix | `en.comix` | `v37` | `en` | `0.8.4` | https://comix.to | Ported Mihon `SiteKey`/`ApicoreKey` direct signed API + `x-enc` v2 decryption + `altHosts` fallback, with WebView SPA harvesters & `waf_pass` fallback |
| Kagane | `multi.kagane` | `v26` | 15 languages | `0.7.1` | https://kagane.moe | Multi-language manhwa & manga |
| MANGA Plus | `multi.mangaplus` | `v6` | 9 languages | `0.7.2` | https://mangaplus.shueisha.co.jp | Official Shueisha releases |
| Mangaball | `multi.mangaball` | `v15` | 34 languages (incl. `uk`) | `0.7.1` | https://mangaball.com | Multi-CDN poster fallback & `.512.jpg` cover optimization |
| MangaBat | `en.mangabat` | `v9` | `en` | `0.7.1` | https://www.mangabats.com | `mangabox` template |
| MangaDex | `multi.mangadex` | `v16` | 54 languages (incl. `uk`, `en`) | `0.7.1` | https://mangadex.org | Public API |
| MangaDotNet | `en.mangadot` | `v20` | 43 languages (incl. `uk`) | `0.8.3` | https://mangadot.net | Single-group chapter deduplication |
| Mangadotnet | `multi.mangadotnet` | `v10` | 43 languages (incl. `uk`) | `0.9.0` | https://mangadot.net | Aidoku 0.9.0+ variant |
| MangaFire | `multi.mangafire` | `v9` | 7 languages | `0.7.1` | https://mangafire.to | Multi-language catalog |
| Mangago | `en.mangago` | `v4` | `en` | `0.7.1` | https://www.mangago.me | 18+ / NSFW |
| MangaKakalot | `en.mangakakalot` | `v6` | `en` | `0.7.1` | https://www.mangakakalot.gg | `mangabox` template |
| MangaNato | `en.manganato` | `v8` | `en` | `0.7.1` | https://www.manganato.gg | `mangabox` template |
| MangaNelo | `en.manganelo` | `v4` | `en` | `0.7.1` | https://www.nelomanga.net | `mangabox` template |
| MangaPill | `en.mangapill` | `v4` | `en` | `0.7.1` | https://mangapill.com | Rust port from Mihon Kotlin source |
| Toonily | `en.toonily` | `v10` | `en` | `0.7.1` | https://toonily.com | `madara` template (18+) |
| Weeb Central | `en.weebcentral` | `v9` | `en` | `0.7.1` | https://weebcentral.com | Full catalog & filters |

> **Comix note:** Rewritten from Mihon's WebView architecture without reverse-engineered API signing, avoiding frequent verification drops. You can switch between `https://comix.to` and `https://comix.ws` in **Comix → Settings → Mirror**; use **Site Check** (`Verify comix.to` / `Verify comix.ws`) only if Comix explicitly asks for verification.

## Maintenance & CLI

A scheduled workflow ([`.github/workflows/sync-upstream.yaml`](.github/workflows/sync-upstream.yaml)) checks Mihon upstream commits every 12 hours strictly for the 8 auto-tracked extensions (`autoSync: true` in [`extensions.json`](extensions.json): Weeb Central, MangaFire, MangaDex, Comix, MangaDotNet, Atsumaru, Mangaball, and Mangago), applies updates via [`scripts/sync-upstream.mjs`](scripts/sync-upstream.mjs), builds `.aix` archives using `aidoku package`, validates schemas & compatibility with [`scripts/build-repo.mjs`](scripts/build-repo.mjs), and deploys `public/` to `gh-pages`.

```bash
# Show all sources and auto-sync watch status
node scripts/mhaid.mjs list

# Port or scaffold a Mihon extension (+ generate .cache/briefs/<id>.md)
node scripts/mhaid.mjs port <extension-name> [--lang <code>] [--watch] [--community]

# Inspect upstream Mihon changes since the pinned commit
node scripts/mhaid.mjs diff <id-or-name>

# Manage which extensions are tracked automatically
node scripts/mhaid.mjs watch <id...>
node scripts/mhaid.mjs unwatch <id...>

# Dry-run or run upstream sync for watched extensions
node scripts/mhaid.mjs sync --dry-run

# Sync built packages from gh-pages and validate manifests, schemas, and .aix archives
node scripts/mhaid.mjs pull-pages
node scripts/mhaid.mjs verify
```

To enable AI-assisted Kotlin/JS → Rust porting in CI when watched upstream extensions change, add a `GEMINI_API_KEY` secret in **Repository Settings → Secrets and variables → Actions**.

## Acknowledgements

This repository builds on the open-source work of:

- **[Aidoku](https://github.com/Aidoku/Aidoku)** & **[aidoku-rs](https://github.com/Aidoku/aidoku-rs)** — iOS/iPadOS reader and Rust WebAssembly source SDK.
- **[Aidoku Community Sources](https://github.com/Aidoku-Community/sources)** — community Rust source crates and shared templates (`mangabox`, `madara`).
- **[Keiyoushi / Mihon Extensions](https://github.com/keiyoushi/extensions-source)** — upstream Kotlin extensions, selectors, and mirror domains tracked for porting and automated sync.

## Disclaimer

This repository does not have any affiliation with the content providers nor the Aidoku application. All names, trademarks, and content belong to their respective owners. If a content provider wishes for a source to be removed from this list, please open an issue.

# Mihon Source Reference: MangaFire

- **Mihon Path:** `src/all/mangafire`
- **Mihon Version Code:** `41`
- **Aidoku Suggested ID:** `multi.mangafire`
- **Base URL:** https://mangafire.to
- **Languages (7):** en, es, es-419, fr, ja, pt, pt-BR
- **Content Rating:** Contains NSFW / Mixed (1)
- **Architecture:** Custom (Filter.CheckBox, Filter.TriState, KeiSource)

## Extracted Kotlin Files
- `src/eu/kanade/tachiyomi/extension/all/mangafire/ChallengeSolverInterceptor.kt`
- `src/eu/kanade/tachiyomi/extension/all/mangafire/Dto.kt`
- `src/eu/kanade/tachiyomi/extension/all/mangafire/Filters.kt`
- `src/eu/kanade/tachiyomi/extension/all/mangafire/MangaFire.kt`
- `src/eu/kanade/tachiyomi/extension/all/mangafire/VrfSigner.kt`

## Extracted Assets
- `assets/solver.html`

## Endpoints & URLs
- `$baseUrl/api/chapters/$chapterId`
- `$baseUrl/api/tags`
- `$baseUrl/api/titles`
- `$baseUrl/api/titles/$hid`
- `$baseUrl/api/titles/$hid/chapters`
- `$baseUrl/api/titles/$hid/volumes`
- `$baseUrl/api/volumes/$last`
- `https://${url.host}`
- `https://${url.host}/@waf/solver`

## Filter Classes
- `AuthorFilter`
- `ContentRatingFilter`
- `GenreFilter`
- `GenreModeFilter`
- `MinChapterFilter`
- `SortFilter`
- `StatusFilter`
- `ThemeFilter`
- `TypeFilter`
- `UriMultiSelectFilter`
- `UriTriSelectFilter`
- `YearFromFilter`
- `YearToFilter`

## Custom Headers
- `Accept: application/json`

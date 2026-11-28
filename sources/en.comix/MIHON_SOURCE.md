# Mihon Source Reference: Comix

- **Mihon Path:** `src/en/comix`
- **Mihon Version Code:** `66`
- **Aidoku Suggested ID:** `en.comix`
- **Base URL:** https://comix.to
- **Languages (1):** en
- **Content Rating:** Contains NSFW / Mixed (1)
- **Architecture:** Custom (Filter.CheckBox, Filter.TriState, KeiSource)

## Extracted Kotlin Files
- `src/eu/kanade/tachiyomi/extension/en/comix/ApicoreKey.kt`
- `src/eu/kanade/tachiyomi/extension/en/comix/Comix.kt`
- `src/eu/kanade/tachiyomi/extension/en/comix/Dto.kt`
- `src/eu/kanade/tachiyomi/extension/en/comix/Filters.kt`
- `src/eu/kanade/tachiyomi/extension/en/comix/SiteKey.kt`
- `src/eu/kanade/tachiyomi/extension/en/comix/SiteKeys.kt`

## Endpoints & URLs
- `$baseUrl/${chapter.url}`
- `$baseUrl/api/v1`
- `$baseUrl/title${manga.url}`
- `/api/v1`

## CSS Selectors
- `script[src*='/dist/main-']`

## Filter Classes
- `ArtistFilter`
- `AuthorFilter`
- `ChapterLengthFilter`
- `ContentRatingFilter`
- `DemographicFilter`
- `Filters`
- `FormatFilter`
- `GenreFilter`
- `MatchModeFilter`
- `SortFilter`
- `StatusFilter`
- `TagsFilter`
- `TermGroupFilter`
- `TypeFilter`
- `UriMultiSelectFilter`
- `UriPartFilter`
- `YearFromFilter`
- `YearToFilter`

## Custom Headers
- `Accept: application/json`
- `X-Requested-With: XMLHttpRequest`
- `X-Rt: 0`

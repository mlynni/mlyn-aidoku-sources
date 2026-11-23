# Mihon Source Reference: Atsumaru

- **Mihon Path:** `src/en/atsumaru`
- **Mihon Version Code:** `28`
- **Aidoku Suggested ID:** `en.atsumaru`
- **Base URL:** https://atsu.moe
- **Languages (1):** en
- **Content Rating:** Contains NSFW / Mixed (1)
- **Architecture:** Custom (Filter.CheckBox, Filter.Group<TagLetterFilter>, Filter.TriState, KeiSource)

## Extracted Kotlin Files
- `src/eu/kanade/tachiyomi/extension/en/atsumaru/Atsumaru.kt`
- `src/eu/kanade/tachiyomi/extension/en/atsumaru/Dto.kt`
- `src/eu/kanade/tachiyomi/extension/en/atsumaru/Filters.kt`

## Endpoints & URLs
- `$baseUrl/api/android/download`
- `$baseUrl/api/explore/availableFilters`
- `$baseUrl/api/home2/$path`
- `$baseUrl/api/manga/allChapters`
- `$baseUrl/api/manga/page`
- `$baseUrl/api/read/chapter`
- `$baseUrl/collections/manga/documents/search`
- `$baseUrl/manga/${it.manga.id}`
- `$baseUrl/manga/${manga.url}`
- `$baseUrl/read/${chapter.url}`
- `https://anilist.co/manga/$it`
- `https://kitsu.app/manga/$it`
- `https://mangabaka.org/$it`
- `https://myanimelist.net/manga/$it`
- `https://www.anime-planet.com/manga/$it`
- `https://www.animenewsnetwork.com/encyclopedia/manga.php?id=$it`
- `https://www.mangaupdates.com/series/$it`

## Filter Classes
- `AdultFilter`
- `FilterData`
- `FilterDto`
- `GenreFilter`
- `MaxChaptersFilter`
- `MinChaptersFilter`
- `OfficialFilter`
- `SortFilter`
- `StatusFilter`
- `TagFilters`
- `TagLetterFilter`
- `TimeframeFilter`
- `TypeFilter`
- `YearFilter`

## Custom Headers
- `releaseYear:=[${years.joinToString(: )}]`

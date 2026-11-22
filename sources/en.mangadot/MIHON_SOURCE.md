# Mihon Source Reference: MangaDot

- **Mihon Path:** `src/all/mangadotnet`
- **Mihon Version Code:** `31`
- **Aidoku Suggested ID:** `multi.mangadotnet`
- **Base URL:** https://mangadot.net
- **Languages (1):** en
- **Content Rating:** Contains NSFW / Mixed (1)
- **Architecture:** Custom (Filter.CheckBox, KeiSource)

## Extracted Kotlin Files
- `src/eu/kanade/tachiyomi/extension/all/mangadotnet/Dto.kt`
- `src/eu/kanade/tachiyomi/extension/all/mangadotnet/Filters.kt`
- `src/eu/kanade/tachiyomi/extension/all/mangadotnet/MangaDot.kt`

## Endpoints & URLs
- `$baseUrl/api/$segment/${chapterUrl.id}/images`
- `$baseUrl/api/discovery/following-reads?limit=30`
- `$baseUrl/api/manga/$mangaId/view`
- `$baseUrl/api/manga/${manga.url}/chapters/list`
- `$baseUrl/api/manga/${manga.url}/volumes`
- `$baseUrl/api/manga/for-you?limit=100`
- `$baseUrl/api/manga/tags`
- `$baseUrl/api/search`
- `$baseUrl/api/search?facets=1`
- `$baseUrl/bookmark.data`
- `$baseUrl/manga/${manga.url}`
- `$baseUrl/manga/${manga.url}.data?_routes=pages/MangaDetailPage`
- `$baseUrl/view-all/$mode.data`
- `$baseUrl/volume/${chapterUrl.id}`
- `https://anilist.co/manga/$it`
- `https://fakeimg.cnbattle.com/400x600/?text=NSFW`
- `https://kitsu.app/manga/$it`
- `https://mangabaka.org/$it`
- `https://mangadex.org/title/$it`
- `https://myanimelist.net/manga/$it`
- `https://shikimori.io/mangas/$it`
- `https://www.anime-planet.com/manga/$it`
- `https://www.animenewsnetwork.com/encyclopedia/manga.php?id=$it`
- `https://www.mangaupdates.com/series/$it`

## Filter Classes
- `ArtistFilter`
- `AuthorFilter`
- `BrowseFilter`
- `ContentRatingFilter`
- `DemographicFilter`
- `FilterDataDto`
- `GenreFilter`
- `LibraryFilter`
- `MaxYearFilter`
- `MinChaptersFilter`
- `MinRatingFilter`
- `MinYearFilter`
- `ScanlatorFilter`
- `SortFilter`
- `StatusFilter`
- `TagFilter`
- `TagsGroupFilter`
- `TriStateFilter`
- `TypeFilter`
- `VolumesFilter`

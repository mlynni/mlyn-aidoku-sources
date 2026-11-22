# Mihon Source Reference: MangaDex

- **Mihon Path:** `src/all/mangadex`
- **Mihon Version Code:** `5`
- **Aidoku Suggested ID:** `multi.mangadex`
- **Base URL:** https://mangadex.org
- **Languages (61):** af, ar, az, be, bg, bn, ca, cs, cv, da, de, el, en, eo, es, es-419, et, eu, fa, fi, fil, fr, ga, he, hi, hr, hu, id, it, ja, jv, ka, kk, ko, la, lt, mn, ms, my, ne, nl, no, pl, pt, pt-BR, ro, ru, sk, sq, sr, sv, ta, te, th, tr, uk, ur, uz, vi, zh-Hans, zh-Hant
- **Content Rating:** Contains NSFW / Mixed (1)
- **Architecture:** Custom (AttributesDto, EntityDto, Filter.CheckBox, Filter.Group<ContentRating>, Filter.Group<Demographic>, Filter.Group<OriginalLanguage>, Filter.Group<Status>, Filter.Group<Tag>, Filter.Select<TagMode>, Filter.TriState, KeiSource)

## Extracted Kotlin Files
- `src/eu/kanade/tachiyomi/extension/all/mangadex/MDConstants.kt`
- `src/eu/kanade/tachiyomi/extension/all/mangadex/MangaDex.kt`
- `src/eu/kanade/tachiyomi/extension/all/mangadex/MangaDexFilters.kt`
- `src/eu/kanade/tachiyomi/extension/all/mangadex/MangaDexHelper.kt`
- `src/eu/kanade/tachiyomi/extension/all/mangadex/MangaDexIntl.kt`
- `src/eu/kanade/tachiyomi/extension/all/mangadex/dto/AggregateDto.kt`
- `src/eu/kanade/tachiyomi/extension/all/mangadex/dto/AtHomeDto.kt`
- `src/eu/kanade/tachiyomi/extension/all/mangadex/dto/AuthorDto.kt`
- `src/eu/kanade/tachiyomi/extension/all/mangadex/dto/ChapterDto.kt`
- `src/eu/kanade/tachiyomi/extension/all/mangadex/dto/CoverArtDto.kt`
- `src/eu/kanade/tachiyomi/extension/all/mangadex/dto/EntityDto.kt`
- `src/eu/kanade/tachiyomi/extension/all/mangadex/dto/ListDto.kt`
- `src/eu/kanade/tachiyomi/extension/all/mangadex/dto/MangaDto.kt`
- `src/eu/kanade/tachiyomi/extension/all/mangadex/dto/ResponseDto.kt`
- `src/eu/kanade/tachiyomi/extension/all/mangadex/dto/ScanlationGroupDto.kt`
- `src/eu/kanade/tachiyomi/extension/all/mangadex/dto/UserDto.kt`

## Extracted Assets
- `assets/i18n/messages_en.properties`
- `assets/i18n/messages_es.properties`
- `assets/i18n/messages_pt_br.properties`
- `assets/i18n/messages_ru.properties`

## Endpoints & URLs
- `https://api.mangadex.org`
- `https://uploads.mangadex.org`

## Filter Classes
- `DaysSinceFilter`
- `HasAvailableChaptersFilter`
- `MangaDexFilters`
- `SortFilter`
- `TagsFilter`

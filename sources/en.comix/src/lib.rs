#![no_std]
use aidoku::{
	BaseUrlProvider, Chapter, CoverImageProcessor, DeepLinkHandler, DeepLinkResult, FilterValue,
	HashMap, Home, HomeComponent, HomeComponentValue, HomeLayout, HomePartialResult,
	ImageRequestProvider, ImageResponse, Link, LinkValue, Listing, ListingProvider, Manga,
	MangaPageResult, MangaWithChapter, NotificationHandler, Page, PageContent, PageContext,
	PageImageProcessor, Result, Source, WebLoginHandler,
	alloc::{String, Vec, format, string::ToString, vec},
	helpers::uri::QueryParameters,
	imports::{
		canvas::ImageRef,
		net::Request,
		std::send_partial_result,
	},
	prelude::*,
};
use core::cell::RefCell;

mod descrambler;
mod helpers;
mod models;
mod settings;
mod site_key;
mod web;

use models::*;
use settings::{ALT_MIRROR, DEFAULT_MIRROR, VERIFY_KEY, VERIFY_WS_KEY};

struct Comix;

const SORT_VALUES: &[&str] = &[
	"relevance",
	"chapter_updated_at",
	"created_at",
	"title",
	"year",
	"score",
	"views_7d",
	"views_30d",
	"views_90d",
	"views_total",
	"follows_total",
];

fn parse_chapter_list(
	chapters_res: ChaptersHarvestResult,
	base_url: &str,
) -> Vec<Chapter> {
	let deduplicate = settings::dedup_chapters();
	let blacklist = settings::scanlator_blacklist();

	let filtered = chapters_res.chapters.into_iter().filter(|ch| {
		if blacklist.is_empty() {
			return true;
		}
		!ch.group.as_ref().is_some_and(|g| {
			let name_lower = g.name.trim().to_ascii_lowercase();
			let id_str = g.id.map(|id| id.to_string()).unwrap_or_default();
			blacklist
				.iter()
				.any(|b| b == &name_lower || (!id_str.is_empty() && b == &id_str))
		})
	});

	let selected: Vec<HarvestedChapter> = if deduplicate {
		let mut map: HashMap<String, HarvestedChapter> = HashMap::new();
		for item in filtered {
			helpers::dedup_insert(&mut map, item);
		}
		let mut list: Vec<HarvestedChapter> = map.into_values().collect();
		list.sort_by(|a, b| {
			b.number
				.partial_cmp(&a.number)
				.unwrap_or(core::cmp::Ordering::Equal)
				.then_with(|| b.id.cmp(&a.id))
		});
		list
	} else {
		filtered.collect()
	};

	selected
		.into_iter()
		.map(|ch| {
			let date_uploaded = helpers::extract_relative_date(&ch.text);
			let scanlators = ch.group.map(|g| vec![g.name]);
			let url = helpers::resolve_url(base_url, &ch.url);
			Chapter {
				key: ch.id.to_string(),
				title: ch.name.filter(|s| !s.trim().is_empty()),
				chapter_number: Some(ch.number),
				volume_number: ch.volume,
				date_uploaded,
				scanlators,
				url: Some(url),
				..Default::default()
			}
		})
		.collect()
}

fn find_detail_manga(data: &InitialData, hid: Option<&str>) -> Option<ComixManga> {
	if let Some(target_hid) = hid
		&& let Some((_, val)) = data
			.queries
			.iter()
			.find(|(k, _)| k.contains("\"detail\"") && k.contains(target_hid))
		&& let Ok(manga) = serde_json::from_value::<ComixManga>(val.clone())
	{
		return Some(manga);
	}
	data.queries
		.iter()
		.find(|(k, _)| k.contains("\"detail\""))
		.and_then(|(_, val)| serde_json::from_value::<ComixManga>(val.clone()).ok())
}

fn extract_home_list(data: &InitialData, needle: &str) -> Vec<ComixManga> {
	let Some((_, val)) = data.queries.iter().find(|(k, _)| k.contains(needle)) else {
		return Vec::new();
	};
	if val.is_array() {
		serde_json::from_value::<Vec<ComixManga>>(val.clone()).unwrap_or_default()
	} else {
		serde_json::from_value::<InitialListPayload>(val.clone())
			.map(|p| p.items)
			.unwrap_or_default()
	}
}

fn push_param(params: &mut Vec<(String, String)>, key: &str, val: &str) {
	params.push((key.to_string(), val.to_string()));
}

fn fetch_manga_list_api(
	base_url: &str,
	api_params: &[(String, String)],
) -> core::result::Result<MangaPageResult, web::ApiError> {
	let res: ApiResponse<MangaItems> =
		web::call_signed_api(base_url, "/manga", api_params, "/browse")?;
	let entries = res
		.result
		.items
		.into_iter()
		.map(|m| m.into_manga(base_url, false))
		.collect();
	Ok(MangaPageResult {
		entries,
		has_next_page: res.result.meta.page < res.result.meta.last_page,
	})
}

fn fetch_chapters_api(
	base_url: &str,
	hid: &str,
	referer_path: &str,
) -> core::result::Result<Vec<HarvestedChapter>, web::ApiError> {
	let api_path = format!("/manga/{hid}/chapters");
	let mut all: Vec<HarvestedChapter> = Vec::new();
	let mut page = 1;
	loop {
		let params = vec![
			("page".into(), page.to_string()),
			("limit".into(), "100".into()),
			("order[number]".into(), "desc".into()),
			("v".into(), "0".into()),
			("w".into(), "0".into()),
		];
		let res: ApiResponse<ChapterItems> =
			web::call_signed_api(base_url, &api_path, &params, referer_path)?;
		let last_page = res.result.meta.last_page;
		if res.result.items.is_empty() {
			break;
		}
		all.extend(res.result.items.into_iter().map(ApiChapter::into_harvested));
		if page >= last_page {
			break;
		}
		page += 1;
	}
	Ok(all)
}

fn extract_url_host(url: &str) -> Option<&str> {
	let rest = url
		.strip_prefix("https://")
		.or_else(|| url.strip_prefix("http://"))?;
	Some(rest.split(['/', '?', '#']).next().unwrap_or(rest))
}

fn browse_with_defaults(
	sort: &str,
	extra_types: Option<&str>,
	page: i32,
) -> Result<MangaPageResult> {
	let base_url = settings::base_url();
	let mut qs = QueryParameters::new();
	let mut api_params: Vec<(String, String)> = Vec::new();

	qs.push("sort", Some(sort));
	qs.push("page", Some(&page.to_string()));
	let (sort_field, sort_dir) = sort.split_once(':').unwrap_or((sort, "desc"));
	push_param(&mut api_params, &format!("order[{sort_field}]"), sort_dir);
	push_param(&mut api_params, "page", &page.to_string());
	push_param(&mut api_params, "limit", "28");

	let rating = settings::content_rating();
	let ratings = settings::content_ratings_up_to(&rating);
	qs.push("content_rating", Some(&ratings.join(",")));
	for r in ratings {
		push_param(&mut api_params, "content_rating[]", r);
	}

	if let Some(types) = extra_types {
		qs.push("types", Some(types));
		for t in types.split(',').map(str::trim).filter(|s| !s.is_empty()) {
			push_param(&mut api_params, "types[]", t);
		}
	} else {
		let types = settings::default_types();
		if !types.is_empty() && types.len() < 4 {
			qs.push("types", Some(&types.join(",")));
			for t in types {
				push_param(&mut api_params, "types[]", &t);
			}
		}
	}

	let demos = settings::default_demographics();
	let mut excluded_genres: Vec<String> = settings::blocked_genres();
	for d in ["3", "4", "1", "2"] {
		if !demos.iter().any(|s| s == d) && !excluded_genres.iter().any(|s| s == d) {
			excluded_genres.push(d.into());
		}
	}
	if !excluded_genres.is_empty() {
		qs.push("genres_ex", Some(&excluded_genres.join(",")));
		for ex in excluded_genres {
			push_param(&mut api_params, "genres_ex[]", &ex);
		}
	}

	if let Ok(page_res) = fetch_manga_list_api(&base_url, &api_params) {
		return Ok(page_res);
	}

	let url = format!("{base_url}/browse?{qs}");
	harvest_and_map_browse(&base_url, &url)
}

fn harvest_and_map_browse(base_url: &str, browse_url: &str) -> Result<MangaPageResult> {
	let res = web::harvest_browse(browse_url)?;
	let quality = settings::thumbnail_quality();
	let per_page = res.per_page.unwrap_or(28);
	let count = res.items.len();

	let entries = res
		.items
		.into_iter()
		.map(|item| {
			let cover = item
				.cover
				.as_deref()
				.filter(|s| !s.trim().is_empty())
				.map(|raw| helpers::apply_cover_quality(&helpers::resolve_url(base_url, raw), &quality));
			let url = helpers::resolve_url(base_url, &item.url);
			Manga {
				key: item.hid,
				title: item.title,
				cover,
				url: Some(url),
				..Default::default()
			}
		})
		.collect();

	Ok(MangaPageResult {
		entries,
		has_next_page: (count as i32) >= per_page,
	})
}

impl Source for Comix {
	fn new() -> Self {
		Self
	}

	fn get_search_manga_list(
		&self,
		query: Option<String>,
		page: i32,
		filters: Vec<FilterValue>,
	) -> Result<MangaPageResult> {
		let base_url = settings::base_url();
		let query_text = query
			.as_deref()
			.map(str::trim)
			.filter(|s| !s.is_empty())
			.map(String::from);

		let mut qs = QueryParameters::new();

		let mut sort_param: Option<String> = None;
		let mut content_rating_override: Option<String> = None;
		let mut selected_types: Option<Vec<String>> = None;
		let mut genres_in: Vec<String> = Vec::new();
		let mut genres_ex: Vec<String> = settings::blocked_genres();
		let mut demos_in: Vec<String> = Vec::new();
		let mut excluded_demos: Vec<String> = Vec::new();
		let mut statuses: Vec<String> = Vec::new();
		let mut genres_mode: Option<String> = None;
		let mut min_chap: Option<String> = None;
		let mut max_chap: Option<String> = None;
		let mut year_from: Option<String> = None;
		let mut year_to: Option<String> = None;
		let mut author_ids: Vec<String> = Vec::new();
		let mut artist_ids: Vec<String> = Vec::new();

		let default_demos = settings::default_demographics();
		let mut demo_filter_seen = false;

		for filter in filters {
			match filter {
				FilterValue::Sort {
					index,
					ascending,
					..
				} => {
					let selected = SORT_VALUES
						.get(index as usize)
						.copied()
						.unwrap_or("relevance");
					let field = if selected == "relevance" && query_text.is_none() {
						"chapter_updated_at"
					} else {
						selected
					};
					let dir = if ascending { "asc" } else { "desc" };
					sort_param = Some(format!("{field}:{dir}"));
				}
				FilterValue::Select { id, value } => match id.as_str() {
					"content_rating" => {
						if value != "default" {
							content_rating_override = Some(value);
						}
					}
					"genres_mode" => {
						if !value.is_empty() {
							genres_mode = Some(value);
						}
					}
					"year_from" => {
						if !value.is_empty() {
							year_from = Some(value);
						}
					}
					"year_to" => {
						if !value.is_empty() {
							year_to = Some(value);
						}
					}
					_ => {}
				},
				FilterValue::MultiSelect {
					id,
					included,
					excluded,
				} => match id.as_str() {
					"types" => {
						if !included.is_empty() {
							selected_types = Some(included);
						}
					}
					"genres" | "formats" => {
						for v in included {
							if let Some(pos) = genres_ex.iter().position(|x| x == &v) {
								genres_ex.swap_remove(pos);
							}
							if !genres_in.contains(&v) {
								genres_in.push(v);
							}
						}
						for v in excluded {
							if !genres_ex.contains(&v) {
								genres_ex.push(v);
							}
						}
					}
					"demos" => {
						demo_filter_seen = true;
						for v in included {
							if !demos_in.contains(&v) {
								demos_in.push(v);
							}
						}
						for v in excluded {
							if !excluded_demos.contains(&v) {
								excluded_demos.push(v);
							}
						}
					}
					"statuses" => {
						for v in included {
							if !statuses.contains(&v) {
								statuses.push(v);
							}
						}
					}
					_ => {}
				},
				FilterValue::Text { id, value } => {
					let trimmed = value.trim();
					if trimmed.is_empty() {
						continue;
					}
					match id.as_str() {
						"tags" => {
							for tid in web::fetch_tag_ids(&base_url, "tag", trimmed) {
								if !genres_in.contains(&tid) {
									genres_in.push(tid);
								}
							}
						}
						"author" => {
							for aid in web::fetch_tag_ids(&base_url, "author", trimmed) {
								if !author_ids.contains(&aid) {
									author_ids.push(aid);
								}
							}
						}
						"artist" => {
							for aid in web::fetch_tag_ids(&base_url, "artist", trimmed) {
								if !artist_ids.contains(&aid) {
									artist_ids.push(aid);
								}
							}
						}
						"min_chap" => {
							if let Ok(v) = trimmed.parse::<i32>()
								&& v > 0
							{
								min_chap = Some(v.to_string());
							}
						}
						"max_chap" => {
							if let Ok(v) = trimmed.parse::<i32>()
								&& v > 0
							{
								max_chap = Some(v.to_string());
							}
						}
						_ => {}
					}
				}
				_ => {}
			}
		}

		let mut api_params: Vec<(String, String)> = Vec::new();
		if let Some(ref q) = query_text {
			qs.push("q", Some(q));
			push_param(&mut api_params, "keyword", q);
		}
		qs.push("page", Some(&page.to_string()));
		push_param(&mut api_params, "page", &page.to_string());
		push_param(&mut api_params, "limit", "28");

		let sort_final = sort_param.unwrap_or_else(|| {
			if query_text.is_some() {
				"relevance:desc".into()
			} else {
				"chapter_updated_at:desc".into()
			}
		});
		qs.push("sort", Some(&sort_final));
		let (sort_field, sort_dir) = sort_final
			.split_once(':')
			.unwrap_or((&sort_final, "desc"));
		push_param(&mut api_params, &format!("order[{sort_field}]"), sort_dir);

		let rating_str = match content_rating_override.as_deref() {
			Some(val) => val.to_string(),
			None => settings::content_rating(),
		};
		let ratings = settings::content_ratings_up_to(&rating_str);
		qs.push("content_rating", Some(&ratings.join(",")));
		for r in ratings {
			push_param(&mut api_params, "content_rating[]", r);
		}

		let types_to_apply = selected_types.unwrap_or_else(settings::default_types);
		if !types_to_apply.is_empty() && types_to_apply.len() < 4 {
			qs.push("types", Some(&types_to_apply.join(",")));
			for t in types_to_apply {
				push_param(&mut api_params, "types[]", &t);
			}
		}

		if !demo_filter_seen {
			for d in ["3", "4", "1", "2"] {
				if !default_demos.iter().any(|s| s == d) && !excluded_demos.iter().any(|s| s == d) {
					excluded_demos.push(d.into());
				}
			}
		}
		if !demos_in.is_empty() {
			qs.push("demos", Some(&demos_in.join(",")));
			for d in demos_in {
				push_param(&mut api_params, "demographics[]", &d);
			}
		}
		for ex_d in excluded_demos {
			if !genres_ex.contains(&ex_d) {
				genres_ex.push(ex_d);
			}
		}

		if !genres_in.is_empty() {
			qs.push("genres_in", Some(&genres_in.join(",")));
			for g in &genres_in {
				push_param(&mut api_params, "genres_in[]", g);
			}
			if let Some(mode) = genres_mode {
				qs.push("genres_mode", Some(&mode));
				push_param(&mut api_params, "genres_mode", &mode);
			}
		}
		if !genres_ex.is_empty() {
			qs.push("genres_ex", Some(&genres_ex.join(",")));
			for g in &genres_ex {
				push_param(&mut api_params, "genres_ex[]", g);
			}
		}
		if !statuses.is_empty() {
			qs.push("statuses", Some(&statuses.join(",")));
			for st in &statuses {
				push_param(&mut api_params, "statuses[]", st);
			}
		}
		if let Some(v) = min_chap {
			qs.push("min_chap", Some(&v));
			push_param(&mut api_params, "min_chap", &v);
		}
		if let Some(v) = max_chap {
			qs.push("max_chap", Some(&v));
			push_param(&mut api_params, "max_chap", &v);
		}
		if let Some(v) = year_from {
			qs.push("year_from", Some(&v));
			push_param(&mut api_params, "year_from", &v);
		}
		if let Some(v) = year_to {
			qs.push("year_to", Some(&v));
			push_param(&mut api_params, "year_to", &v);
		}
		if !author_ids.is_empty() {
			qs.push("authors", Some(&author_ids.join(",")));
			for a in &author_ids {
				push_param(&mut api_params, "authors[]", a);
			}
		}
		if !artist_ids.is_empty() {
			qs.push("artists", Some(&artist_ids.join(",")));
			for a in &artist_ids {
				push_param(&mut api_params, "artists[]", a);
			}
		}

		if let Ok(page_res) = fetch_manga_list_api(&base_url, &api_params) {
			return Ok(page_res);
		}

		let url = format!("{base_url}/browse?{qs}");
		harvest_and_map_browse(&base_url, &url)
	}

	fn get_manga_update(
		&self,
		mut manga: Manga,
		needs_details: bool,
		needs_chapters: bool,
	) -> Result<Manga> {
		if !needs_details && !needs_chapters {
			return Ok(manga);
		}

		let base_url = settings::base_url();
		let title_path = manga
			.url
			.as_deref()
			.map(helpers::extract_path)
			.filter(|p| p.starts_with("/title/") && p.len() > 7)
			.unwrap_or_else(|| format!("/title/{}", manga.key));
		let title_url = helpers::resolve_url(&base_url, &title_path);

		let mut details_done = !needs_details;
		if needs_details {
			let detail_path = format!("/manga/{}", manga.key);
			if let Ok(res) =
				web::call_signed_api::<ApiResponse<ComixManga>>(&base_url, &detail_path, &[], &title_path)
			{
				let detailed = res.result.into_manga(&base_url, true);
				manga.copy_from(detailed);
				if needs_chapters {
					send_partial_result(&manga);
				}
				details_done = true;
			}
		}

		let mut chapters_done = !needs_chapters;
		if needs_chapters {
			if let Ok(chapters) = fetch_chapters_api(&base_url, &manga.key, &title_path) {
				let res = ChaptersHarvestResult {
					hid: Some(manga.key.clone()),
					count: chapters.len(),
					complete: true,
					chapters,
				};
				manga.chapters = Some(parse_chapter_list(res, &base_url));
				chapters_done = true;
			}
		}

		if details_done && chapters_done {
			return Ok(manga);
		}

		let manga_cell = RefCell::new(manga);
		let (_, chapters_opt) = web::harvest_title_page(
			&title_url,
			!details_done,
			!chapters_done,
			|initial_data| {
				let hid = manga_cell.borrow().key.clone();
				if let Some(comix_manga) = find_detail_manga(initial_data, Some(&hid)) {
					let detailed = comix_manga.into_manga(&base_url, true);
					let mut m = manga_cell.borrow_mut();
					m.copy_from(detailed);
					if !chapters_done {
						send_partial_result(&*m);
					}
				}
			},
		)?;

		let mut manga = manga_cell.into_inner();
		if !chapters_done && let Some(chapters_res) = chapters_opt {
			manga.chapters = Some(parse_chapter_list(chapters_res, &base_url));
		}

		Ok(manga)
	}

	fn get_page_list(&self, manga: Manga, chapter: Chapter) -> Result<Vec<Page>> {
		let base_url = settings::base_url();

		let valid_chapter_path = chapter
			.url
			.as_deref()
			.map(helpers::extract_path)
			.filter(|path| {
				if let Some(rest) = path.strip_prefix("/title/")
					&& let Some((manga_seg, ch_seg)) = rest.split_once('/')
				{
					manga_seg.contains('-') && !ch_seg.is_empty() && path.contains("-chapter-")
				} else {
					false
				}
			});

		let referer_path = valid_chapter_path
			.clone()
			.unwrap_or_else(|| format!("/title/{}", manga.key));

		if let Ok(res) = web::call_signed_api::<ApiResponse<ChapterPagesResult>>(
			&base_url,
			&format!("/chapters/{}", chapter.key),
			&[("v".into(), "0".into()), ("w".into(), "0".into())],
			&referer_path,
		) && !res.result.pages.items.is_empty()
		{
			let pages_dto = res.result.pages;
			return Ok(pages_dto
				.items
				.iter()
				.enumerate()
				.map(|(index, p)| {
					let mut context = PageContext::new();
					context.insert("url".into(), p.url.clone());
					if let Some(host) = extract_url_host(&p.url) {
						let alt_hosts = pages_dto.alt_hosts(host, index);
						if !alt_hosts.is_empty() {
							context.insert("alt_hosts".into(), alt_hosts.join(","));
						}
					}
					Page {
						content: PageContent::url_context(p.url.clone(), context),
						..Default::default()
					}
				})
				.collect());
		}

		let chapter_url = if let Some(path) = valid_chapter_path {
			helpers::resolve_url(&base_url, &path)
		} else {
			let mut slug_path = manga
				.url
				.as_deref()
				.map(helpers::extract_path)
				.filter(|p| p.starts_with("/title/") && p.contains('-'))
				.unwrap_or_default();

			if slug_path.is_empty() {
				let lookup_url = format!("{base_url}/title/{}", manga.key);
				if let Ok(initial) = web::fetch_initial_data(&lookup_url)
					&& let Some(detail) = find_detail_manga(&initial, Some(&manga.key))
					&& let Some(u) = detail.url
				{
					slug_path = helpers::extract_path(&u);
				}
			}
			if slug_path.is_empty() {
				slug_path = format!("/title/{}", manga.key);
			}

			let num_str = helpers::format_chapter_num(chapter.chapter_number.unwrap_or(0.0));
			helpers::resolve_url(
				&base_url,
				&format!("{}/{}-chapter-{}", slug_path.trim_end_matches('/'), chapter.key, num_str),
			)
		};

		let harvested = web::harvest_chapter_pages(&chapter_url)?;
		if harvested.pages.is_empty() {
			bail!("No pages found for this chapter");
		}

		Ok(harvested
			.pages
			.into_iter()
			.map(|p| {
				let mut context = PageContext::new();
				context.insert("url".into(), p.url.clone());
				Page {
					content: PageContent::url_context(p.url, context),
					..Default::default()
				}
			})
			.collect())
	}
}

impl ImageRequestProvider for Comix {
	fn get_image_request(&self, url: String, context: Option<PageContext>) -> Result<Request> {
		let _ = context;
		// Comix image domains reject requests that send Referer / Origin headers.
		Ok(Request::get(url)?)
	}
}

impl CoverImageProcessor for Comix {
	fn process_cover_image(&self, response: ImageResponse) -> Result<ImageRef> {
		if response.code < 400 && response.image.width() > 0.0 {
			return Ok(response.image);
		}
		if let Some(orig_url) = response.request.url.as_deref() {
			let mut candidates: Vec<String> = Vec::new();
			if orig_url.contains("@280.") {
				candidates.push(orig_url.replace("@280.", "."));
			} else if orig_url.ends_with(".jpg") {
				candidates.push(orig_url.replacen(".jpg", "@280.jpg", 1));
			}
			if let Some(alt) = settings::alternate_mirror(orig_url) {
				candidates.push(alt.clone());
				if alt.contains("@280.") {
					candidates.push(alt.replace("@280.", "."));
				}
			}
			for candidate in candidates {
				if let Ok(req) = Request::get(&candidate)
					&& let Ok(resp) = req.send()
					&& resp.status_code() < 400
					&& let Ok(bytes) = resp.get_data()
					&& !bytes.is_empty()
				{
					return Ok(ImageRef::new(&bytes));
				}
			}
		}
		Ok(response.image)
	}
}

impl PageImageProcessor for Comix {
	fn process_page_image(
		&self,
		response: ImageResponse,
		context: Option<PageContext>,
	) -> Result<ImageRef> {
		descrambler::process_page_image(response, context)
	}
}

impl Home for Comix {
	fn get_home(&self) -> Result<HomeLayout> {
		send_partial_result(&HomePartialResult::Layout(HomeLayout {
			components: vec![
				HomeComponent {
					title: Some("Trending".into()),
					subtitle: None,
					value: HomeComponentValue::empty_scroller(),
				},
				HomeComponent {
					title: Some("Most Followed".into()),
					subtitle: None,
					value: HomeComponentValue::empty_scroller(),
				},
				HomeComponent {
					title: Some("Latest Updates".into()),
					subtitle: None,
					value: HomeComponentValue::empty_scroller(),
				},
				HomeComponent {
					title: Some("Recently Added".into()),
					subtitle: None,
					value: HomeComponentValue::empty_manga_chapter_list(),
				},
			],
		}));

		let base_url = settings::base_url();
		let rating = settings::content_rating();
		let allowed_ratings = settings::content_ratings_up_to(&rating);
		let allowed_types = settings::default_types();
		let blocked_genres = settings::blocked_genres();

		let data = web::fetch_initial_data(&format!("{base_url}/home"))?;

		let sections = [
			("\"trending\"", "Trending"),
			("\"follows\"", "Most Followed"),
			("\"scope\":\"hot\"", "Latest Updates"),
		];

		for (needle, title) in sections {
			let entries: Vec<Link> = extract_home_list(&data, needle)
				.into_iter()
				.filter(|m| m.matches_settings(&allowed_ratings, &allowed_types, &blocked_genres))
				.take(20)
				.map(|m| {
					let manga = m.into_manga(&base_url, false);
					Link {
						title: manga.title.clone(),
						subtitle: None,
						image_url: manga.cover.clone(),
						value: Some(LinkValue::Manga(manga)),
					}
				})
				.collect();

			send_partial_result(&HomePartialResult::Component(HomeComponent {
				title: Some(title.into()),
				subtitle: None,
				value: HomeComponentValue::Scroller {
					entries,
					listing: Some(Listing {
						id: title.into(),
						name: title.into(),
						..Default::default()
					}),
				},
			}));
		}

		{
			let recent_items = extract_home_list(&data, "\"created_at\"");
			let entries: Vec<MangaWithChapter> = recent_items
				.into_iter()
				.filter(|m| m.matches_settings(&allowed_ratings, &allowed_types, &blocked_genres))
				.take(15)
				.map(|m| {
					let latest_ch = m.latest_chapter;
					let manga = m.into_manga(&base_url, false);
					MangaWithChapter {
						manga,
						chapter: Chapter {
							chapter_number: latest_ch,
							..Default::default()
						},
					}
				})
				.collect();

			let title = "Recently Added";
			send_partial_result(&HomePartialResult::Component(HomeComponent {
				title: Some(title.into()),
				subtitle: None,
				value: HomeComponentValue::MangaChapterList {
					page_size: None,
					entries,
					listing: Some(Listing {
						id: title.into(),
						name: title.into(),
						..Default::default()
					}),
				},
			}));
		}

		Ok(HomeLayout::default())
	}
}

impl ListingProvider for Comix {
	fn get_manga_list(&self, listing: Listing, page: i32) -> Result<MangaPageResult> {
		match listing.id.as_str() {
			"Popular" => browse_with_defaults("score:desc", None, page),
			"Latest Updates" | "Latest Updates (Hot)" => {
				browse_with_defaults("chapter_updated_at:desc", None, page)
			}
			"Trending" | "Most Recent Popular" => browse_with_defaults("views_7d:desc", None, page),
			"Most Followed" | "Most Follows New Comics" => {
				browse_with_defaults("follows_total:desc", None, page)
			}
			"Recently Added" => browse_with_defaults("created_at:desc", None, page),
			"Trending Webtoon" => browse_with_defaults("views_30d:desc", Some("manhua,manhwa"), page),
			"Trending Manga" => browse_with_defaults("views_30d:desc", Some("manga"), page),
			_ => bail!("Unknown listing: {}", listing.id),
		}
	}
}

impl BaseUrlProvider for Comix {
	fn get_base_url(&self) -> Result<String> {
		Ok(settings::base_url())
	}
}

impl NotificationHandler for Comix {
	fn handle_notification(&self, notification: String) {
		if notification == "resetFilters" {
			settings::reset_filters();
		}
	}
}

impl DeepLinkHandler for Comix {
	fn handle_deep_link(&self, url: String) -> Result<Option<DeepLinkResult>> {
		let path = [
			"https://comix.to/",
			"https://www.comix.to/",
			"https://comix.ws/",
			"https://www.comix.ws/",
		]
		.iter()
		.find_map(|prefix| url.strip_prefix(prefix));

		let Some(path) = path else {
			return Ok(None);
		};

		let mut segments = path.split('/');
		if let (Some("title"), Some(manga_segment)) = (segments.next(), segments.next()) {
			let manga_key = manga_segment.split('-').next().unwrap_or(manga_segment);
			if manga_key.is_empty() {
				return Ok(None);
			}
			if let Some(chapter_segment) = segments.next() {
				let chapter_key = chapter_segment.split('-').next().unwrap_or("");
				if !chapter_key.is_empty() {
					return Ok(Some(DeepLinkResult::Chapter {
						manga_key: manga_key.to_string(),
						key: chapter_key.to_string(),
					}));
				}
			}
			return Ok(Some(DeepLinkResult::Manga {
				key: manga_key.to_string(),
			}));
		}

		Ok(None)
	}
}

impl WebLoginHandler for Comix {
	fn handle_web_login(&self, key: String, cookies: HashMap<String, String>) -> Result<bool> {
		if key == VERIFY_KEY || key == VERIFY_WS_KEY {
			let mirror_url = if key == VERIFY_WS_KEY {
				ALT_MIRROR
			} else {
				DEFAULT_MIRROR
			};
			if let Some((_, val)) = cookies
				.iter()
				.find(|(name, v)| name.eq_ignore_ascii_case("waf_pass") && !v.trim().is_empty())
			{
				settings::set_waf_pass_for(mirror_url, val);
			}
			// The pass cookie is named `waf_pass`; accept any WAF/Cloudflare clearance cookie
			// so the check can't get stuck if the site renames it.
			let passed = cookies.iter().any(|(name, value)| {
				let name = name.to_ascii_lowercase();
				!value.trim().is_empty()
					&& (name.contains("waf") || name == "cf_clearance")
			});
			return Ok(passed);
		}
		Ok(false)
	}
}

register_source!(
	Comix,
	Home,
	ListingProvider,
	BaseUrlProvider,
	ImageRequestProvider,
	CoverImageProcessor,
	PageImageProcessor,
	NotificationHandler,
	DeepLinkHandler,
	WebLoginHandler
);

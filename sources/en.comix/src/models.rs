use crate::{helpers, settings};
use aidoku::{
	ContentRating, HashMap, Manga, MangaStatus, Viewer,
	alloc::{String, Vec, format, string::ToString},
};
use serde::{Deserialize, Deserializer, de};

fn null_default<'de, D, T>(deserializer: D) -> core::result::Result<T, D::Error>
where
	D: Deserializer<'de>,
	T: Default + Deserialize<'de>,
{
	Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

#[derive(Deserialize)]
pub struct InitialData {
	pub page: Option<String>,
	#[serde(default)]
	pub queries: HashMap<String, serde_json::Value>,
}

#[derive(Deserialize)]
pub struct InitialListPayload {
	#[serde(default)]
	pub items: Vec<ComixManga>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComixManga {
	pub hid: String,
	pub title: String,
	#[serde(default, alias = "alt_titles", deserialize_with = "null_default")]
	pub alt_titles: Vec<String>,
	pub synopsis: Option<String>,
	#[serde(default, deserialize_with = "null_default")]
	pub r#type: String,
	#[serde(alias = "cover", alias = "thumbnail", alias = "thumb")]
	pub poster: Option<Poster>,
	#[serde(default, deserialize_with = "null_default")]
	pub status: String,
	#[serde(default, alias = "content_rating", deserialize_with = "null_default")]
	pub content_rating: ComixContentRating,
	#[serde(default, alias = "original_language")]
	pub original_language: Option<String>,
	#[serde(alias = "latest_chapter")]
	pub latest_chapter: Option<f32>,
	pub year: Option<i32>,
	pub rank: Option<i32>,
	#[serde(alias = "follows_total")]
	pub follows_total: Option<i64>,
	#[serde(alias = "rated_avg")]
	pub rated_avg: Option<f64>,
	#[serde(alias = "rated_count")]
	pub rated_count: Option<i64>,
	pub links: Option<HashMap<String, serde_json::Value>>,
	pub url: Option<String>,
	#[serde(alias = "genre")]
	pub genres: Option<Vec<Term>>,
	#[serde(alias = "demographic")]
	pub demographics: Option<Vec<Term>>,
	pub formats: Option<Vec<Term>>,
	#[serde(alias = "theme")]
	pub tags: Option<Vec<Term>>,
	#[serde(alias = "author")]
	pub authors: Option<Vec<Term>>,
	#[serde(alias = "artist")]
	pub artists: Option<Vec<Term>>,
}

#[derive(Deserialize, Default, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ComixContentRating {
	Safe,
	Suggestive,
	Erotica,
	Pornographic,
	#[default]
	#[serde(other)]
	Unknown,
}

impl ComixContentRating {
	pub fn as_str(&self) -> Option<&'static str> {
		match self {
			Self::Safe => Some("safe"),
			Self::Suggestive => Some("suggestive"),
			Self::Erotica => Some("erotica"),
			Self::Pornographic => Some("pornographic"),
			Self::Unknown => None,
		}
	}
}

impl From<ComixContentRating> for ContentRating {
	fn from(value: ComixContentRating) -> Self {
		match value {
			ComixContentRating::Safe => ContentRating::Safe,
			ComixContentRating::Suggestive => ContentRating::Suggestive,
			ComixContentRating::Erotica | ComixContentRating::Pornographic => ContentRating::NSFW,
			ComixContentRating::Unknown => ContentRating::Unknown,
		}
	}
}

#[derive(Deserialize)]
pub struct Term {
	pub id: Option<i64>,
	#[serde(alias = "label", alias = "name")]
	pub title: String,
}

#[derive(Default)]
pub struct Poster {
	pub small: Option<String>,
	pub medium: Option<String>,
	pub large: Option<String>,
}

impl Poster {
	pub fn best_url(&self, base_url: &str, quality: &str) -> Option<String> {
		let raw = match quality {
			"medium" => self
				.medium
				.as_deref()
				.or(self.large.as_deref())
				.or(self.small.as_deref()),
			"small" => self
				.small
				.as_deref()
				.or(self.medium.as_deref())
				.or(self.large.as_deref()),
			_ => self
				.large
				.as_deref()
				.or(self.medium.as_deref())
				.or(self.small.as_deref()),
		}?;
		let resolved = helpers::resolve_url(base_url, raw);
		Some(helpers::apply_cover_quality(&resolved, quality))
	}
}

impl<'de> Deserialize<'de> for Poster {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: Deserializer<'de>,
	{
		struct PosterVisitor;

		impl<'de> de::Visitor<'de> for PosterVisitor {
			type Value = Poster;

			fn expecting(&self, formatter: &mut core::fmt::Formatter) -> core::fmt::Result {
				formatter.write_str("a poster URL string or object")
			}

			fn visit_str<E: de::Error>(self, v: &str) -> Result<Poster, E> {
				let s = v.trim();
				Ok(Poster {
					large: (!s.is_empty()).then(|| s.to_string()),
					..Default::default()
				})
			}

			fn visit_string<E: de::Error>(self, v: String) -> Result<Poster, E> {
				let s = v.trim();
				Ok(Poster {
					large: (!s.is_empty()).then(|| s.to_string()),
					..Default::default()
				})
			}

			fn visit_map<A>(self, mut map: A) -> Result<Poster, A::Error>
			where
				A: de::MapAccess<'de>,
			{
				let mut small = None;
				let mut medium = None;
				let mut large = None;
				let mut fallback = None;

				while let Some(key) = map.next_key::<String>()? {
					match key.as_str() {
						"small" => {
							small = map
								.next_value::<Option<String>>()?
								.and_then(|s| (!s.trim().is_empty()).then_some(s));
						}
						"medium" => {
							medium = map
								.next_value::<Option<String>>()?
								.and_then(|s| (!s.trim().is_empty()).then_some(s));
						}
						"large" => {
							large = map
								.next_value::<Option<String>>()?
								.and_then(|s| (!s.trim().is_empty()).then_some(s));
						}
						"url" | "thumb" => {
							fallback = map
								.next_value::<Option<String>>()?
								.and_then(|s| (!s.trim().is_empty()).then_some(s));
						}
						_ => {
							let _ = map.next_value::<de::IgnoredAny>()?;
						}
					}
				}

				if large.is_none() {
					large = fallback;
				}
				Ok(Poster {
					small,
					medium,
					large,
				})
			}
		}

		deserializer.deserialize_any(PosterVisitor)
	}
}

impl ComixManga {
	pub fn matches_settings(
		&self,
		allowed_ratings: &[&str],
		allowed_types: &[String],
		blocked_genres: &[String],
	) -> bool {
		if let Some(rating) = self.content_rating.as_str()
			&& !allowed_ratings.is_empty()
			&& !allowed_ratings.contains(&rating)
		{
			return false;
		}

		if !allowed_types.is_empty()
			&& !self.r#type.is_empty()
			&& !allowed_types.iter().any(|t| t == &self.r#type)
		{
			return false;
		}

		if !blocked_genres.is_empty()
			&& let Some(genres) = self.genres.as_ref()
			&& genres.iter().any(|g| {
				g.id.map(|id| id.to_string())
					.is_some_and(|id_str| blocked_genres.contains(&id_str))
			})
		{
			return false;
		}

		true
	}

	pub fn into_manga(self, base_url: &str, detailed: bool) -> Manga {
		let quality = settings::thumbnail_quality();
		let cover = self
			.poster
			.as_ref()
			.and_then(|p| p.best_url(base_url, &quality));
		let path = self
			.url
			.as_deref()
			.filter(|s| !s.trim().is_empty())
			.map(String::from)
			.unwrap_or_else(|| format!("/title/{}", self.hid));
		let url = helpers::resolve_url(base_url, &path);

		let description = if detailed {
			let mut parts: Vec<String> = Vec::new();
			let mut extra_sections: Vec<String> = Vec::new();
			let score_line = self
				.rated_avg
				.filter(|&s| s > 0.0)
				.map(|s| helpers::format_score(s, self.rated_count));

			if let Some(synopsis) = self.synopsis.as_deref().filter(|s| !s.trim().is_empty()) {
				let normalized = synopsis.replace("\r\n", "\n").replace('\r', "\n");
				let mut main_lines: Vec<String> = Vec::new();
				let mut link_lines: Vec<String> = Vec::new();
				let mut in_links = false;
				for raw_line in normalized.lines() {
					let line = raw_line.trim();
					if line.is_empty() {
						continue;
					}
					let lower = line.to_ascii_lowercase();
					if lower.starts_with("**original ")
						|| lower.starts_with("**official ")
						|| lower.starts_with("[original ")
						|| lower.starts_with("original webtoon")
						|| lower.starts_with("official translation")
					{
						in_links = true;
					}
					if in_links {
						link_lines.push(line.to_string());
					} else {
						main_lines.push(line.to_string());
					}
				}
				if !main_lines.is_empty() {
					parts.push(main_lines.join(" "));
				}
				if !link_lines.is_empty() {
					extra_sections.push(link_lines.join("\n"));
				}
			}

			if settings::show_score()
				&& let Some(line) = score_line
			{
				extra_sections.push(line);
			}

			if settings::alt_names_in_desc() && !self.alt_titles.is_empty() {
				let filtered: Vec<String> = self
					.alt_titles
					.iter()
					.map(|s| s.trim().to_string())
					.filter(|s| !s.is_empty())
					.collect();
				if !filtered.is_empty() {
					extra_sections.push(format!("Alternative Titles:\n- {}", filtered.join("\n- ")));
				}
			}

			if settings::show_extra_info() {
				let mut meta: Vec<String> = Vec::new();
				if let Some(y) = self.year.filter(|&y| y > 0) {
					meta.push(format!("Year: {y}"));
				}
				if let Some(lang) = self
					.original_language
					.as_deref()
					.filter(|l| !l.trim().is_empty())
				{
					meta.push(format!("Language: {}", lang.to_ascii_uppercase()));
				}
				if let Some(rank) = self.rank.filter(|&r| r > 0) {
					meta.push(format!("Rank: #{}", helpers::format_number_commas(rank as i64)));
				}
				if let Some(follows) = self.follows_total.filter(|&f| f > 0) {
					meta.push(format!("Followers: {}", helpers::format_number_commas(follows)));
				}
				if let Some(links) = self.links.as_ref() {
					let tracker_names = [
						("al", "AniList"),
						("mal", "MyAnimeList"),
						("mu", "MangaUpdates"),
						("md", "MangaDex"),
						("mb", "MangaBaka"),
					];
					let mut tracker_links = Vec::new();
					for (k, label) in tracker_names {
						if let Some(u) = links
							.get(k)
							.and_then(|v| v.as_str())
							.filter(|u| u.starts_with("http"))
						{
							tracker_links.push(format!("[{label}]({u})"));
						}
					}
					if !tracker_links.is_empty() {
						meta.push(format!("Trackers: {}", tracker_links.join(" · ")));
					}
				}
				if !meta.is_empty() {
					extra_sections.push(meta.join("\n"));
				}
			}

			parts.extend(extra_sections);
			(!parts.is_empty()).then(|| parts.join("\n\n"))
		} else {
			None
		};

		let mut chip_tags: Vec<String> = Vec::new();
		let push_unique = |list: &mut Vec<String>, label: String| {
			let trimmed = label.trim();
			if !trimmed.is_empty() && !list.iter().any(|x| x.eq_ignore_ascii_case(trimmed)) {
				list.push(trimmed.to_string());
			}
		};

		match self.r#type.as_str() {
			"manga" => push_unique(&mut chip_tags, "Manga".into()),
			"manhwa" => push_unique(&mut chip_tags, "Manhwa".into()),
			"manhua" => push_unique(&mut chip_tags, "Manhua".into()),
			_ => {}
		}
		if let Some(demographics) = self.demographics {
			for d in demographics {
				push_unique(&mut chip_tags, d.title);
			}
		}
		if let Some(genres) = self.genres {
			for g in genres {
				push_unique(&mut chip_tags, g.title);
			}
		}
		if let Some(formats) = self.formats {
			for f in formats {
				push_unique(&mut chip_tags, f.title);
			}
		}
		if detailed
			&& settings::show_tags_in_genres()
			&& let Some(tags) = self.tags
		{
			for t in tags {
				push_unique(&mut chip_tags, t.title);
			}
		}

		let authors = self.authors.map(|v| {
			v.into_iter()
				.map(|t| t.title.trim().to_string())
				.filter(|s| !s.is_empty())
				.collect::<Vec<_>>()
		});
		let artists = self.artists.map(|v| {
			v.into_iter()
				.map(|t| t.title.trim().to_string())
				.filter(|s| !s.is_empty())
				.collect::<Vec<_>>()
		});

		Manga {
			key: self.hid,
			title: self.title,
			cover,
			artists: artists.filter(|v| !v.is_empty()),
			authors: authors.filter(|v| !v.is_empty()),
			description,
			url: Some(url),
			tags: (!chip_tags.is_empty()).then_some(chip_tags),
			status: match self.status.as_str() {
				"releasing" => MangaStatus::Ongoing,
				"finished" => MangaStatus::Completed,
				"on_hiatus" => MangaStatus::Hiatus,
				"discontinued" => MangaStatus::Cancelled,
				_ => MangaStatus::Unknown,
			},
			content_rating: self.content_rating.into(),
			viewer: match self.r#type.as_str() {
				"manhwa" | "manhua" => Viewer::Webtoon,
				"manga" => Viewer::RightToLeft,
				_ => Viewer::Unknown,
			},
			..Default::default()
		}
	}
}

#[derive(Deserialize)]
pub struct TagsSearchResponse {
	pub result: Option<Vec<TagSearchItem>>,
}

#[derive(Deserialize)]
pub struct TagSearchItem {
	pub id: i64,
	pub label: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowseHarvestResult {
	pub per_page: Option<i32>,
	#[serde(default)]
	pub count: usize,
	#[serde(default)]
	pub items: Vec<BrowseItem>,
}

#[derive(Deserialize)]
pub struct BrowseItem {
	pub hid: String,
	pub url: String,
	#[serde(default)]
	pub title: String,
	pub cover: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChaptersHarvestResult {
	pub hid: Option<String>,
	#[serde(default)]
	pub count: usize,
	#[serde(default)]
	pub complete: bool,
	#[serde(default)]
	pub chapters: Vec<HarvestedChapter>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HarvestedChapter {
	pub id: i64,
	pub number: f32,
	pub volume: Option<f32>,
	pub url: String,
	#[serde(default)]
	pub text: String,
	pub name: Option<String>,
	#[serde(default)]
	pub votes: i64,
	pub group: Option<HarvestedGroup>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HarvestedGroup {
	pub id: Option<i64>,
	pub name: String,
	#[serde(default)]
	pub is_official: bool,
}

#[derive(Deserialize)]
pub struct PagesHarvestResult {
	#[serde(default)]
	pub total: usize,
	#[serde(default)]
	pub pages: Vec<HarvestedPage>,
}

#[derive(Deserialize)]
pub struct HarvestedPage {
	pub index: usize,
	pub url: String,
}

#[derive(Deserialize)]
pub struct ApiResponse<T> {
	pub result: T,
}

#[derive(Deserialize, Default)]
pub struct ApiMeta {
	#[serde(default, deserialize_with = "null_default")]
	pub page: i32,
	#[serde(
		default,
		alias = "last_page",
		alias = "lastPage",
		deserialize_with = "null_default"
	)]
	pub last_page: i32,
}

#[derive(Deserialize)]
pub struct MangaItems {
	#[serde(default, deserialize_with = "null_default")]
	pub items: Vec<ComixManga>,
	#[serde(default)]
	pub meta: ApiMeta,
}

#[derive(Deserialize)]
pub struct ChapterItems {
	#[serde(default, deserialize_with = "null_default")]
	pub items: Vec<ApiChapter>,
	#[serde(default)]
	pub meta: ApiMeta,
}

#[derive(Deserialize)]
pub struct ApiUploader {
	pub id: Option<i64>,
	pub name: Option<String>,
}

#[derive(Deserialize)]
pub struct ApiChapter {
	pub id: i64,
	#[serde(default, deserialize_with = "null_default")]
	pub url: String,
	#[serde(default, deserialize_with = "null_default")]
	pub number: f32,
	#[serde(default, deserialize_with = "null_default")]
	pub volume: f32,
	pub name: Option<String>,
	#[serde(default, deserialize_with = "null_default")]
	pub votes: i64,
	#[serde(
		default,
		alias = "created_at_formatted",
		alias = "createdAtFormatted",
		deserialize_with = "null_default"
	)]
	pub created_at_formatted: String,
	pub group: Option<ApiUploader>,
	pub creator: Option<ApiUploader>,
	#[serde(
		default,
		alias = "is_official",
		alias = "isOfficial",
		deserialize_with = "null_default"
	)]
	pub is_official: bool,
}

impl ApiChapter {
	pub fn into_harvested(self) -> HarvestedChapter {
		let group_name = self
			.group
			.as_ref()
			.and_then(|g| g.name.as_deref())
			.or_else(|| self.creator.as_ref().and_then(|c| c.name.as_deref()))
			.map(str::trim)
			.filter(|s| !s.is_empty())
			.map(String::from)
			.or_else(|| self.is_official.then(|| String::from("Official")));
		let group_id = self
			.group
			.as_ref()
			.and_then(|g| g.id)
			.or_else(|| self.is_official.then_some(10702));
		let group = group_name.map(|name| HarvestedGroup {
			id: group_id,
			name,
			is_official: self.is_official,
		});
		HarvestedChapter {
			id: self.id,
			number: self.number,
			volume: (self.volume > 0.0).then_some(self.volume),
			url: self.url,
			text: self.created_at_formatted,
			name: self.name,
			votes: self.votes,
			group,
		}
	}
}

#[derive(Deserialize)]
pub struct ApiRefusal {
	pub code: Option<serde_json::Value>,
	pub error: Option<String>,
}

impl ApiRefusal {
	pub fn code_str(&self) -> Option<String> {
		match self.code.as_ref()? {
			serde_json::Value::String(s) => Some(s.clone()),
			serde_json::Value::Number(n) => Some(n.to_string()),
			_ => None,
		}
	}
}

#[derive(Deserialize)]
pub struct EncryptedBody {
	pub e: String,
}

#[derive(Deserialize)]
pub struct ChapterPagesResult {
	pub pages: ApiChapterPages,
}

#[derive(Deserialize)]
pub struct ApiChapterPages {
	#[serde(default, deserialize_with = "null_default")]
	pub items: Vec<ApiPageDto>,
	#[serde(default, deserialize_with = "null_default")]
	pub alt: Vec<String>,
	pub alt2: Option<serde_json::Value>,
	#[serde(alias = "alt_node")]
	pub alt_node: Option<serde_json::Value>,
}

#[derive(Deserialize)]
pub struct ApiPageDto {
	pub url: String,
}

fn json_primitive_string(val: Option<&serde_json::Value>) -> Option<String> {
	match val? {
		serde_json::Value::String(s) => {
			let t = s.trim();
			(!t.is_empty()).then(|| t.to_string())
		}
		serde_json::Value::Number(n) => Some(n.to_string()),
		_ => None,
	}
}

impl ApiChapterPages {
	pub fn alt_hosts(&self, host: &str, index: usize) -> Vec<String> {
		let Some((label, domain)) = host.split_once('.') else {
			return Vec::new();
		};
		let mut domains: Vec<String> = Vec::new();
		domains.push(domain.to_string());
		if !self.alt.is_empty()
			&& let Some(d) = self.alt.get(index % self.alt.len())
		{
			let d = d.trim();
			if !d.is_empty() && !domains.iter().any(|x| x == d) {
				domains.push(d.to_string());
			}
		}
		if let Some(d) = json_primitive_string(self.alt2.as_ref())
			&& !domains.iter().any(|x| x == &d)
		{
			domains.push(d);
		}

		let mut labels: Vec<String> = Vec::new();
		labels.push(label.to_string());
		if let Some(l) = json_primitive_string(self.alt_node.as_ref())
			&& !labels.iter().any(|x| x == &l)
		{
			labels.push(l);
		}

		let mut out = Vec::new();
		for l in &labels {
			for d in &domains {
				let candidate = format!("{l}.{d}");
				if candidate != host && !out.contains(&candidate) {
					out.push(candidate);
				}
			}
		}
		out
	}
}

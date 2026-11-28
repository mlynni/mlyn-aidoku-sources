use crate::{
	helpers, settings,
	models::{
		ApiRefusal, BrowseHarvestResult, ChaptersHarvestResult, EncryptedBody, InitialData,
		PagesHarvestResult, TagsSearchResponse,
	},
	site_key::{self, CachedSiteKey, USER_AGENT},
};
use aidoku::{
	Result,
	alloc::{String, Vec, format, string::ToString},
	helpers::uri::{QueryParameters, encode_uri_component},
	imports::{
		js::{JsContext, WebView},
		net::{Request, Response},
		std::sleep,
	},
	prelude::*,
};
use serde::{Deserialize, de::DeserializeOwned};

const HARVEST_BROWSE_JS: &str = include_str!("../assets/harvest_browse.js");
const HARVEST_CHAPTERS_JS: &str = include_str!("../assets/harvest_chapters.js");
const CHAPTER_PAGES_JS: &str = include_str!("../assets/chapter_pages.js");

const WAF_ERROR_MSG: &str = "Comix requested a security check. Open Comix Settings → Site Check (Verify comix.to / Verify comix.ws) or switch Mirror.";

const READY_PROBE_JS: &str = "(() => JSON.stringify({ ready: document.readyState || 'loading', href: location.href || '', path: location.pathname || '', title: document.title || '', hasInitialData: !!document.querySelector('script#initial-data'), waf: (location.pathname || '').startsWith('/@waf/') || document.title === 'Security check' || document.title === 'Just a moment...' || !!document.querySelector('iframe[src*=\"/@waf/challenge\"]') }))()";

const POLL_RESULT_JS: &str = "(() => JSON.stringify({ waf: !!window.__comixWaf || (location.pathname || '').startsWith('/@waf/') || document.title === 'Security check' || !!document.querySelector('iframe[src*=\"/@waf/challenge\"]'), done: !!window.__comixResult, error: window.__comixError || null, result: window.__comixResult || null }))()";

const READ_INITIAL_DATA_JS: &str = "(() => { const s = document.querySelector('script#initial-data'); return s ? (s.textContent || '') : ''; })()";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReadyProbe {
	ready: String,
	href: String,
	#[serde(default)]
	has_initial_data: bool,
	waf: bool,
}

#[derive(Deserialize)]
struct PollState {
	waf: bool,
	done: bool,
	error: Option<String>,
	result: Option<serde_json::Value>,
}

/// How long to let a security check resolve on its own before giving up.
/// The Turnstile widget on `/@waf/challenge` usually passes automatically in a real
/// WebView and then redirects, so we wait for that instead of failing immediately.
const WAF_AUTO_PASS_SECS: i32 = 25;

enum Ready {
	Page,
	Waf,
}

enum Polled<T> {
	Done(T),
	Waf,
}

fn sleep_ms(ms: u32) {
	let ctx = JsContext::new();
	let _ = ctx.eval(&format!("var t=Date.now()+{ms};while(Date.now()<t){{}};''"));
}

fn pause_poll(step: i32, fast_steps: i32) {
	if step < fast_steps {
		sleep_ms(60);
	} else {
		sleep(1);
	}
}

fn wait_webview_ready(wv: &WebView, timeout_secs: i32, wait_for_initial_data: bool) -> Result<Ready> {
	const FAST_STEPS: i32 = 40;
	for step in 0..(FAST_STEPS + timeout_secs) {
		if let Ok(raw) = wv.eval(READY_PROBE_JS)
			&& let Ok(probe) = serde_json::from_str::<ReadyProbe>(&raw)
		{
			if probe.waf {
				return Ok(Ready::Waf);
			}
			let loaded = !probe.href.is_empty()
				&& probe.href != "about:blank"
				&& probe.ready != "loading";
			if loaded && (!wait_for_initial_data || probe.has_initial_data || probe.ready == "complete") {
				return Ok(Ready::Page);
			}
		}
		pause_poll(step, FAST_STEPS);
	}
	bail!("Timed out waiting for Comix WebView to load")
}

const SYNC_WAF_COOKIE_JS: &str = "(() => { const m = document.cookie.match(/(?:^|;\\s*)waf_pass=([^;]+)/); return m ? decodeURIComponent(m[1]) : ''; })()";

fn extract_waf_pass_from_header(raw: &str) -> Option<String> {
	for segment in raw.split([',', ';']) {
		let trimmed = segment.trim();
		if let Some(value) = trimmed.strip_prefix("waf_pass=") {
			let token = value.split(';').next().unwrap_or(value).trim();
			if !token.is_empty() {
				return Some(token.to_string());
			}
		}
	}
	None
}

fn capture_waf_pass(resp: &Response, base_url: &str) {
	if let Some(raw) = resp
		.get_header("Set-Cookie")
		.or_else(|| resp.get_header("set-cookie"))
		&& let Some(token) = extract_waf_pass_from_header(&raw)
	{
		settings::set_waf_pass_for(base_url, &token);
	}
}

fn sync_waf_cookie_from_webview(wv: &WebView, url: &str) {
	if let Ok(token) = wv.eval(SYNC_WAF_COOKIE_JS) {
		let trimmed = token.trim();
		if !trimmed.is_empty() {
			settings::set_waf_pass_for(url, trimmed);
		}
	}
}

pub enum ApiError {
	CaptchaRequired,
	Other(String),
}

fn load_or_extract_key(base_url: &str, force_refresh: bool) -> core::result::Result<CachedSiteKey, ApiError> {
	if !force_refresh
		&& let Some(raw) = settings::cached_site_key()
		&& let Some(cached) = CachedSiteKey::deserialize(&raw)
	{
		return Ok(cached);
	}
	let waf_pass = settings::waf_pass_for(base_url);
	site_key::extract_site_key(base_url, waf_pass.as_deref())
		.map_err(|e| ApiError::Other(e.to_string()))
}

pub fn call_signed_api<T: DeserializeOwned>(
	base_url: &str,
	api_path: &str,
	params: &[(String, String)],
	referer_path: &str,
) -> core::result::Result<T, ApiError> {
	let base = base_url.trim_end_matches('/');
	let mut key_refreshed = false;
	let mut gateway_retried = false;

	loop {
		let cached = load_or_extract_key(base, key_refreshed)?;
		let sig = cached.key.sign(api_path, params);

		let mut qs = QueryParameters::new();
		for (k, v) in params {
			qs.push(k, Some(v));
		}
		qs.push("_", Some(&sig));
		let url = format!("{base}/api/v1{api_path}?{qs}");

		let referer = if referer_path.starts_with("http") {
			referer_path.to_string()
		} else {
			format!("{base}{referer_path}")
		};

		let mut req = Request::get(&url)
			.map_err(|e| ApiError::Other(format!("{e:?}")))?
			.header("User-Agent", USER_AGENT)
			.header("Accept", "application/json")
			.header("X-Requested-With", "XMLHttpRequest")
			.header("X-Rt", "0")
			.header("Referer", &referer);

		if let Some(token) = settings::waf_pass_for(base) {
			req = req.header("Cookie", &format!("waf_pass={token}"));
		}

		let resp = req
			.send()
			.map_err(|e| ApiError::Other(format!("{e:?}")))?;
		capture_waf_pass(&resp, base);

		let status = resp.status_code();
		if (200..300).contains(&status) {
			if let Some(build_id) = resp.get_header("x-build-id")
				&& site_key::is_newer_build(&build_id, &cached.build)
			{
				settings::set_cached_site_key(None);
			}
			let encrypted = resp.get_header("x-enc").as_deref() == Some("2");
			let raw_body = resp
				.get_string()
				.map_err(|e| ApiError::Other(format!("{e:?}")))?;
			let payload = if encrypted {
				let wrapper = serde_json::from_str::<EncryptedBody>(&raw_body)
					.map_err(|e| ApiError::Other(format!("Encrypted wrapper JSON error: {e}")))?;
				cached
					.key
					.decrypt(&wrapper.e)
					.map_err(|e| ApiError::Other(e.to_string()))?
			} else {
				raw_body
			};
			let parsed = serde_json::from_str::<T>(&payload)
				.map_err(|e| ApiError::Other(format!("API JSON parse error: {e}")))?;
			settings::mark_verified(true);
			return Ok(parsed);
		}

		let body = resp.get_string().unwrap_or_default();
		let refusal = serde_json::from_str::<ApiRefusal>(&body).ok();
		let err_str = refusal
			.as_ref()
			.and_then(|r| r.error.as_deref())
			.unwrap_or("");
		let code_str = refusal.as_ref().and_then(|r| r.code_str());

		if err_str == "captcha_required" {
			return Err(ApiError::CaptchaRequired);
		}
		if matches!(code_str.as_deref(), Some("key_retired" | "invalid_token")) && !key_refreshed {
			settings::set_cached_site_key(None);
			key_refreshed = true;
			continue;
		}
		if status == 502 && !gateway_retried {
			sleep(1);
			gateway_retried = true;
			continue;
		}
		return Err(ApiError::Other(format!("Comix API returned status {status}")));
	}
}

/// Waits for the security check to clear, then reloads `url` and waits for the real page.
/// Fails with `WAF_ERROR_MSG` if the check needs manual interaction or never clears.
fn recover_from_waf(wv: &WebView, url: &str, wait_for_initial_data: bool) -> Result<()> {
	let mut cleared = false;
	for _ in 0..WAF_AUTO_PASS_SECS {
		sleep(1);
		if let Ok(raw) = wv.eval(READY_PROBE_JS)
			&& let Ok(probe) = serde_json::from_str::<ReadyProbe>(&raw)
			&& !probe.waf
			&& probe.ready != "loading"
		{
			cleared = true;
			break;
		}
	}
	if !cleared {
		settings::mark_verified(false);
		bail!("{WAF_ERROR_MSG}");
	}
	// The check may redirect to `/` instead of the page we wanted, so load it again.
	wv.load(Request::get(url)?)?;
	match wait_webview_ready(wv, 20, wait_for_initial_data)? {
		Ready::Page => {
			sync_waf_cookie_from_webview(wv, url);
			settings::mark_verified(true);
			Ok(())
		}
		Ready::Waf => {
			settings::mark_verified(false);
			bail!("{WAF_ERROR_MSG}")
		}
	}
}

/// Opens `url` in a fresh WebView, recovering from a security check when possible.
fn open_page(url: &str, wait_for_initial_data: bool) -> Result<WebView> {
	let wv = WebView::new();
	wv.load(Request::get(url)?)?;
	match wait_webview_ready(&wv, 20, wait_for_initial_data)? {
		Ready::Page => {
			sync_waf_cookie_from_webview(&wv, url);
			settings::mark_verified(true);
		}
		Ready::Waf => recover_from_waf(&wv, url, wait_for_initial_data)?,
	}
	Ok(wv)
}

fn poll_webview_result<T: DeserializeOwned>(wv: &WebView, max_secs: i32) -> Result<Polled<T>> {
	const FAST_STEPS: i32 = 45;
	sleep_ms(45);
	for step in 0..(FAST_STEPS + max_secs) {
		if let Ok(raw) = wv.eval(POLL_RESULT_JS)
			&& let Ok(state) = serde_json::from_str::<PollState>(&raw)
		{
			if state.waf {
				return Ok(Polled::Waf);
			}
			if let Some(err) = state.error {
				bail!("Comix error: {err}");
			}
			if state.done
				&& let Some(val) = state.result
			{
				return serde_json::from_value::<T>(val)
					.map(Polled::Done)
					.map_err(|e| error!("Failed to parse Comix WebView result: {e}"));
			}
		}
		pause_poll(step, FAST_STEPS);
	}
	bail!("Timed out waiting for Comix page data")
}

/// Runs a harvest script and polls for its result. If a security check shows up mid-run,
/// waits for it to clear, reloads the page, and runs the script once more.
fn run_script<T: DeserializeOwned>(
	wv: &WebView,
	url: &str,
	script: &str,
	start_error: &str,
	max_secs: i32,
) -> Result<T> {
	for attempt in 0..2 {
		wv.eval(script).map_err(|_| error!("{start_error}"))?;
		match poll_webview_result::<T>(wv, max_secs)? {
			Polled::Done(value) => return Ok(value),
			Polled::Waf if attempt == 0 => recover_from_waf(wv, url, false)?,
			Polled::Waf => break,
		}
	}
	settings::mark_verified(false);
	bail!("{WAF_ERROR_MSG}")
}

fn try_http_initial_data(url: &str) -> Option<InitialData> {
	let mut req = Request::get(url)
		.ok()?
		.header("User-Agent", USER_AGENT)
		.header("Accept", "text/html,application/xhtml+xml");
	if let Some(token) = settings::waf_pass_for(url) {
		req = req.header("Cookie", &format!("waf_pass={token}"));
	}
	let resp = req.send().ok()?;
	capture_waf_pass(&resp, url);
	if resp.status_code() >= 400 {
		return None;
	}
	let html = resp.get_string().ok()?;
	if helpers::is_waf_or_cf_html(&html) {
		return None;
	}
	let json_str = helpers::extract_initial_data_json(&html)?;
	let parsed = serde_json::from_str::<InitialData>(json_str).ok()?;
	settings::mark_verified(true);
	Some(parsed)
}

/// Fetches SSR `script#initial-data` via HTTP first, falling back to WebView if challenged.
pub fn fetch_initial_data(url: &str) -> Result<InitialData> {
	if let Some(parsed) = try_http_initial_data(url) {
		return Ok(parsed);
	}

	let wv = open_page(url, true)?;
	let raw = wv
		.eval(READ_INITIAL_DATA_JS)
		.map_err(|_| error!("Failed to read initial-data from WebView"))?;
	if raw.trim().is_empty() {
		bail!("Comix page did not include initial-data");
	}
	serde_json::from_str::<InitialData>(&raw)
		.map_err(|e| error!("Failed to parse Comix initial-data: {e}"))
}

/// Resolves comma-separated author/artist/tag names via `/api/v1/tags/search`.
pub fn fetch_tag_ids(base_url: &str, term_type: &str, raw_csv: &str) -> Vec<String> {
	let base = base_url.trim_end_matches('/');
	let mut ids = Vec::new();
	for token in raw_csv.split(',') {
		let query = token.trim();
		if query.is_empty() {
			continue;
		}
		if query.chars().all(|c| c.is_ascii_digit()) {
			if !ids.iter().any(|x| x == query) {
				ids.push(query.to_string());
			}
			continue;
		}
		let url = format!(
			"{base}/api/v1/tags/search?type={term_type}&q={}&limit=20",
			encode_uri_component(query)
		);
		if let Ok(mut req) = Request::get(&url).map(|r| {
			r.header("User-Agent", USER_AGENT)
				.header("Accept", "application/json")
				.header("X-Requested-With", "XMLHttpRequest")
				.header("Referer", &format!("{base}/browse"))
		}) {
			if let Some(token) = settings::waf_pass_for(base) {
				req = req.header("Cookie", &format!("waf_pass={token}"));
			}
			if let Ok(resp) = req.send()
				&& resp.status_code() < 400
				&& let Ok(parsed) = resp.get_json_owned::<TagsSearchResponse>()
				&& let Some(items) = parsed.result
			{
				let picked = items
					.iter()
					.find(|item| item.label.eq_ignore_ascii_case(query))
					.or_else(|| items.first());
				if let Some(item) = picked {
					let id_str = item.id.to_string();
					if !ids.contains(&id_str) {
						ids.push(id_str);
					}
				}
			}
		}
	}
	ids
}

pub fn harvest_browse(url: &str) -> Result<BrowseHarvestResult> {
	let wv = open_page(url, false)?;
	run_script(&wv, url, HARVEST_BROWSE_JS, "Failed to start Comix browse script", 35)
}

pub fn harvest_title_page<F>(
	url: &str,
	needs_details: bool,
	needs_chapters: bool,
	on_details: F,
) -> Result<(Option<InitialData>, Option<ChaptersHarvestResult>)>
where
	F: FnOnce(&InitialData),
{
	let mut on_details = Some(on_details);
	let mut details: Option<InitialData> = None;

	if needs_details
		&& let Some(data) = try_http_initial_data(url)
	{
		if let Some(cb) = on_details.take() {
			cb(&data);
		}
		details = Some(data);
	}

	if !needs_chapters {
		if needs_details && details.is_none() {
			let data = fetch_initial_data(url)?;
			if let Some(cb) = on_details.take() {
				cb(&data);
			}
			return Ok((Some(data), None));
		}
		return Ok((details, None));
	}

	let wv = open_page(url, needs_details && details.is_none())?;

	if needs_details && details.is_none() {
		let raw = wv.eval(READ_INITIAL_DATA_JS).unwrap_or_default();
		let parsed = serde_json::from_str::<InitialData>(&raw).ok();
		if let Some(ref data) = parsed
			&& let Some(cb) = on_details.take()
		{
			cb(data);
		}
		details = parsed;
	}

	let chapters: ChaptersHarvestResult = run_script(
		&wv,
		url,
		HARVEST_CHAPTERS_JS,
		"Failed to start Comix chapter script",
		90,
	)?;
	Ok((details, Some(chapters)))
}

pub fn harvest_chapter_pages(url: &str) -> Result<PagesHarvestResult> {
	let wv = open_page(url, false)?;
	run_script(&wv, url, CHAPTER_PAGES_JS, "Failed to start Comix reader script", 90)
}

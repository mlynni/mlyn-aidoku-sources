use crate::settings;
use aidoku::{
	HashMap, Result,
	alloc::{String, Vec, format, string::ToString, vec},
	imports::{js::JsContext, net::Request},
	prelude::*,
};

pub const USER_AGENT: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1";
const MASK_SIZE: usize = 1024;
const MATERIAL_SIZE: usize = 64 + MASK_SIZE;

const PRELUDE_JS: &str = r#"
globalThis.__t = (fn) => { let m; return () => (m || (fn((m = { exports: {} }).exports, m), fn = null), m.exports); };
globalThis.__n = (a, b) => { let c = {}; for (var d in a) Object.defineProperty(c, d, { get: a[d], enumerable: true }); return b || Object.defineProperty(c, Symbol.toStringTag, { value: 'Module' }), c; };
globalThis.__r = (a, b, c) => (c = a != null ? Object.create(Object.getPrototypeOf(a)) : {},
	((t, e) => { if (e && typeof e === 'object' || typeof e === 'function') for (const k of Object.getOwnPropertyNames(e)) if (!Object.prototype.hasOwnProperty.call(t, k) && k !== 'default') Object.defineProperty(t, k, { get: () => e[k], enumerable: true }); return t; })(
		Object.defineProperty(c, 'default', { value: a, enumerable: true }), a));
"#;

const FIND_KEY_JS: &str = r#"
(() => {
	for (const name of Object.getOwnPropertyNames(globalThis)) {
		let value;
		try { value = globalThis[name]; } catch (e) { continue; }
		if (!value || typeof value !== 'object' || typeof value.kid !== 'string' || typeof value.material !== 'function') continue;
		const material = value.material();
		if (material && material.length === 1088) return value.kid + ',' + Array.from(material).join(',');
	}
	return '';
})()
"#;

const K: [u32; 64] = [
	0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
	0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
	0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
	0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
	0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
	0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
	0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
	0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

#[derive(Clone)]
pub struct SiteKey {
	pub id: String,
	inner: [u32; 8],
	outer: [u32; 8],
	mask: Vec<u8>,
	raw_material: Vec<u8>,
}

#[derive(Clone)]
pub struct CachedSiteKey {
	pub key: SiteKey,
	pub build: String,
}

impl SiteKey {
	pub fn of(id: String, material: &[u8]) -> Result<Self> {
		if material.len() != MATERIAL_SIZE {
			bail!("Invalid Comix site key material length");
		}
		let mut inner = [0u32; 8];
		let mut outer = [0u32; 8];
		for i in 0..8 {
			let off_in = i * 4;
			inner[i] = u32::from_be_bytes([
				material[off_in],
				material[off_in + 1],
				material[off_in + 2],
				material[off_in + 3],
			]);
			let off_out = 32 + i * 4;
			outer[i] = u32::from_be_bytes([
				material[off_out],
				material[off_out + 1],
				material[off_out + 2],
				material[off_out + 3],
			]);
		}
		Ok(Self {
			id,
			inner,
			outer,
			mask: material[64..MATERIAL_SIZE].to_vec(),
			raw_material: material.to_vec(),
		})
	}

	pub fn sign(&self, api_path: &str, params: &[(String, String)]) -> String {
		let canon = canonical(api_path, params);
		let first = compress(&self.inner, canon.as_bytes());
		let digest = compress(&self.outer, &first);
		format!("{}.{}", self.id, encode_base64url_nopad(&digest[..12]))
	}

	pub fn decrypt(&self, encoded: &str) -> Result<String> {
		let data = decode_base64(encoded).ok_or_else(|| error!("Invalid base64 payload"))?;
		if data.len() < 2 {
			bail!("Encrypted response is too short");
		}
		let start = ((data[0] as usize) << 8) | (data[1] as usize);
		let rotation = start % MASK_SIZE;
		let mut plain = Vec::with_capacity(data.len() - 2);
		for (i, &byte) in data[2..].iter().enumerate() {
			plain.push(byte ^ self.mask[(rotation + i) % MASK_SIZE]);
		}
		String::from_utf8(plain).map_err(|_| error!("Decrypted response is not valid UTF-8"))
	}
}

impl CachedSiteKey {
	pub fn serialize(&self) -> String {
		format!(
			"{}|{}|{}",
			self.build,
			self.key.id,
			encode_base64url_nopad(&self.key.raw_material)
		)
	}

	pub fn deserialize(raw: &str) -> Option<Self> {
		let mut parts = raw.splitn(3, '|');
		let build = parts.next()?.trim();
		let id = parts.next()?.trim();
		let b64 = parts.next()?.trim();
		if build.is_empty() || id.is_empty() || b64.is_empty() {
			return None;
		}
		let material = decode_base64(b64)?;
		let key = SiteKey::of(id.to_string(), &material).ok()?;
		Some(Self {
			key,
			build: build.to_string(),
		})
	}
}

pub fn is_newer_build(theirs: &str, ours: &str) -> bool {
	let Ok(their_num) = u64::from_str_radix(theirs.trim(), 36) else {
		return false;
	};
	let Ok(our_num) = u64::from_str_radix(ours.trim(), 36) else {
		return false;
	};
	their_num > our_num
}

fn canonical(api_path: &str, query_params: &[(String, String)]) -> String {
	let path = api_path.strip_prefix("/api/v1").unwrap_or(api_path);
	let mut indexes: HashMap<String, usize> = HashMap::new();
	let mut items: Vec<(String, String, usize, String)> = Vec::new();

	for (name, value) in query_params {
		if name == "_" {
			continue;
		}
		let base = name.split('[').next().unwrap_or(name.as_str());
		if name.ends_with("[]") {
			let idx = indexes.get(base).copied().unwrap_or(0);
			indexes.insert(base.to_string(), idx + 1);
			items.push((
				base.to_string(),
				String::new(),
				idx,
				format!("{base}[{idx}]={value}"),
			));
		} else {
			let nested = &name[base.len()..];
			items.push((
				base.to_string(),
				nested.to_string(),
				0,
				format!("{name}={value}"),
			));
		}
	}

	if items.is_empty() {
		return path.to_string();
	}

	items.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)).then_with(|| a.2.cmp(&b.2)));
	let mut out = String::from(path);
	out.push('?');
	for (i, item) in items.iter().enumerate() {
		if i > 0 {
			out.push('&');
		}
		out.push_str(&item.3);
	}
	out
}

fn compress(iv: &[u32; 8], message: &[u8]) -> [u8; 32] {
	let blocks = (message.len() + 8) / 64 + 1;
	let mut padded = vec![0u8; blocks * 64];
	padded[..message.len()].copy_from_slice(message);
	padded[message.len()] = 0x80;
	let bit_length = (message.len() as u64 + 64) * 8;
	let total = padded.len();
	for i in 0..8 {
		padded[total - 1 - i] = (bit_length >> (8 * i)) as u8;
	}

	let mut h = *iv;
	let mut w = [0u32; 64];
	for block in 0..blocks {
		let offset = block * 64;
		for i in 0..16 {
			w[i] = u32::from_be_bytes([
				padded[offset + i * 4],
				padded[offset + i * 4 + 1],
				padded[offset + i * 4 + 2],
				padded[offset + i * 4 + 3],
			]);
		}
		for i in 16..64 {
			let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
			let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
			w[i] = w[i - 16]
				.wrapping_add(s0)
				.wrapping_add(w[i - 7])
				.wrapping_add(s1);
		}

		let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
		for i in 0..64 {
			let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
			let ch = (e & f) ^ (!e & g);
			let t1 = hh
				.wrapping_add(s1)
				.wrapping_add(ch)
				.wrapping_add(K[i])
				.wrapping_add(w[i]);
			let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
			let maj = (a & b) ^ (a & c) ^ (b & c);
			let t2 = s0.wrapping_add(maj);
			hh = g;
			g = f;
			f = e;
			e = d.wrapping_add(t1);
			d = c;
			c = b;
			b = a;
			a = t1.wrapping_add(t2);
		}

		h[0] = h[0].wrapping_add(a);
		h[1] = h[1].wrapping_add(b);
		h[2] = h[2].wrapping_add(c);
		h[3] = h[3].wrapping_add(d);
		h[4] = h[4].wrapping_add(e);
		h[5] = h[5].wrapping_add(f);
		h[6] = h[6].wrapping_add(g);
		h[7] = h[7].wrapping_add(hh);
	}

	let mut out = [0u8; 32];
	for (i, word) in h.iter().enumerate() {
		out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
	}
	out
}

pub fn encode_base64url_nopad(input: &[u8]) -> String {
	const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
	let mut out = String::with_capacity((input.len() * 4).div_ceil(3));
	for chunk in input.chunks(3) {
		let b0 = chunk[0] as usize;
		let b1 = *chunk.get(1).unwrap_or(&0) as usize;
		let b2 = *chunk.get(2).unwrap_or(&0) as usize;
		out.push(TABLE[(b0 >> 2) & 0x3F] as char);
		out.push(TABLE[((b0 & 0x03) << 4) | (b1 >> 4)] as char);
		if chunk.len() > 1 {
			out.push(TABLE[((b1 & 0x0F) << 2) | (b2 >> 6)] as char);
		}
		if chunk.len() > 2 {
			out.push(TABLE[b2 & 0x3F] as char);
		}
	}
	out
}

pub fn decode_base64(input: &str) -> Option<Vec<u8>> {
	let mut out = Vec::with_capacity(input.len() * 3 / 4);
	let mut buf: u32 = 0;
	let mut bits: u8 = 0;
	for b in input.bytes() {
		if b.is_ascii_whitespace() {
			continue;
		}
		if b == b'=' {
			break;
		}
		let val = match b {
			b'A'..=b'Z' => b - b'A',
			b'a'..=b'z' => b - b'a' + 26,
			b'0'..=b'9' => b - b'0' + 52,
			b'+' | b'-' => 62,
			b'/' | b'_' => 63,
			_ => return None,
		};
		buf = (buf << 6) | (val as u32);
		bits += 6;
		if bits >= 8 {
			bits -= 8;
			out.push(((buf >> bits) & 0xFF) as u8);
		}
	}
	Some(out)
}

fn fetch_text(url: &str, waf_pass: Option<&str>) -> Result<String> {
	let mut req = Request::get(url)?.header("User-Agent", USER_AGENT);
	if let Some(token) = waf_pass.filter(|t| !t.trim().is_empty()) {
		req = req.header("Cookie", &format!("waf_pass={token}"));
	}
	let resp = req.send()?;
	if resp.status_code() >= 400 {
		bail!("HTTP {} fetching {}", resp.status_code(), url);
	}
	Ok(resp.get_string()?)
}

fn resolve_script_url(base: &str, rel: &str) -> String {
	if rel.starts_with("http://") || rel.starts_with("https://") {
		return rel.to_string();
	}
	let origin = if let Some(rest) = base
		.strip_prefix("https://")
		.or_else(|| base.strip_prefix("http://"))
	{
		let scheme_len = base.len() - rest.len();
		let host = rest.split('/').next().unwrap_or(rest);
		format!("{}{}", &base[..scheme_len], host)
	} else {
		base.trim_end_matches('/').to_string()
	};
	if rel.starts_with("//") {
		return format!("https:{rel}");
	}
	if rel.starts_with('/') {
		return format!("{origin}{rel}");
	}
	let cleaned = rel.strip_prefix("./").unwrap_or(rel);
	if let Some(slash) = base.rfind('/') {
		format!("{}/{cleaned}", &base[..slash])
	} else {
		format!("{origin}/{cleaned}")
	}
}

fn find_main_script_src(html: &str) -> Option<String> {
	let pos = html.find("/dist/main-")?;
	let before = &html[..pos];
	let quote_start = before.rfind(['"', '\''])?;
	let quote_char = html.as_bytes()[quote_start] as char;
	let after = &html[quote_start + 1..];
	let quote_end = after.find(quote_char)?;
	Some(after[..quote_end].to_string())
}

fn extract_build_id(main_url: &str) -> Option<String> {
	let last_seg = main_url.rsplit('/').next()?;
	let rest = last_seg.strip_prefix("main-")?;
	let build = rest.split('-').next()?;
	(!build.is_empty()).then(|| build.to_string())
}

fn find_apicore_import(main_js: &str) -> Option<String> {
	let pos = main_js.find("./apicore-")?;
	let after = &main_js[pos..];
	let end = after.find(".js")?;
	Some(after[..end + 3].to_string())
}

fn rewrite_apicore(source: &str) -> String {
	let mut out = String::with_capacity(source.len());

	// Replace `import{n as A,r as B,t as C}from"...";` without regex.
	let mut cursor = 0;
	while let Some(rel_imp) = source[cursor..].find("import{") {
		let imp_start = cursor + rel_imp;
		if let Some(rel_brace) = source[imp_start..].find('}') {
			let brace_end = imp_start + rel_brace;
			let after_brace = source[brace_end + 1..].trim_start();
			if let Some(after_from) = after_brace.strip_prefix("from") {
				let after_from_trimmed = after_from.trim_start();
				if let Some(quote) = after_from_trimmed.bytes().next().filter(|&b| b == b'"' || b == b'\'')
					&& let Some(close_q) = after_from_trimmed[1..].find(quote as char)
				{
					let specifiers = &source[imp_start + 7..brace_end];
					let mut bindings = Vec::new();
					for part in specifiers.split(',') {
						let mut tokens = part.split(" as ");
						if let (Some(orig), Some(alias)) = (tokens.next(), tokens.next()) {
							let orig = orig.trim();
							let alias = alias.trim();
							if matches!(orig, "n" | "r" | "t") && !alias.is_empty() {
								bindings.push(format!("{alias}=globalThis.__{orig}"));
							}
						}
					}
					let from_offset =
						(after_from_trimmed.as_ptr() as usize) - (source.as_ptr() as usize);
					let mut stmt_end = from_offset + 1 + close_q + 1;
					if source.as_bytes().get(stmt_end) == Some(&b';') {
						stmt_end += 1;
					}
					out.push_str(&source[cursor..imp_start]);
					if !bindings.is_empty() {
						out.push_str("const ");
						out.push_str(&bindings.join(","));
						out.push(';');
					}
					cursor = stmt_end;
					continue;
				}
			}
		}
		out.push_str(&source[cursor..imp_start + 7]);
		cursor = imp_start + 7;
	}
	out.push_str(&source[cursor..]);

	// Strip trailing `export{...};`
	if let Some(exp_pos) = out.rfind("export{")
		&& out[exp_pos..].contains('}')
	{
		out.truncate(exp_pos);
	}
	out
}

pub fn extract_site_key(base_url: &str, waf_pass: Option<&str>) -> Result<CachedSiteKey> {
	let html = fetch_text(base_url, waf_pass)?;
	let main_src = find_main_script_src(&html)
		.ok_or_else(|| error!("Could not find Comix main script"))?;
	let build = extract_build_id(&main_src).unwrap_or_else(|| "0".into());
	let main_url = resolve_script_url(base_url, &main_src);
	let main_js = fetch_text(&main_url, waf_pass)?;
	let apicore_rel = find_apicore_import(&main_js)
		.ok_or_else(|| error!("Could not find Comix apicore import"))?;
	let apicore_url = resolve_script_url(&main_url, &apicore_rel);
	let apicore_js = fetch_text(&apicore_url, waf_pass)?;
	let rewritten = rewrite_apicore(&apicore_js);

	let ctx = JsContext::new();
	let _ = ctx.eval(PRELUDE_JS);
	// Browser-only initializers later in apicore may throw in JsContext after the key initializes.
	let _ = ctx.eval(&rewritten);
	let raw = ctx
		.eval(FIND_KEY_JS)
		.map_err(|_| error!("Could not read Comix site key from JsContext"))?;
	let trimmed = raw.trim();
	if trimmed.is_empty() {
		bail!("Comix site key not found in apicore bundle");
	}
	let mut parts = trimmed.split(',');
	let kid = parts
		.next()
		.map(str::trim)
		.filter(|s| !s.is_empty())
		.ok_or_else(|| error!("Missing Comix site key id"))?
		.to_string();
	let mut material = Vec::with_capacity(MATERIAL_SIZE);
	for token in parts {
		let val: i32 = token
			.trim()
			.parse()
			.map_err(|_| error!("Invalid byte in Comix site key material"))?;
		material.push(val as u8);
	}
	let key = SiteKey::of(kid, &material)?;
	let cached = CachedSiteKey { key, build };
	settings::set_cached_site_key(Some(&cached.serialize()));
	Ok(cached)
}

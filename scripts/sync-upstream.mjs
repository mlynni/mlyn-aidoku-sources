#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { parseMihonExtension, renderMihonDoc } from "./mihon-to-aidoku.mjs";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const ROOT_DIR = path.resolve(__dirname, "..");
const EXTENSIONS_FILE = path.join(ROOT_DIR, "extensions.json");
const CACHE_DIR = path.join(ROOT_DIR, ".cache");
const SCRATCH_UPSTREAM =
	"C:/Users/user/.gemini/antigravity/brain/7e2fff51-f1df-47e0-84d7-a6443bb6fd07/scratch/upstream";

const MIHON_REPO = "https://github.com/keiyoushi/extensions-source.git";
const AIDOKU_REPO = "https://github.com/Aidoku-Community/sources.git";

const DEFAULT_GEMINI_MODELS = [
	"gemini-3.1-pro-preview",
	"gemini-3.8-flash",
	"gemini-3-flash-preview",
];

let cachedDiscoveredModels;

const MAX_CRATE_FILE_CHARS = 45_000;
const MAX_DIFF_CHARS = 100_000;
const MAX_MIHON_FILES_CHARS = 180_000;
const MAX_ERROR_CHARS = 12_000;
const MAX_COMPILE_ATTEMPTS = 3;

const FORBIDDEN_084_WEBVIEW_RE =
	/\.(eval_async|add_user_script|set_rule_list|get_cookies|delete_cookie|delete_all_cookies)\s*\(/g;

function runGit(args, cwd) {
	return execFileSync("git", args, {
		cwd,
		encoding: "utf8",
		stdio: ["ignore", "pipe", "pipe"],
	}).trim();
}

function readJson(filePath) {
	return JSON.parse(fs.readFileSync(filePath, "utf8"));
}

function writeJson(filePath, data) {
	fs.writeFileSync(filePath, JSON.stringify(data, null, "\t") + "\n", "utf8");
}

function findOrCloneRepo(cacheName, scratchName, repoUrl, branch, sparsePaths, offlineOnly) {
	const candidates = [
		path.join(CACHE_DIR, cacheName),
		path.join(SCRATCH_UPSTREAM, scratchName),
	];
	let repoDir = candidates.find((p) => fs.existsSync(path.join(p, ".git")));

	if (!repoDir) {
		if (offlineOnly) {
			throw new Error(`Upstream cache not found locally for ${cacheName}`);
		}
		repoDir = path.join(CACHE_DIR, cacheName);
		fs.mkdirSync(CACHE_DIR, { recursive: true });
		console.log(`Cloning ${repoUrl} (treeless sparse checkout)...`);
		runGit(
			[
				"clone",
				"--filter=blob:none",
				"--no-checkout",
				"--sparse",
				"--branch",
				branch,
				repoUrl,
				repoDir,
			],
			ROOT_DIR
		);
	}

	if (!offlineOnly) {
		console.log(`Fetching updates for ${cacheName}...`);
		const isShallow = runGit(["rev-parse", "--is-shallow-repository"], repoDir) === "true";
		const fetchArgs = ["fetch", "--filter=blob:none"];
		if (isShallow) {
			fetchArgs.push("--unshallow");
		}
		fetchArgs.push("origin", branch);
		try {
			runGit(fetchArgs, repoDir);
		} catch {
			runGit(["fetch", "--filter=blob:none", "origin", branch], repoDir);
		}

		if (sparsePaths.length > 0) {
			runGit(["sparse-checkout", "set", "--no-cone", ...sparsePaths], repoDir);
		}
		runGit(["checkout", "-B", branch, "FETCH_HEAD"], repoDir);
	}

	return repoDir;
}

function getLatestCommitForPath(repoDir, relPath) {
	const sha = runGit(["log", "-n", "1", "--format=%H", "--", relPath], repoDir);
	if (sha) return sha;
	return runGit(["rev-parse", "HEAD"], repoDir);
}

function getChangedFilesBetween(repoDir, oldCommit, newCommit, relPath) {
	if (!oldCommit || oldCommit === newCommit) return [];
	try {
		runGit(["cat-file", "-e", `${oldCommit}^{commit}`], repoDir);
	} catch {
		try {
			runGit(["fetch", "--filter=blob:none", "origin", oldCommit], repoDir);
		} catch {
			return [relPath];
		}
	}
	const out = runGit(["diff", "--name-only", oldCommit, newCommit, "--", relPath], repoDir);
	return out
		.split(/\r?\n/)
		.map((s) => s.trim())
		.filter(Boolean);
}

function updateMirrorSettings(crateDir, mihonMirrors) {
	const settingsPath = path.join(crateDir, "res", "settings.json");
	if (!fs.existsSync(settingsPath) || !Array.isArray(mihonMirrors) || mihonMirrors.length === 0) {
		return false;
	}

	const settings = readJson(settingsPath);
	if (!Array.isArray(settings)) return false;

	let changed = false;
	const walk = (items) => {
		for (const item of items) {
			if (!item || typeof item !== "object") continue;
			if (item.key === "mirror" && item.type === "select" && Array.isArray(item.values)) {
				const missing = mihonMirrors.filter((u) => !item.values.includes(u));
				if (missing.length > 0) {
					item.values.push(...missing);
					if (Array.isArray(item.titles)) {
						for (const u of missing) {
							item.titles.push(u.replace(/^https?:\/\//, "").replace(/\/+$/, ""));
						}
					}
					changed = true;
				}
			}
			if (Array.isArray(item.items)) {
				walk(item.items);
			}
		}
	};
	walk(settings);

	if (changed) {
		writeJson(settingsPath, settings);
	}
	return changed;
}

function syncBaseUrls(crateDir, info) {
	const sourceJsonPath = path.join(crateDir, "res", "source.json");
	if (!fs.existsSync(sourceJsonPath)) return false;

	const srcManifest = readJson(sourceJsonPath);
	const manifestInfo = srcManifest.info || {};
	let changed = false;

	const mihonPrimary = info.baseUrl;
	const mihonMirrors = Array.isArray(info.mirrors) && info.mirrors.length > 0 ? info.mirrors : [];

	let previousUrl = null;
	if (typeof manifestInfo.url === "string" && mihonPrimary && manifestInfo.url !== mihonPrimary) {
		previousUrl = manifestInfo.url;
		manifestInfo.url = mihonPrimary;
		changed = true;
	} else if (Array.isArray(manifestInfo.urls) && mihonMirrors.length > 0) {
		const missing = mihonMirrors.filter((u) => !manifestInfo.urls.includes(u));
		if (missing.length > 0) {
			previousUrl = manifestInfo.urls[0];
			manifestInfo.urls = Array.from(new Set([...manifestInfo.urls, ...missing]));
			changed = true;
		}
	}

	if (changed) {
		srcManifest.info = manifestInfo;
		writeJson(sourceJsonPath, srcManifest);

		if (previousUrl && mihonPrimary) {
			const rustDir = path.join(crateDir, "src");
			if (fs.existsSync(rustDir)) {
				for (const f of fs.readdirSync(rustDir)) {
					if (!f.endsWith(".rs")) continue;
					const rp = path.join(rustDir, f);
					const code = fs.readFileSync(rp, "utf8");
					const updated = code.replaceAll(`"${previousUrl}"`, `"${mihonPrimary}"`);
					if (updated !== code) {
						fs.writeFileSync(rp, updated, "utf8");
						console.log(`  [url] Updated "${previousUrl}" -> "${mihonPrimary}" in src/${f}`);
					}
				}
			}
		}
	}

	if (updateMirrorSettings(crateDir, mihonMirrors)) {
		console.log(`  [mirror] Synced mirror options in res/settings.json`);
		changed = true;
	}

	return changed;
}

function collectFilesMap(baseDir, filterFn, prefix = "") {
	const files = {};
	if (!fs.existsSync(baseDir)) return files;

	const walk = (dir) => {
		const entries = fs.readdirSync(dir, { withFileTypes: true }).sort((a, b) =>
			a.name.localeCompare(b.name)
		);
		for (const item of entries) {
			const full = path.join(dir, item.name);
			if (item.isDirectory()) {
				walk(full);
				continue;
			}
			if (!item.isFile()) continue;
			const rel = path.relative(baseDir, full).replace(/\\/g, "/");
			if (!filterFn(item.name, rel)) continue;
			const key = prefix ? `${prefix}/${rel}` : rel;
			files[key] = fs.readFileSync(full, "utf8");
		}
	};

	walk(baseDir);
	return files;
}

function collectCrateFiles(crateDir) {
	const files = {};
	for (const sub of ["src", "res", "assets"]) {
		const subFiles = collectFilesMap(
			path.join(crateDir, sub),
			(name) => !/\.(png|jpg|jpeg|webp|gif|aix|wasm)$/i.test(name),
			sub
		);
		Object.assign(files, subFiles);
	}
	return files;
}

function collectMihonFiles(mihonDir) {
	return collectFilesMap(mihonDir, (name) => {
		const lower = name.toLowerCase();
		return (
			lower.endsWith(".kt") ||
			lower.endsWith(".kts") ||
			lower.endsWith(".js") ||
			lower === "build.gradle" ||
			lower === "build.gradle.kts"
		);
	});
}

function formatMihonFilesBlock(mihonFiles, maxTotalChars) {
	const sorted = Object.entries(mihonFiles).sort(([a], [b]) => {
		const rank = (rel) => {
			if (rel.endsWith(".kt") || rel.endsWith(".js")) return 0;
			if (rel.toLowerCase().includes("gradle")) return 1;
			return 2;
		};
		return rank(a) - rank(b) || a.localeCompare(b);
	});

	let total = 0;
	const blocks = [];
	const omitted = [];

	for (const [rel, content] of sorted) {
		const ext = rel.endsWith(".js") ? "javascript" : rel.endsWith(".kt") ? "kotlin" : "";
		const block = `### ${rel}\n\`\`\`${ext}\n${content}\n\`\`\``;
		if (total + block.length > maxTotalChars) {
			if (total === 0) {
				const sliceLen = Math.max(1000, maxTotalChars - rel.length - 64);
				blocks.push(
					`### ${rel} (truncated)\n\`\`\`${ext}\n${content.slice(0, sliceLen)}\n\`\`\``
				);
				total = maxTotalChars;
			} else {
				omitted.push(rel);
			}
			continue;
		}
		blocks.push(block);
		total += block.length + 2;
	}

	if (omitted.length > 0) {
		blocks.push(`_Omitted due to context budget: ${omitted.join(", ")}_`);
	}

	return blocks.join("\n\n");
}

function isBefore090(versionStr) {
	const parts = String(versionStr || "0.7.1")
		.trim()
		.split(".")
		.map((n) => Number.parseInt(n, 10) || 0);
	const major = parts[0] ?? 0;
	const minor = parts[1] ?? 0;
	return major === 0 && minor < 9;
}

function checkAidoku084Compat(entry, crateDir) {
	const errors = [];

	const resFiles = collectFilesMap(path.join(crateDir, "res"), (name) =>
		name.toLowerCase().endsWith(".json")
	);
	for (const [rel, raw] of Object.entries(resFiles)) {
		try {
			JSON.parse(raw);
		} catch (err) {
			errors.push(`Invalid JSON in res/${rel}: ${err.message}`);
		}
	}

	if (isBefore090(entry.minAppVersion || "0.7.1")) {
		const rustFiles = collectFilesMap(path.join(crateDir, "src"), (name) =>
			name.toLowerCase().endsWith(".rs")
		);
		let usesDynamicSettings = false;
		let usesLoginSetting = false;

		for (const [rel, code] of Object.entries(rustFiles)) {
			const matches = code.match(FORBIDDEN_084_WEBVIEW_RE);
			if (matches && matches.length > 0) {
				const unique = Array.from(new Set(matches.map((m) => m.replace(/^\.|\s*\($/g, ""))));
				errors.push(
					`src/${rel} calls Aidoku 0.9.0-only WebView method(s) [${unique.join(", ")}] while minAppVersion=${entry.minAppVersion || "0.7.1"}`
				);
			}
			if (/\bDynamicSettings\b/.test(code)) usesDynamicSettings = true;
			if (/\bLoginSetting\b/.test(code)) usesLoginSetting = true;
		}

		const settingsJson = resFiles["settings.json"];
		if (settingsJson && /"type"\s*:\s*"login"/.test(settingsJson)) {
			usesLoginSetting = true;
		}
		if (usesDynamicSettings && usesLoginSetting) {
			errors.push(
				`DynamicSettings cannot be combined with LoginSetting / "type":"login" when minAppVersion < 0.9.0 (Postcard ABI breaks Settings on Aidoku 0.8.4).`
			);
		}
	}

	return { ok: errors.length === 0, errors };
}

function restoreBackups(backups) {
	for (const [abs, prev] of backups.entries()) {
		if (prev === null) {
			fs.rmSync(abs, { force: true });
		} else {
			fs.mkdirSync(path.dirname(abs), { recursive: true });
			fs.writeFileSync(abs, prev, "utf8");
		}
	}
}

function resolveSafeCrateRelPath(crateDir, relPath) {
	if (typeof relPath !== "string") return null;
	const normalized = relPath.replace(/\\/g, "/").replace(/^\.\/+/, "");
	if (
		!normalized.startsWith("src/") &&
		!normalized.startsWith("res/") &&
		!normalized.startsWith("assets/")
	) {
		return null;
	}
	if (normalized === "res/source.json") return null;
	const abs = path.resolve(crateDir, normalized);
	const crateRoot = path.resolve(crateDir) + path.sep;
	if (!abs.startsWith(crateRoot)) return null;
	return { relPath: normalized, abs };
}

function parseGeminiJson(rawText) {
	try {
		return JSON.parse(rawText);
	} catch {
		const match = rawText.match(/```(?:json)?\s*([\s\S]*?)```/i);
		if (match) {
			return JSON.parse(match[1].trim());
		}
		throw new Error("invalid-gemini-json");
	}
}

function rankGeminiModel(id) {
	const verMatch = id.match(/^gemini-(\d+)(?:\.(\d+))?/i);
	const versionScore = verMatch
		? Number(verMatch[1]) * 100 + Number(verMatch[2] ?? 0)
		: 0;
	const isLite = /(?:^|-)lite\b/i.test(id) ? 1 : 0;
	const tier = /(?:^|-)pro\b/i.test(id) ? 0 : /(?:^|-)flash\b/i.test(id) ? 1 : 2;
	const isUnstable = /(preview|exp)/i.test(id) ? 1 : 0;
	return { versionScore, isLite, tier, isUnstable };
}

async function discoverGeminiModels(apiKey) {
	if (cachedDiscoveredModels !== undefined) {
		return cachedDiscoveredModels;
	}

	const excludedRe =
		/(embed|imagen|veo|tts|audio|live|robotics|computer-use|image|vision|aqa|gemma|deep-research|nano)/i;

	try {
		const url = `https://generativelanguage.googleapis.com/v1beta/models?key=${encodeURIComponent(apiKey)}&pageSize=100`;
		const resp = await fetch(url, { signal: AbortSignal.timeout(20_000) });
		if (!resp.ok) {
			cachedDiscoveredModels = null;
			return null;
		}
		const data = await resp.json();
		const candidates = [];
		for (const m of data?.models || []) {
			if (
				!Array.isArray(m?.supportedGenerationMethods) ||
				!m.supportedGenerationMethods.includes("generateContent") ||
				typeof m?.name !== "string" ||
				!m.name.startsWith("models/gemini-")
			) {
				continue;
			}
			const id = m.name.slice("models/".length);
			if (excludedRe.test(id)) continue;
			candidates.push(id);
		}

		const ranked = Array.from(new Set(candidates))
			.sort((a, b) => {
				const ra = rankGeminiModel(a);
				const rb = rankGeminiModel(b);
				return (
					rb.versionScore - ra.versionScore ||
					ra.isLite - rb.isLite ||
					ra.tier - rb.tier ||
					ra.isUnstable - rb.isUnstable ||
					a.localeCompare(b)
				);
			})
			.slice(0, 6);

		if (ranked.length > 0) {
			console.log(`  [ai] Discovered models: ${ranked.join(", ")}`);
			cachedDiscoveredModels = ranked;
		} else {
			cachedDiscoveredModels = null;
		}
		return cachedDiscoveredModels;
	} catch {
		cachedDiscoveredModels = null;
		return null;
	}
}

async function callGemini(apiKey, contents, pinnedModel = null) {
	const envModel = process.env.GEMINI_MODEL?.trim();
	const models = pinnedModel
		? [pinnedModel]
		: envModel
			? [envModel]
			: (await discoverGeminiModels(apiKey)) || DEFAULT_GEMINI_MODELS;

	let lastErr = null;
	let lastStatus = "error";
	const attempts = [];

	for (const model of models) {
		const endpoint = `https://generativelanguage.googleapis.com/v1beta/models/${model}:generateContent?key=${encodeURIComponent(apiKey)}`;
		const delays = [5000, 10000];

		for (let retry = 0; retry < 3; retry++) {
			let attempt;
			try {
				attempt = await fetch(endpoint, {
					method: "POST",
					headers: { "Content-Type": "application/json" },
					signal: AbortSignal.timeout(180_000),
					body: JSON.stringify({
						contents,
						generationConfig: {
							temperature: 0.1,
							responseMimeType: "application/json",
							maxOutputTokens: 65536,
						},
					}),
				});
			} catch (fetchErr) {
				lastStatus = "fetch";
				lastErr = fetchErr.message;
				if (retry < 2) {
					await new Promise((r) => setTimeout(r, delays[retry]));
					continue;
				}
				attempts.push(`${model}:fetch`);
				break;
			}

			if (attempt.ok) {
				const data = await attempt.json();
				const rawText = (data?.candidates?.[0]?.content?.parts || [])
					.map((p) => p?.text || "")
					.join("")
					.trim();
				if (!rawText) {
					return { ok: false, reason: "empty-gemini-response" };
				}
				return { ok: true, model, rawText };
			}

			const errText = await attempt.text();
			lastStatus = String(attempt.status);
			lastErr = errText.slice(0, 200);
			if (attempt.status === 503 && retry < 2) {
				await new Promise((r) => setTimeout(r, delays[retry]));
				continue;
			}
			attempts.push(`${model}:${attempt.status}`);
			if (attempt.status !== 404 && attempt.status !== 429 && attempt.status !== 503) {
				return {
					ok: false,
					reason: `gemini-http-${attempt.status} [${attempts.join(", ")}]: ${lastErr}`,
				};
			}
			break;
		}
	}

	return {
		ok: false,
		reason: attempts.length
			? `gemini-http-${lastStatus} [${attempts.join(", ")}]: ${lastErr || ""}`
			: "gemini-no-available-model",
	};
}

async function applyGeminiPort(entry, crateDir, mihonDir, diffText, forced = false) {
	const apiKey = process.env.GEMINI_API_KEY;
	if (!apiKey) {
		return { applied: false, reason: "no-api-key" };
	}

	const mihonFiles = collectMihonFiles(mihonDir);
	if (!forced && !diffText.trim() && Object.keys(mihonFiles).length === 0) {
		return { applied: false, reason: "empty-diff" };
	}

	const currentFiles = collectCrateFiles(crateDir);
	const filesBlock = Object.entries(currentFiles)
		.map(([rel, content]) => {
			const clipped =
				content.length > MAX_CRATE_FILE_CHARS
					? `${content.slice(0, MAX_CRATE_FILE_CHARS)}\n/* ... truncated ... */`
					: content;
			return `### ${rel}\n\`\`\`\n${clipped}\n\`\`\``;
		})
		.join("\n\n");

	const mihonBlock = formatMihonFilesBlock(mihonFiles, MAX_MIHON_FILES_CHARS);
	const diffBlock = diffText.trim()
		? `Upstream Mihon git diff for ${entry.mihonPath}:\n\`\`\`diff\n${diffText.slice(0, MAX_DIFF_CHARS)}\n\`\`\``
		: `Forced sync requested against current Mihon HEAD (${entry.mihonPath}); compare full Mihon source files below with the current Aidoku crate.`;

	const prompt = `You are porting an upstream Mihon extension (${entry.mihonPath}) to an existing Aidoku Rust (aidoku v0.3.0 / #![no_std], Rust edition 2024) extension crate "${entry.id}" (minAppVersion: "${entry.minAppVersion || "0.7.1"}").

${diffBlock}

Full current Mihon source files (${entry.mihonPath}):
${mihonBlock}

Current Aidoku crate files:
${filesBlock}

Aidoku compatibility & Rust constraints (CRITICAL):
- Target is #![no_std] Rust edition 2024 (use aidoku::alloc::{...}; import aidoku::alloc::string::ToString when calling .to_string()).
- aidoku::AidokuError derives Debug ONLY and does NOT implement Display or ToString! Always format it with \`format!("{e:?}")\`, NEVER \`e.to_string()\`.
- aidoku::imports::js::JsContext::new() and ctx.eval(&str) ARE available on Aidoku 0.8.4.
- If minAppVersion < 0.9.0: NEVER combine DynamicSettings with LoginSetting / "type":"login" (Postcard ABI breaks Settings on Aidoku 0.8.4), and NEVER call 0.9.0-only WebView methods (eval_async, add_user_script, set_rule_list, get_cookies, delete_cookie, delete_all_cookies); use WebView::new(), load(), and eval() polling.
- Never alter res/source.json (the sync script bumps version and syncs URLs separately).

Porting policy:
- Preserve existing working crate features (mirror switcher, settings, custom descramblers, WebView fallback) while integrating upstream API endpoints, request signing, decryption, headers, filters, and fallback hosts.
- Do NOT skip meaningful upstream improvements or API rewrites just because a WebView fallback already exists.
- Return \`{ "needsUpdate": false, "summary": "brief reason", "files": {} }\` ONLY if the Mihon changes are strictly Android UI/Gradle boilerplate or already fully implemented with equivalent functionality in the current crate.
- Otherwise return ONLY valid JSON containing complete updated file contents for ONLY the files in src/, res/, or assets/ that need adding or updating (never include res/source.json), plus optional \`deleteFiles\` for obsolete files:
  { "needsUpdate": true, "summary": "what changed", "files": { "src/lib.rs": "full updated file content" }, "deleteFiles": [] }`;

	const contents = [{ role: "user", parts: [{ text: prompt }] }];
	const backups = new Map();
	let pinnedModel = null;
	let finalSummary = "Gemini auto-ported changes";

	try {
		for (let attempt = 0; attempt < MAX_COMPILE_ATTEMPTS; attempt++) {
			const res = await callGemini(apiKey, contents, pinnedModel);
			if (!res.ok) {
				restoreBackups(backups);
				return { applied: false, reason: res.reason };
			}
			pinnedModel = res.model;

			let parsed;
			try {
				parsed = parseGeminiJson(res.rawText);
			} catch {
				restoreBackups(backups);
				return { applied: false, reason: "invalid-gemini-json" };
			}

			if (attempt === 0 && !parsed?.needsUpdate) {
				return { applied: false, reason: parsed?.summary || "no-rust-changes-needed" };
			}

			if (parsed?.summary) {
				finalSummary = String(parsed.summary);
			}

			let touchedThisTurn = false;
			if (parsed?.files && typeof parsed.files === "object") {
				for (const [rawRel, newContent] of Object.entries(parsed.files)) {
					if (typeof newContent !== "string") continue;
					const resolved = resolveSafeCrateRelPath(crateDir, rawRel);
					if (!resolved) continue;
					if (!backups.has(resolved.abs)) {
						backups.set(
							resolved.abs,
							fs.existsSync(resolved.abs) ? fs.readFileSync(resolved.abs, "utf8") : null
						);
					}
					fs.mkdirSync(path.dirname(resolved.abs), { recursive: true });
					fs.writeFileSync(resolved.abs, newContent, "utf8");
					touchedThisTurn = true;
				}
			}

			if (Array.isArray(parsed?.deleteFiles)) {
				for (const rawRel of parsed.deleteFiles) {
					const resolved = resolveSafeCrateRelPath(crateDir, rawRel);
					if (!resolved || !fs.existsSync(resolved.abs)) continue;
					if (!backups.has(resolved.abs)) {
						backups.set(resolved.abs, fs.readFileSync(resolved.abs, "utf8"));
					}
					fs.rmSync(resolved.abs, { force: true });
					touchedThisTurn = true;
				}
			}

			if (backups.size === 0 || (attempt > 0 && !touchedThisTurn)) {
				restoreBackups(backups);
				return { applied: false, reason: "no-applicable-files" };
			}

			const checkErrors = [];
			const compat = checkAidoku084Compat(entry, crateDir);
			if (!compat.ok) {
				checkErrors.push(...compat.errors);
			}

			try {
				execFileSync("cargo", ["check", "--target", "wasm32-unknown-unknown"], {
					cwd: crateDir,
					encoding: "utf8",
					stdio: ["ignore", "pipe", "pipe"],
				});
			} catch (compileErr) {
				if (compileErr?.code !== "ENOENT") {
					const stderr = String(compileErr.stderr || compileErr.message || "");
					checkErrors.push(stderr);
				}
			}

			if (checkErrors.length === 0) {
				return { applied: true, summary: `${finalSummary} [${pinnedModel}]` };
			}

			const errorText = checkErrors.join("\n\n").slice(0, MAX_ERROR_CHARS);
			console.log(
				`  [ai] ${entry.id}: attempt ${attempt + 1}/${MAX_COMPILE_ATTEMPTS} failed check (${pinnedModel}); ` +
					(attempt + 1 < MAX_COMPILE_ATTEMPTS ? "requesting fix..." : "rolling back.")
			);

			if (attempt + 1 < MAX_COMPILE_ATTEMPTS) {
				contents.push({ role: "model", parts: [{ text: res.rawText }] });
				contents.push({
					role: "user",
					parts: [
						{
							text:
								`Your previous output failed verification/compilation:\n\`\`\`\n${errorText}\n\`\`\`\n` +
								`Fix ALL errors above and return ONLY valid JSON in the exact format \`{ "needsUpdate": true, "summary": "...", "files": { "src/...": "complete fixed file content" } }\` containing ONLY the modified/added files.`,
						},
					],
				});
				continue;
			}

			restoreBackups(backups);
			return {
				applied: false,
				reason: `compile-check-failed: ${errorText.slice(0, 300)}`,
			};
		}

		restoreBackups(backups);
		return { applied: false, reason: "compile-check-failed: exceeded attempts" };
	} catch (err) {
		restoreBackups(backups);
		return { applied: false, reason: err.message };
	}
}

function bumpCrateVersion(crateDir, entry) {
	const sourceJsonPath = path.join(crateDir, "res", "source.json");
	const srcManifest = readJson(sourceJsonPath);
	const currentVersion = Number(srcManifest?.info?.version ?? entry.version ?? 1);
	const nextVersion = currentVersion + 1;
	srcManifest.info.version = nextVersion;
	writeJson(sourceJsonPath, srcManifest);

	entry.version = nextVersion;
	entry.baseURL =
		srcManifest.info.url ||
		(Array.isArray(srcManifest.info.urls) ? srcManifest.info.urls[0] : entry.baseURL);
	entry.packageFile = `${entry.id}-v${nextVersion}.aix`;
	entry.iconFile = `${entry.id}-v${nextVersion}.png`;
	return nextVersion;
}

function writeStepSummary(lines, initCommits) {
	const summaryPath = process.env.GITHUB_STEP_SUMMARY;
	if (!summaryPath) return;
	const items =
		lines.length > 0
			? lines.map((line) => `- ${line}`)
			: [initCommits ? "- Initialized upstream commit SHAs." : "- All watched extensions are up to date."];
	const md = `## Upstream Sync Summary\n\n${items.join("\n")}\n\n`;
	fs.appendFileSync(summaryPath, md, "utf8");
}

async function main() {
	const args = process.argv.slice(2);
	const initCommits = args.includes("--init-commits");
	const dryRun = args.includes("--dry-run");
	const includeAll = args.includes("--all");
	const force = args.includes("--force");
	const idIdx = args.indexOf("--id");
	const rawTargetId = idIdx !== -1 ? args[idIdx + 1]?.trim() || "" : "";
	const targetId = rawTargetId.replace(/^id\s*:\s*/i, "").trim() || null;

	if (force && !targetId) {
		throw new Error("--force requires --id <extension-id> to avoid burning API quota on all extensions.");
	}

	if (!fs.existsSync(EXTENSIONS_FILE)) {
		throw new Error("extensions.json not found");
	}

	const registry = readJson(EXTENSIONS_FILE);
	const allSources = registry.sources || [];
	let watchedSources;
	if (targetId) {
		const needle = targetId.toLowerCase();
		const exact = allSources.filter((entry) => entry.id.toLowerCase() === needle);
		watchedSources =
			exact.length > 0
				? exact
				: allSources.filter(
						(entry) =>
							(entry.name || "").toLowerCase() === needle ||
							entry.id.split(".").slice(1).join(".").toLowerCase() === needle
					);
	} else {
		watchedSources = allSources.filter((entry) =>
			includeAll || initCommits ? true : entry.autoSync === true
		);
	}

	if (targetId && watchedSources.length === 0) {
		throw new Error(`No extension found matching --id "${rawTargetId}"`);
	}

	console.log(
		`Checking ${watchedSources.length} of ${registry.sources.length} extension(s)` +
			(force ? " [force]" : "") +
			(dryRun ? " [dry-run]" : "") +
			"..."
	);

	const mihonPaths = Array.from(
		new Set(watchedSources.map((s) => s.mihonPath).filter(Boolean))
	);
	const aidokuPaths = Array.from(
		new Set(
			watchedSources
				.filter((s) => !s.customRust && s.aidokuPath)
				.map((s) => s.aidokuPath)
		)
	);

	const keiyoushiRepo = findOrCloneRepo(
		"keiyoushi",
		"keiyoushi",
		MIHON_REPO,
		"main",
		mihonPaths,
		initCommits
	);
	const aidokuRepo = findOrCloneRepo(
		"aidoku-community-main",
		"aidoku-community-main",
		AIDOKU_REPO,
		"main",
		[...aidokuPaths, "templates/mangabox", "templates/madara"],
		initCommits
	);

	let registryDirty = false;
	const summaryLines = [];

	for (const entry of watchedSources) {
		const crateDir = path.join(ROOT_DIR, entry.aidokuPath);
		const aidokuExists =
			!entry.customRust &&
			fs.existsSync(path.join(aidokuRepo, "sources", entry.id, "Cargo.toml"));

		const latestMihon = entry.mihonPath
			? getLatestCommitForPath(keiyoushiRepo, entry.mihonPath)
			: null;
		const latestAidoku = aidokuExists
			? getLatestCommitForPath(aidokuRepo, `sources/${entry.id}`)
			: null;

		if (initCommits) {
			if (entry.lastMihonCommit !== latestMihon) {
				entry.lastMihonCommit = latestMihon;
				registryDirty = true;
			}
			if (latestAidoku && entry.lastAidokuCommit !== latestAidoku) {
				entry.lastAidokuCommit = latestAidoku;
				registryDirty = true;
			}
			continue;
		}

		if (!entry.mihonPath) continue;

		const changedFiles = getChangedFilesBetween(
			keiyoushiRepo,
			entry.lastMihonCommit,
			latestMihon,
			entry.mihonPath
		);

		if (changedFiles.length === 0 && !force) {
			if (latestMihon && entry.lastMihonCommit !== latestMihon) {
				entry.lastMihonCommit = latestMihon;
				registryDirty = true;
			}
			if (latestAidoku && entry.lastAidokuCommit !== latestAidoku) {
				entry.lastAidokuCommit = latestAidoku;
				registryDirty = true;
			}
			continue;
		}

		if (changedFiles.length > 0) {
			console.log(`[update] ${entry.id}: Mihon changed ${changedFiles.length} file(s)`);
		} else {
			console.log(`[force] ${entry.id}: Running forced AI sync against Mihon HEAD (${latestMihon?.slice(0, 7)})`);
		}

		const mihonDir = path.join(keiyoushiRepo, entry.mihonPath);
		const info = parseMihonExtension(mihonDir, entry.mihonPath);

		if (dryRun) {
			summaryLines.push(
				changedFiles.length > 0
					? `${entry.id}: would sync ${changedFiles.join(", ")}`
					: `${entry.id}: would force-sync full Mihon sources (${latestMihon?.slice(0, 7)})`
			);
			continue;
		}

		fs.writeFileSync(path.join(crateDir, "MIHON_SOURCE.md"), renderMihonDoc(info), "utf8");

		const urlUpdated = syncBaseUrls(crateDir, info);

		let aiResult = { applied: false, reason: "no-kt-or-js-diff" };
		const hasCodeChange = changedFiles.some((f) => f.endsWith(".kt") || f.endsWith(".js"));
		if (hasCodeChange || force) {
			let diffText = "";
			if (entry.lastMihonCommit && latestMihon && entry.lastMihonCommit !== latestMihon) {
				try {
					diffText = runGit(
						["diff", entry.lastMihonCommit, latestMihon, "--", entry.mihonPath],
						keiyoushiRepo
					);
				} catch {
					diffText = "";
				}
			}
			aiResult = await applyGeminiPort(entry, crateDir, mihonDir, diffText, force);
			console.log(`  [ai] ${entry.id}: applied=${aiResult.applied} (${aiResult.summary || aiResult.reason})`);
		}

		if (urlUpdated || aiResult.applied) {
			const newVer = bumpCrateVersion(crateDir, entry);
			summaryLines.push(
				`${entry.id} -> v${newVer} (${aiResult.applied ? aiResult.summary : "URL/mirror synced"})`
			);
		} else {
			summaryLines.push(
				`${entry.id}: updated MIHON_SOURCE.md only (${aiResult.reason})`
			);
		}

		const holdCommit =
			!aiResult.applied &&
			/^(gemini-http-|empty-gemini-response|invalid-gemini-json|compile-check-failed)/.test(
				aiResult.reason || ""
			);

		if (!holdCommit) {
			entry.lastMihonCommit = latestMihon;
			if (latestAidoku) entry.lastAidokuCommit = latestAidoku;
			registryDirty = true;
		}
	}

	if (registryDirty && !dryRun) {
		writeJson(EXTENSIONS_FILE, registry);
		console.log("Updated extensions.json");
	}

	if (summaryLines.length > 0) {
		console.log("Sync summary:\n - " + summaryLines.join("\n - "));
	} else {
		console.log(initCommits ? "Initialized upstream commit SHAs." : "All watched extensions are up to date.");
	}

	writeStepSummary(summaryLines, initCommits);
}

main().catch((err) => {
	console.error(err);
	process.exit(1);
});

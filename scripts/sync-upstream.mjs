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

function collectCrateFiles(crateDir) {
	const files = {};
	for (const sub of ["src", "res", "assets"]) {
		const dir = path.join(crateDir, sub);
		if (!fs.existsSync(dir)) continue;
		for (const f of fs.readdirSync(dir)) {
			if (f.endsWith(".png")) continue;
			const full = path.join(dir, f);
			if (!fs.statSync(full).isFile()) continue;
			files[`${sub}/${f}`] = fs.readFileSync(full, "utf8");
		}
	}
	return files;
}

async function applyGeminiPort(entry, crateDir, diffText) {
	const apiKey = process.env.GEMINI_API_KEY;
	if (!apiKey || !diffText.trim()) {
		return { applied: false, reason: apiKey ? "empty-diff" : "no-api-key" };
	}

	const currentFiles = collectCrateFiles(crateDir);
	const filesBlock = Object.entries(currentFiles)
		.map(([rel, content]) => `### ${rel}\n\`\`\`\n${content.slice(0, 12000)}\n\`\`\``)
		.join("\n\n");

	const prompt = `You are porting an upstream Mihon extension update (Kotlin/JS diff) to an existing Aidoku Rust (aidoku v0.3.0 / #![no_std]) extension crate "${entry.id}" (minAppVersion: "${entry.minAppVersion || "0.7.1"}").

Upstream Mihon diff for ${entry.mihonPath}:
\`\`\`diff
${diffText.slice(0, 35000)}
\`\`\`

Current Aidoku crate files:
${filesBlock}

Aidoku compatibility constraints (CRITICAL):
- Target is #![no_std] Rust edition 2024 (use aidoku::alloc::{...}, include aidoku::alloc::string::ToString if calling .to_string()).
- If minAppVersion < 0.9.0: NEVER combine DynamicSettings with LoginSetting (Postcard ABI mismatch on Aidoku 0.8.4), and NEVER call 0.9.0-only WebView methods (eval_async, add_user_script, set_rule_list, get_cookies, delete_cookie, delete_all_cookies); use WebView::new(), load(), and eval() polling.
- Never alter res/source.json info.version (the sync script bumps it automatically).

Task:
Determine whether the Mihon diff changes runtime behavior (API endpoints, CSS selectors, auth/headers, descrambling/crypto, URL structures, filters, embedded JS assets) that is NOT already handled cleanly by the current crate.
- If the diff is only Kotlin refactoring, build.gradle tweaks already covered, or irrelevant Android UI boilerplate, respond with:
  { "needsUpdate": false, "summary": "brief reason", "files": {} }
- Otherwise, respond with ONLY valid JSON containing the complete updated file contents for ONLY the files in src/, res/, or assets/ that need changes (never include res/source.json):
  { "needsUpdate": true, "summary": "what changed", "files": { "src/lib.rs": "full updated file content" } }`;

	const model = process.env.GEMINI_MODEL || "gemini-3.1-pro-preview";
	const endpoint = `https://generativelanguage.googleapis.com/v1beta/models/${model}:generateContent?key=${encodeURIComponent(apiKey)}`;
	const resp = await fetch(endpoint, {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify({
			contents: [{ role: "user", parts: [{ text: prompt }] }],
			generationConfig: {
				temperature: 0.1,
				responseMimeType: "application/json",
			},
		}),
	});

	if (!resp.ok) {
		const errText = await resp.text();
		return { applied: false, reason: `gemini-http-${resp.status}: ${errText.slice(0, 200)}` };
	}

	const data = await resp.json();
	const rawText = data?.candidates?.[0]?.content?.parts?.[0]?.text;
	if (!rawText) {
		return { applied: false, reason: "empty-gemini-response" };
	}

	let parsed;
	try {
		parsed = JSON.parse(rawText);
	} catch {
		return { applied: false, reason: "invalid-gemini-json" };
	}

	if (!parsed?.needsUpdate || !parsed.files || typeof parsed.files !== "object") {
		return { applied: false, reason: parsed?.summary || "no-rust-changes-needed" };
	}

	const backups = new Map();
	let wroteAny = false;
	try {
		for (const [relPath, newContent] of Object.entries(parsed.files)) {
			if (typeof newContent !== "string") continue;
			if (!relPath.startsWith("src/") && !relPath.startsWith("res/") && !relPath.startsWith("assets/")) {
				continue;
			}
			if (relPath === "res/source.json") continue;
			const abs = path.join(crateDir, relPath);
			backups.set(abs, fs.existsSync(abs) ? fs.readFileSync(abs, "utf8") : null);
			fs.mkdirSync(path.dirname(abs), { recursive: true });
			fs.writeFileSync(abs, newContent, "utf8");
			wroteAny = true;
		}

		if (!wroteAny) {
			return { applied: false, reason: "no-applicable-files" };
		}

		try {
			execFileSync("cargo", ["check", "--target", "wasm32-unknown-unknown"], {
				cwd: crateDir,
				encoding: "utf8",
				stdio: ["ignore", "pipe", "pipe"],
			});
		} catch (compileErr) {
			if (compileErr?.code !== "ENOENT") {
				for (const [abs, prev] of backups.entries()) {
					if (prev === null) fs.rmSync(abs, { force: true });
					else fs.writeFileSync(abs, prev, "utf8");
				}
				return {
					applied: false,
					reason: `compile-check-failed: ${String(compileErr.stderr || compileErr.message).slice(0, 300)}`,
				};
			}
		}

		return { applied: true, summary: parsed.summary || "Gemini auto-ported changes" };
	} catch (err) {
		for (const [abs, prev] of backups.entries()) {
			if (prev === null) fs.rmSync(abs, { force: true });
			else fs.writeFileSync(abs, prev, "utf8");
		}
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

async function main() {
	const args = process.argv.slice(2);
	const initCommits = args.includes("--init-commits");
	const dryRun = args.includes("--dry-run");
	const includeAll = args.includes("--all");
	const idIdx = args.indexOf("--id");
	const targetId = idIdx !== -1 ? args[idIdx + 1] : null;

	if (!fs.existsSync(EXTENSIONS_FILE)) {
		throw new Error("extensions.json not found");
	}

	const registry = readJson(EXTENSIONS_FILE);
	const watchedSources = (registry.sources || []).filter((entry) => {
		if (targetId) {
			return entry.id.toLowerCase() === targetId.toLowerCase();
		}
		if (includeAll || initCommits) return true;
		return entry.autoSync === true;
	});

	if (targetId && watchedSources.length === 0) {
		throw new Error(`No extension found matching --id "${targetId}"`);
	}

	console.log(
		`Checking ${watchedSources.length} of ${registry.sources.length} extension(s)` +
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

		if (changedFiles.length === 0) {
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

		console.log(`[update] ${entry.id}: Mihon changed ${changedFiles.length} file(s)`);
		const mihonDir = path.join(keiyoushiRepo, entry.mihonPath);
		const info = parseMihonExtension(mihonDir, entry.mihonPath);

		if (dryRun) {
			summaryLines.push(`${entry.id}: would sync ${changedFiles.join(", ")}`);
			continue;
		}

		fs.writeFileSync(path.join(crateDir, "MIHON_SOURCE.md"), renderMihonDoc(info), "utf8");

		const urlUpdated = syncBaseUrls(crateDir, info);

		let aiResult = { applied: false, reason: "no-kt-or-js-diff" };
		const hasCodeChange = changedFiles.some((f) => f.endsWith(".kt") || f.endsWith(".js"));
		if (hasCodeChange) {
			const diffText = runGit(
				["diff", entry.lastMihonCommit, latestMihon, "--", entry.mihonPath],
				keiyoushiRepo
			);
			aiResult = await applyGeminiPort(entry, crateDir, diffText);
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

		const isTransientAiError =
			!aiResult.applied &&
			/^(gemini-http-|empty-gemini-response|invalid-gemini-json)/.test(aiResult.reason || "");

		if (!isTransientAiError) {
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
}

main().catch((err) => {
	console.error(err);
	process.exit(1);
});

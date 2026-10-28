#!/usr/bin/env node
// 依赖漏洞判据：锁里有「有修复版」的已知漏洞就红。推之前跑（`push-candidate.sh` 调它），要联网、不进 quick-check。
//
// 用法：node tests/scripts/dep-advisories.mjs [--root <仓根>] [--rev <提交>]
//   --rev 给了 ⇒ 锁与豁免表从那个提交里读（`git show <提交>:<路径>`，推送前判的是要推的那个提交），不读工作树。
//   读 <仓根> 下 src/backend/Cargo.lock · src/frontend/shell/Cargo.lock · package-lock.json 里从 registry 来的包，
//   批量问 OSV（`POST /v1/querybatch`，不要钥匙），命中的公告再逐条取详情（`GET /v1/vulns/<编号>`）看有没有修复版。
//   往外只发包名、版本与公告编号，仓里别的东西一样不发。
// 判：
//   红     随产品发的包（两份 Cargo.lock 全部 · package-lock 里不是 dev 的）有修复版、且豁免表里没登 —— 退 1
//   红     豁免表里有一行没命中任何公告（死行，删掉）—— 退 1
//   只印   不再维护（RustSec informational = unmaintained 一类）· 无修复版 · 开发依赖
//   判不了 OSV 连接失败 ·回的不是 200 · 回的形状不对 · 条数对不上 · 哪份锁一个包都没读出来 —— 退 2，不当通过
//   末行恒是 `DEP-ADVISORIES: OK` / `DEP-ADVISORIES: 红 <n> 条` / `DEP-ADVISORIES: 判不了 …`。
// 豁免表：本目录 `dep-advisories-waived.txt`，一行一条「包 编号 理由」（编号写公告本号或它的别名都认）。
// 环境变量：OSV_API（默认 https://api.osv.dev，测试指本地假 OSV）· OSV_TIMEOUT_MS 每个请求的兜底期限（默认 60000）。
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

export const LOCKS = [
  { path: "src/backend/Cargo.lock", ecosystem: "crates.io" },
  { path: "src/frontend/shell/Cargo.lock", ecosystem: "crates.io" },
  { path: "package-lock.json", ecosystem: "npm" },
];
export const WAIVERS = "tests/scripts/dep-advisories-waived.txt";
const BATCH = 500;
const PARALLEL = 8;

/** Cargo.lock 里从 crates.io 来的包（`registry+` · `sparse+`）；path 与 git 的不问。 */
export function parseCargoLock(text) {
  const out = [];
  for (const block of text.split(/^\[\[package\]\]\s*$/m).slice(1)) {
    const field = (k) => block.match(new RegExp(`^${k} = "([^"]*)"`, "m"))?.[1];
    const source = field("source") ?? "";
    if (!/^(registry|sparse)\+/.test(source)) continue;
    out.push({ name: field("name"), version: field("version") });
  }
  return out;
}

/** package-lock.json（v2/v3）`packages` 里 node_modules 下的包；根、link、没版本的不问。 */
export function parsePackageLock(text) {
  const lock = JSON.parse(text);
  const out = [];
  for (const [key, meta] of Object.entries(lock.packages ?? {})) {
    const at = key.lastIndexOf("node_modules/");
    if (at < 0 || meta.link || !meta.version) continue;
    out.push({ name: key.slice(at + "node_modules/".length), version: meta.version, dev: meta.dev === true });
  }
  return out;
}

/** 豁免表：一行「包 编号 理由」，`#` 起头是注释。理由不许空。 */
export function parseWaivers(text) {
  const out = [];
  text.split("\n").forEach((raw, i) => {
    const line = raw.trim();
    if (!line || line.startsWith("#")) return;
    const m = line.match(/^(\S+)\s+(\S+)\s+(.+)$/);
    if (!m) throw new Error(`豁免表第 ${i + 1} 行要写「包 编号 理由」三样，理由不许空：${line}`);
    out.push({ pkg: m[1], id: m[2], why: m[3].trim(), line: i + 1 });
  });
  return out;
}

/** semver 比较：主版本号三段按数，有预发布的排在同号正式版前，预发布逐段比（数段按数）。 */
export function cmpVersion(a, b) {
  const split = (v) => {
    const [core, pre] = String(v).split("+")[0].split(/-(.*)/s);
    return { core: core.split(".").map((x) => Number.parseInt(x, 10) || 0), pre: pre ? pre.split(".") : [] };
  };
  const x = split(a);
  const y = split(b);
  for (let i = 0; i < Math.max(x.core.length, y.core.length); i++) {
    const d = (x.core[i] ?? 0) - (y.core[i] ?? 0);
    if (d) return Math.sign(d);
  }
  if (!x.pre.length || !y.pre.length) return Math.sign(y.pre.length) - Math.sign(x.pre.length);
  for (let i = 0; i < Math.max(x.pre.length, y.pre.length); i++) {
    const p = x.pre[i];
    const q = y.pre[i];
    if (p === undefined) return -1;
    if (q === undefined) return 1;
    const np = /^\d+$/.test(p);
    const nq = /^\d+$/.test(q);
    if (np && nq && Number(p) !== Number(q)) return Math.sign(Number(p) - Number(q));
    if (np !== nq) return np ? -1 : 1;
    if (p !== q) return p < q ? -1 : 1;
  }
  return 0;
}

/** 罩住 version 的那一段的 fixed；那一段没收口（last_affected 或开着）⇒ null。 */
export function fixedVersion(vuln, ecosystem, name, version) {
  for (const aff of vuln.affected ?? []) {
    if (aff.package?.ecosystem !== ecosystem || aff.package?.name !== name) continue;
    for (const range of aff.ranges ?? []) {
      if (range.type !== "SEMVER" && range.type !== "ECOSYSTEM") continue;
      let start = null;
      for (const ev of range.events ?? []) {
        if (ev.introduced !== undefined) {
          start = ev.introduced;
        } else if (start !== null && (ev.fixed !== undefined || ev.last_affected !== undefined)) {
          const inside = (start === "0" || cmpVersion(version, start) >= 0) &&
            (ev.fixed !== undefined ? cmpVersion(version, ev.fixed) < 0 : cmpVersion(version, ev.last_affected) <= 0);
          if (inside) return ev.fixed ?? null;
          start = null;
        }
      }
    }
  }
  return null;
}

/** 这条公告对这个包怎么算：red · waived · info（只印）· skip（撤回了）。 */
export function classify({ ecosystem, name, version, shipped, vuln, waivers }) {
  if (vuln.withdrawn) return { level: "skip" };
  const ids = new Set([vuln.id, ...(vuln.aliases ?? [])]);
  const informational = (vuln.affected ?? [])
    .filter((a) => a.package?.name === name)
    .map((a) => a.database_specific?.informational)
    .find((x) => x) ?? vuln.database_specific?.informational ?? null;
  const fixed = fixedVersion(vuln, ecosystem, name, version);
  if (informational && informational !== "unsound") return { level: "info", fixed, why: informational };
  if (!fixed) return { level: "info", fixed, why: "无修复版" };
  if (!shipped) return { level: "info", fixed, why: "开发依赖" };
  const waiver = waivers.find((w) => w.pkg === name && ids.has(w.id));
  if (waiver) return { level: "waived", fixed, waiver };
  return { level: "red", fixed };
}

class Undecidable extends Error {}

async function osv(api, path, body) {
  const timeout = Number(process.env.OSV_TIMEOUT_MS) || 60000;
  let res;
  try {
    res = await fetch(`${api}${path}`, {
      method: body ? "POST" : "GET",
      headers: body ? { "content-type": "application/json" } : {},
      body: body ? JSON.stringify(body) : undefined,
      signal: AbortSignal.timeout(timeout),
    });
  } catch (e) {
    throw new Undecidable(`OSV 连接失败（${api}${path}）：${e.cause?.code ?? e.message}`);
  }
  if (res.status !== 200) throw new Undecidable(`OSV 回 ${res.status}（${path}）`);
  try {
    return await res.json();
  } catch {
    throw new Undecidable(`OSV 回的不是 JSON（${path}）`);
  }
}

/** 一批 {ecosystem,name,version} ⇒ 每个包命中的公告编号（照 next_page_token 翻页）。 */
async function queryIds(api, pkgs) {
  const out = [];
  for (let i = 0; i < pkgs.length; i += BATCH) {
    const chunk = pkgs.slice(i, i + BATCH);
    const q = (p, page_token) => ({ package: { name: p.name, ecosystem: p.ecosystem }, version: p.version, page_token });
    const r = await osv(api, "/v1/querybatch", { queries: chunk.map((p) => q(p)) });
    if (!Array.isArray(r?.results) || r.results.length !== chunk.length) {
      throw new Undecidable(`OSV querybatch 回了 ${r?.results?.length ?? "?"} 条、问的是 ${chunk.length} 条`);
    }
    for (let j = 0; j < chunk.length; j++) {
      const ids = (r.results[j].vulns ?? []).map((v) => v.id);
      let token = r.results[j].next_page_token;
      while (token) {
        const more = await osv(api, "/v1/querybatch", { queries: [q(chunk[j], token)] });
        const m = more?.results?.[0];
        if (!m) throw new Undecidable(`OSV 翻页回的不对（${chunk[j].name}）`);
        ids.push(...(m.vulns ?? []).map((v) => v.id));
        token = m.next_page_token;
      }
      out.push(ids);
    }
  }
  return out;
}

async function fetchVulns(api, ids) {
  const out = new Map();
  const todo = [...ids];
  const worker = async () => {
    while (todo.length) {
      const id = todo.pop();
      const v = await osv(api, `/v1/vulns/${encodeURIComponent(id)}`);
      if (v?.id !== id) throw new Undecidable(`OSV 回的公告对不上（问 ${id}）`);
      out.set(id, v);
    }
  };
  await Promise.all(Array.from({ length: PARALLEL }, worker));
  return out;
}

async function main(argv) {
  const opt = (name) => {
    const at = argv.indexOf(name);
    return at >= 0 ? argv[at + 1] : undefined;
  };
  const root = resolve(opt("--root") ?? join(dirname(fileURLToPath(import.meta.url)), "..", ".."));
  const rev = opt("--rev");
  const read = (rel) => {
    if (!rev) return readFileSync(join(root, rel), "utf8");
    try {
      return execFileSync("git", ["show", `${rev}:${rel}`], { cwd: root, encoding: "utf8", maxBuffer: 1 << 28, stdio: ["ignore", "pipe", "pipe"] });
    } catch (e) {
      throw new Undecidable(`读不到 ${rev}:${rel}（${String(e.stderr ?? e.message).trim()}）`);
    }
  };
  const api = (process.env.OSV_API || "https://api.osv.dev").replace(/\/$/, "");
  const waivers = parseWaivers(read(WAIVERS));

  const pkgs = [];
  for (const lock of LOCKS) {
    const text = read(lock.path);
    const list = lock.ecosystem === "npm"
      ? parsePackageLock(text)
      : parseCargoLock(text).map((p) => ({ ...p, dev: false }));
    if (!list.length) throw new Undecidable(`${lock.path} 里一个 registry 包都没读出来（锁的写法变了？）`);
    const seen = new Set();
    for (const p of list) {
      const key = `${p.name}@${p.version}`;
      if (seen.has(key)) continue;
      seen.add(key);
      pkgs.push({ lock: lock.path, ecosystem: lock.ecosystem, name: p.name, version: p.version, shipped: !p.dev });
    }
  }
  const hits = await queryIds(api, pkgs);
  const vulns = await fetchVulns(api, new Set(hits.flat()));

  const red = [];
  const info = [];
  const waived = [];
  const usedWaivers = new Set();
  pkgs.forEach((p, i) => {
    // RUSTSEC 排前面，GHSA 与它互为别名的只算一条
    const ids = [...hits[i]].sort((a, b) => Number(b.startsWith("RUSTSEC")) - Number(a.startsWith("RUSTSEC")) || a.localeCompare(b));
    const covered = new Set();
    for (const id of ids) {
      const v = vulns.get(id);
      const names = [v.id, ...(v.aliases ?? [])];
      if (names.some((n) => covered.has(n))) continue;
      names.forEach((n) => covered.add(n));
      const c = classify({ ...p, vuln: v, waivers });
      if (c.level === "skip") continue;
      const alias = (v.aliases ?? []).filter((a) => a !== v.id).join(" · ");
      const line = `${p.lock}  ${p.name} ${p.version}  ${v.id}${alias ? `（${alias}）` : ""}` +
        `${c.fixed ? `  修在 ${c.fixed}` : ""}  ${(v.summary ?? "").trim()}`;
      if (c.level === "red") red.push(line);
      else if (c.level === "waived") {
        usedWaivers.add(c.waiver.line);
        waived.push(`${line}\n      豁免：${c.waiver.why}`);
      } else info.push(`${line}  〔${c.why}〕`);
    }
  });
  const dead = waivers.filter((w) => !usedWaivers.has(w.line));

  if (info.length) console.log(`只印（不再维护 · 无修复版 · 开发依赖）${info.length} 条：\n  ${info.join("\n  ")}`);
  if (waived.length) console.log(`豁免 ${waived.length} 条：\n  ${waived.join("\n  ")}`);
  if (red.length) console.log(`红：有修复版 ${red.length} 条（升到「修在」那一版，或在 ${WAIVERS} 登记为什么碰不到）：\n  ${red.join("\n  ")}`);
  for (const w of dead) console.log(`红：${WAIVERS} 第 ${w.line} 行（${w.pkg} ${w.id}）没命中任何公告 —— 删掉`);
  const n = red.length + dead.length;
  console.log(`问了 ${pkgs.length} 个包（${LOCKS.map((l) => l.path).join(" · ")}）`);
  console.log(n ? `DEP-ADVISORIES: 红 ${n} 条` : "DEP-ADVISORIES: OK");
  return n ? 1 : 0;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main(process.argv.slice(2)).then(
    (code) => process.exit(code),
    (e) => {
      if (e instanceof Undecidable) {
        console.log(`DEP-ADVISORIES: 判不了 —— ${e.message}`);
        process.exit(2);
      }
      console.error(e);
      console.log(`DEP-ADVISORIES: 判不了 —— ${e.message}`);
      process.exit(2);
    },
  );
}

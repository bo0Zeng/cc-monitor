// 实时那一路的字节：把一份记录逐行追加进沙箱后端盯着的那份文件，只收追加之后的帧，按帧种类与压缩形算字节。读数不进判据。
//
// node replay.mjs <后端> <样本.jsonl> <工作目录> [--batch 20] [--gap-ms 10]
//   样本的头一行先写进 <工作目录>/home 那个沙箱家目录里的同名记录，挂一个活进程（本进程）的 pidfile，后端 `-- --stream` 起在沙箱里；
//   宣告与预读到了之后，其余各行照原样每 `--batch` 行一追加、隔 `--gap-ms`，追完等 2 s 没有新帧就停。样本只读；用造的记录，不拷真记录。
// 压缩一律按「每帧 sync flush」算（SSH 一包一刷的那一形）：deflate 6（OpenSSH ≤ 10.5 的 zlib@openssh.com）· 只 Huffman
// （OpenSSH 10.6 起 sshd 关了 LZ77）· zstd 3（应用层对照；Node 没带 zstd 时不出这一格）。
// 印：一行一条 JSON —— 每种帧的帧数与字节、`line` 里不带 `record` 的与每帧都带的 `path` ＋ `session_id`、合计的几种压缩形、
// 按记录里的时刻摊成 1 s / 10 s 窗的 p50 · p99 · 最大（原样 / deflate 6）、几种改法同一把尺子（原样 / deflate 6 / 只 Huffman）。
import { spawn } from "node:child_process";
import { appendFileSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { createInterface } from "node:readline";
import zlib from "node:zlib";

const argv = process.argv.slice(2);
const [bin, sample, work] = argv;
if (!bin || !sample || !work) throw new Error("usage: replay.mjs <后端> <样本.jsonl> <工作目录> [--batch N] [--gap-ms N]");
const opt = (k, d) => {
  const i = argv.indexOf(k);
  return i >= 0 ? Number(argv[i + 1]) : d;
};
const BATCH = opt("--batch", 20);
const GAP = opt("--gap-ms", 10);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const emit = (o) => console.log(JSON.stringify(o));

const rows = readFileSync(sample, "utf8").split("\n").filter(Boolean);
const first = JSON.parse(rows.find((l) => l.includes('"sessionId"') && l.includes('"cwd"')));
rmSync(work, { recursive: true, force: true });
const home = join(work, "home");
const pdir = join(home, ".claude", "projects", "-p");
const sessions = join(home, ".claude", "sessions");
for (const d of [pdir, sessions, join(work, "tmux"), join(work, "run")]) mkdirSync(d, { recursive: true });
const path = join(pdir, `${first.sessionId}.jsonl`);
writeFileSync(path, rows[0] + "\n");
writeFileSync(join(sessions, `${process.pid}.json`), JSON.stringify({ pid: process.pid, sessionId: first.sessionId, cwd: first.cwd, startedAt: Date.now(), kind: "interactive" }));

// 每一行的末字节 → 它在记录里的时刻（`line` 帧带 `byte_offset` ＝ 那一行末尾，按它摊到时间窗上）。
const timeAtEnd = new Map();
{
  let end = 0;
  let last = 0;
  for (const l of rows) {
    end += Buffer.byteLength(l) + 1;
    let t = last;
    try {
      const ts = JSON.parse(l).timestamp;
      if (ts) t = Date.parse(ts);
    } catch {
      /* 解不出的行沿用上一行的时刻 */
    }
    if (Number.isFinite(t)) last = t;
    timeAtEnd.set(end, last);
  }
}

const env = { PATH: "/usr/bin:/bin", HOME: home, LANG: "C.UTF-8", TZ: "UTC", TMUX_TMPDIR: join(work, "tmux"), XDG_RUNTIME_DIR: join(work, "run"), RUST_LOG: "warn" };
const child = spawn(bin, ["--", "--stream"], { env, stdio: ["pipe", "pipe", "pipe"] });
child.stderr.on("data", () => {});
setTimeout(() => {
  console.error("replay.mjs: 600 秒没跑完");
  child.kill("SIGTERM");
  process.exit(1);
}, 600000).unref();

let ready = { hello: false, added: false, primed: false };
let taking = false;
let lastFrameAt = 0;
let lastT = 0;
/** 追加之后收到的帧：原样一行 ＋ 种类 ＋ 摊到哪个时刻。 */
const frames = [];
createInterface({ input: child.stdout, crlfDelay: Infinity }).on("line", (raw) => {
  let f;
  try {
    f = JSON.parse(raw);
  } catch {
    return;
  }
  if (f.kind === "hello") ready.hello = true;
  if (f.kind === "session_added" && f.sid === first.sessionId) ready.added = true;
  if (f.kind === "session_primed" && f.sid === first.sessionId) ready.primed = true;
  if (!taking || f.kind === "reply") return;
  if (f.kind === "line" && timeAtEnd.has(f.byte_offset)) lastT = timeAtEnd.get(f.byte_offset);
  frames.push({ raw, f, t: lastT });
  lastFrameAt = performance.now();
});

const s0 = performance.now();
while (!(ready.hello && ready.added && ready.primed)) {
  if (performance.now() - s0 > 60000) throw new Error("60 秒没等到宣告与预读");
  await sleep(10);
}
taking = true;
const t0 = performance.now();
for (let i = 1; i < rows.length; i += BATCH) {
  appendFileSync(path, rows.slice(i, i + BATCH).join("\n") + "\n");
  if (GAP > 0) await sleep(GAP);
}
lastFrameAt = performance.now();
while (performance.now() - lastFrameAt < 2000) await sleep(100);
emit({ name: "replayed", rows: rows.length - 1, frames: frames.length, ms: Math.round(performance.now() - t0) });
child.stdin.end();
child.kill("SIGTERM");

const kb = (n) => +(n / 1024).toFixed(1);
const bytesOf = (raw) => Buffer.byteLength(raw) + 1;

// 每种帧
const byKind = {};
for (const { raw, f } of frames) {
  const k = (byKind[f.kind] ??= { frames: 0, kb: 0 });
  k.frames++;
  k.kb += bytesOf(raw);
}
for (const k of Object.values(byKind)) k.kb = kb(k.kb);
emit({ name: "kinds", ...byKind });

// `line` 里不带 `record` 的、每帧都带的 `path` ＋ `session_id`
{
  let bare = 0;
  let bareBytes = 0;
  let idBytes = 0;
  for (const { raw, f } of frames) {
    if (f.kind !== "line") continue;
    if (f.record === undefined) {
      bare++;
      bareBytes += bytesOf(raw);
    }
    idBytes += Buffer.byteLength(`"path":${JSON.stringify(f.path)},"session_id":${JSON.stringify(f.session_id)},`);
  }
  emit({ name: "line-parts", withoutRecord: bare, withoutRecordKb: kb(bareBytes), pathAndSessionIdKb: kb(idBytes) });
}

/** 一条压缩流、每帧 sync flush：逐帧压完多少字节（与帧一一对应）。 */
async function perFrame(lines, make, flushKind) {
  const z = make();
  let got = 0;
  z.on("data", (c) => (got += c.length));
  const out = [];
  for (const l of lines) {
    const before = got;
    z.write(l + "\n");
    await new Promise((r) => z.flush(flushKind, r));
    out.push(got - before);
  }
  z.end();
  return out;
}
const forms = [
  ["deflate6", () => zlib.createDeflateRaw({ level: 6 }), zlib.constants.Z_SYNC_FLUSH],
  ["huffman", () => zlib.createDeflateRaw({ level: 6, strategy: zlib.constants.Z_HUFFMAN_ONLY }), zlib.constants.Z_SYNC_FLUSH],
];
if (typeof zlib.createZstdCompress === "function")
  forms.push(["zstd3", () => zlib.createZstdCompress({ params: { [zlib.constants.ZSTD_c_compressionLevel]: 3 } }), zlib.constants.ZSTD_e_flush]);
const sum = (a) => a.reduce((x, y) => x + y, 0);

const raws = frames.map((x) => x.raw);
const sized = { raw: raws.map(bytesOf) };
for (const [name, make, kind] of forms) sized[name] = await perFrame(raws, make, kind);
{
  const total = sum(sized.raw);
  const o = { name: "total", rawKb: kb(total) };
  for (const [name] of forms) o[`${name}Kb`] = kb(sum(sized[name]));
  for (const [name] of forms) o[`${name}Ratio`] = +(total / Math.max(1, sum(sized[name]))).toFixed(2);
  emit(o);
}

// 按记录里的时刻摊成每秒（原样 / deflate 6）
function windows(sec) {
  const per = new Map();
  frames.forEach(({ t }, i) => {
    const k = Math.floor(t / (sec * 1000));
    const w = per.get(k) ?? { raw: 0, deflate6: 0 };
    w.raw += sized.raw[i];
    w.deflate6 += sized.deflate6[i];
    per.set(k, w);
  });
  const ws = [...per.values()];
  const q = (key, p) => {
    const v = ws.map((w) => w[key] / sec).sort((a, b) => a - b);
    return v.length ? kb(v[Math.min(v.length - 1, Math.floor(p * v.length))]) : null;
  };
  emit({ name: `window-${sec}s`, windows: ws.length, rawKbPerS: { p50: q("raw", 0.5), p99: q("raw", 0.99), max: q("raw", 1) }, deflate6KbPerS: { p50: q("deflate6", 0.5), p99: q("deflate6", 0.99), max: q("deflate6", 1) } });
}
windows(1);
windows(10);

// 几种改法同一把尺子：去掉每帧的 `path` · `session_id` · 不发不带 `record` 的 `line`
const variants = [
  ["today", (f) => f],
  [
    "no-path-session-id",
    (f) => {
      if (f.kind !== "line") return f;
      const rest = { ...f };
      delete rest.path;
      delete rest.session_id;
      return rest;
    },
  ],
  ["no-bare-line", (f) => (f.kind === "line" && f.record === undefined ? null : f)],
];
for (const [name, cut] of variants) {
  const ls = frames.map(({ f }) => cut(f)).filter((f) => f !== null).map((f) => JSON.stringify(f));
  const o = { name: `variant-${name}`, frames: ls.length, rawKb: kb(sum(ls.map(bytesOf))) };
  for (const [form, make, kind] of forms.slice(0, 2)) o[`${form}Kb`] = kb(sum(await perFrame(ls, make, kind)));
  emit(o);
}
process.exit(0);

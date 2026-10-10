// 后端台架：起一个沙箱后端（白名单环境 · 临时 HOME · stdio 载体，与本机宿主同一组流模式旗标），
// 照界面的问法逐条问、量每条应答的墙钟与字节，再量空闲 CPU / 常驻内存与实时追加一行的延迟。
// 用法：node drive.mjs <后端二进制> <HOME> [--reps N] [--idle S] [--only 名,名]
//   只量全文搜索那一组（热索引用时 · 不含 / 含工具每问 · 问过含工具之后再问不含工具 · 常驻）：--only search
//   读法：按每问的进程 CPU 毫秒（cpuMs）比，不按墙钟（机器忙时墙钟抖几倍）；index 是那一问后端记的读盘账（full=整份重读几份）。
// 输出：一行一条 JSON（台架读数），最后一行是汇总。
import { spawn } from "node:child_process";
import { readFileSync, readdirSync, statSync, appendFileSync, mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { createInterface } from "node:readline";

const [bin, home] = process.argv.slice(2);
const opt = (k, d) => {
  const i = process.argv.indexOf(k);
  return i > 0 ? process.argv[i + 1] : d;
};
const REPS = Number(opt("--reps", 5));
const IDLE_S = Number(opt("--idle", 20));
const ONLY = opt("--only", "")?.split(",").filter(Boolean) ?? [];
const RELAY = opt("--relay", "");
const box = join(home, "..");
mkdirSync(join(box, "tmux"), { recursive: true });
mkdirSync(join(box, "run"), { recursive: true });

const env = {
  PATH: "/usr/bin:/bin",
  HOME: home,
  LANG: "C.UTF-8",
  TMUX_TMPDIR: join(box, "tmux"),
  XDG_RUNTIME_DIR: join(box, "run"),
  // 搜索索引那几行（后台建好 · 每问读盘的账）要看见：它们是读数的一部分。
  RUST_LOG: "warn,cc_monitor_backend::observe::search_query=debug",
  ...(RELAY ? { CCM_RELAY_PORT: RELAY } : {}),
};
const t0 = performance.now();
const child = spawn(bin, ["--", "--stream"], { env, stdio: ["pipe", "pipe", "pipe"] });
let stderr = "";
let errLine = "";
let warmAt = null;
let warmWhat = "";
let lastIndex = "";
child.stderr.on("data", (d) => {
  errLine += d;
  let i;
  while ((i = errLine.indexOf("\n")) >= 0) {
    const l = errLine.slice(0, i);
    errLine = errLine.slice(i + 1);
    if (l.includes("后台建好")) {
      warmAt = performance.now();
      warmWhat = l.replace(/^.*全文搜索索引：/, "");
      continue;
    }
    const m = l.match(/history-search index: (.*)$/);
    if (m) {
      lastIndex = m[1];
      continue;
    }
    stderr += l + "\n";
  }
});
const rl = createInterface({ input: child.stdout, crlfDelay: Infinity });
const waiting = new Map();
const frameWaiters = [];
let helloAt = null;
let frames = 0;
rl.on("line", (line) => {
  frames++;
  let f;
  try {
    f = JSON.parse(line);
  } catch {
    return;
  }
  const now = performance.now();
  if (f.kind === "hello" && helloAt === null) helloAt = now;
  if (f.kind === "reply" && waiting.has(f.id)) {
    const w = waiting.get(f.id);
    waiting.delete(f.id);
    w({ f, bytes: line.length, at: now });
    return;
  }
  for (let i = frameWaiters.length - 1; i >= 0; i--) {
    if (frameWaiters[i].test(f)) {
      frameWaiters[i].done(now);
      frameWaiters.splice(i, 1);
    }
  }
});

let nid = 0;
function call(cmd, args) {
  const id = `b${++nid}`;
  return new Promise((resolve) => {
    const s = performance.now();
    waiting.set(id, ({ f, bytes, at }) => resolve({ ms: at - s, bytes, ok: f.ok, code: f.code, data: f.data }));
    child.stdin.write(JSON.stringify({ id, cmd, args: args ?? null, within_ms: 120000 }) + "\n");
  });
}

function cpuTicks(pid) {
  // 进程（含全部线程）的 utime+stime，单位 clock tick（100/s）。
  const st = readFileSync(`/proc/${pid}/stat`, "utf8");
  const rest = st.slice(st.lastIndexOf(")") + 2).split(" ");
  return Number(rest[11]) + Number(rest[12]);
}
function mem(pid) {
  const s = readFileSync(`/proc/${pid}/status`, "utf8");
  const g = (k) => Number((s.match(new RegExp(`${k}:\\s+(\\d+)`)) || [])[1] || 0);
  return { rssMb: +(g("VmRSS") / 1024).toFixed(1), hwmMb: +(g("VmHWM") / 1024).toFixed(1), threads: g("Threads") };
}
const out = [];
function emit(o) {
  out.push(o);
  console.log(JSON.stringify(o));
}
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function bench(name, cmd, args, reps = REPS) {
  if (ONLY.length && !ONLY.includes(name)) return null;
  const ms = [];
  let last;
  const c0 = cpuTicks(child.pid);
  for (let i = 0; i < reps; i++) {
    last = await call(cmd, typeof args === "function" ? args(i) : args);
    ms.push(last.ms);
  }
  const cpu = (cpuTicks(child.pid) - c0) * 10;
  const sorted = [...ms].sort((a, b) => a - b);
  emit({
    name,
    cmd,
    first: +ms[0].toFixed(1),
    p50: +sorted[Math.floor(sorted.length / 2)].toFixed(1),
    max: +sorted[sorted.length - 1].toFixed(1),
    cpuMsPerCall: +(cpu / reps).toFixed(1),
    kb: +(last.bytes / 1024).toFixed(1),
    ok: last.ok,
    ...(last.ok ? {} : { code: last.code }),
  });
  return last;
}

// —— 开场：hello、空闲 ——
while (helloAt === null) await sleep(5);
emit({ name: "hello", ms: +(helloAt - t0).toFixed(1) });
// 起来之后的第一条应答（后台热缓存的线程正在跑时）：ping ＋ 一条读帧命令。
{
  const p = await call("ping", {});
  emit({ name: "first-ping", ms: +(performance.now() - t0).toFixed(1), replyMs: +p.ms.toFixed(1) });
  const q = await call("quota-read", {});
  emit({ name: "first-quota-read", replyMs: +q.ms.toFixed(1) });
}
await sleep(300);
emit({ name: "mem-start", ...mem(child.pid) });

// —— 清单与会话路径 ——
const list = await bench("history-list", "history-list", {}, REPS);
const rows = list?.data?.rows ?? [];
emit({ name: "history-list-rows", n: rows.length });
// 最大的那份会话（长会话）＋ 一份中等的
const projects = join(home, ".claude", "projects");
const files0 = readdirSync(projects).flatMap((d) => readdirSync(join(projects, d)).map((f) => join(projects, d, f)));
const files = files0.filter((f) => f.endsWith(".jsonl"));
files.sort((a, b) => statSync(b).size - statSync(a).size);
const big = files[0];
const mid = files[Math.floor(files.length / 2)];
const midSid = mid.slice(mid.lastIndexOf("/") + 1, -".jsonl".length);
const midCwd = JSON.parse(readFileSync(mid, "utf8").split("\n").find((l) => l.includes('"cwd"'))).cwd;
emit({ name: "files", n: files.length, bigMb: +(statSync(big).size / 1e6).toFixed(1), midKb: +(statSync(mid).size / 1e3).toFixed(0) });

// 查看窗 / agent 窗按 sid 问清单（找那一条的记录路径）
const bigSid = big.slice(big.lastIndexOf("/") + 1, -".jsonl".length);
await bench("history-list-sid", "history-list", { sid: bigSid }, REPS);
// 正在写的会话变长之后再问清单（那一份的 (长度, 时刻) 变了）
if (!ONLY.length || ONLY.includes("history-list-grow")) {
  const ms = [];
  const tplBig = readFileSync(big, "utf8").split("\n").filter(Boolean).slice(-1)[0];
  for (let i = 0; i < 5; i++) {
    appendFileSync(big, tplBig + "\n");
    ms.push((await call("history-list", {})).ms);
  }
  emit({ name: "history-list-after-append-big", ms: ms.map((x) => +x.toFixed(1)) });
}
for (const [tag, path] of [["big", big], ["mid", mid]]) {
  const facts = await bench(`history-facts-${tag}-cold`, "history-facts", { path }, 1);
  await bench(`history-facts-${tag}`, "history-facts", { path }, REPS);
  if (facts?.ok) await bench(`history-facts-${tag}-prior`, "history-facts", { path, prior: facts.data }, REPS);
  await bench(`history-index-${tag}`, "history-index", { path, offset: 0 });
  await bench(`history-turns-${tag}`, "history-turns", { path, from: 0 });
  await bench(`history-user-inputs-${tag}`, "history-user-inputs", { path, from: 0 });
  await bench(`history-page-${tag}-first`, "history-page", { path, offset: 0, seq: 0 });
  await bench(`history-find-${tag}`, "history-find", { path, query: "cursor", include_tools: false, limit: 200 });
  await bench(`history-read-${tag}`, "history-read", { path }, 2);
}
// 整份按页读（查看器打开最长的会话）
if (!ONLY.length || ONLY.includes("whole-read")) {
  const s = performance.now();
  const c0 = cpuTicks(child.pid);
  let offset = 0;
  let seq = 0;
  let pages = 0;
  for (;;) {
    const r = await call("history-page", { path: big, offset, seq });
    pages++;
    if (!r.ok || r.data.eof || r.data.next <= offset) break;
    offset = r.data.next;
    seq = r.data.nextSeq;
  }
  emit({ name: "whole-read-big", ms: +(performance.now() - s).toFixed(1), pages, cpuMs: (cpuTicks(child.pid) - c0) * 10 });
}

// —— 全文搜索那一组：后台热索引要多久 · 每问 CPU · 问过含工具之后不含工具的那一问还快不快 · 常驻 ——
if (!ONLY.length || ONLY.includes("search")) {
  while (warmAt === null) await sleep(20);
  emit({ name: "search-warm", ms: +(warmAt - t0).toFixed(0), what: warmWhat, ...mem(child.pid) });
  const ask = async (name, args, n) => {
    const ms = [];
    const cpuMs = [];
    for (let i = 0; i < n; i++) {
      const c0 = cpuTicks(child.pid);
      const r = await call("history-search", args);
      ms.push(+r.ms.toFixed(0));
      cpuMs.push((cpuTicks(child.pid) - c0) * 10);
      if (!r.ok) emit({ name, code: r.code });
    }
    await sleep(50); // 让那一问的读盘账那一行到
    emit({ name, ms, cpuMs, index: lastIndex, ...mem(child.pid) });
  };
  await ask("search-plain", { query: "游标分页" }, 3);
  await ask("search-tools", { query: "npm test", include_tools: true }, Math.max(REPS, 3));
  await ask("search-tools-other", { query: "cursor", include_tools: true }, 3);
  await ask("search-plain-after-tools", { query: "游标分页" }, 3);
}
await bench("history-search", "history-search", { query: "游标分页" }, 3);
await bench("history-search-rare", "history-search", { query: "skip-41" }, 3);
await bench("history-search-tools", "history-search", { query: "npm test", include_tools: true }, 3);
await bench("quota-read", "quota-read", {});
await bench("rotation-rules-read", "rotation-rules-read", {});
await bench("accounts-list", "accounts-list", { agent: "claude" });
await bench("accounts-sessions", "accounts-sessions", {});
await bench("tasks-list", "tasks-list", { sid: midSid });
await bench("terminals-list", "terminals-list", {});
await bench("machine-interrupts", "machine-interrupts", {});
await bench("session-interrupts", "session-interrupts", { sid: midSid });
await bench("assets-catalog", "assets-catalog", {});
await bench("ext-list", "ext-list", {});
await bench("profiles-read", "profiles-read", {});
await bench("footprint-report", "footprint-report", {}, 2);
await bench("data-report", "data-report", {}, 2);
await bench("resync", "resync", {});
await bench("files-home", "files-home", {});
await bench("files-ls-projects", "files-ls", { path: projects });
await bench("ping", "ping", {}, 20);

// —— 文件窗口问的那几条：一个两万项的目录 · 一份 20 MB 的文本 · 一棵几万项的树 ——
if (!ONLY.length || ONLY.includes("files")) {
  const fdir = join(home, "work", "wide");
  const tree = join(home, "work", "tree");
  mkdirSync(fdir, { recursive: true });
  for (let i = 0; i < 20000; i++) writeFileSync(join(fdir, `f${String(i).padStart(5, "0")}.${["rs", "ts", "md", "json", "log"][i % 5]}`), "");
  for (let d = 0; d < 200; d++) {
    const dd = join(tree, `pkg${d}`, "src", "inner");
    mkdirSync(dd, { recursive: true });
    for (let i = 0; i < 150; i++) writeFileSync(join(dd, `mod${i}.rs`), i % 50 === 0 ? `fn retry_${d}_${i}() {}\n` : "");
  }
  const bigText = join(home, "work", "big.log");
  writeFileSync(bigText, Array.from({ length: 300000 }, (_, i) => `2026-10-01T08:00:${String(i % 60).padStart(2, "0")}Z INFO step ${i} cursor=${i * 7} ok`).join("\n"));
  await bench("files-ls-20k", "files-ls", { path: fdir });
  await bench("files-stat", "files-stat", { path: bigText });
  await bench("files-read-text-20mb", "files-read-text", { path: bigText }, 3);
  await bench("files-index-rebuild-30k", "files-index-rebuild", { path: tree }, 2);
  await bench("files-find", "files-find", { query: "mod1", under: tree, limit: 200 }, 5);
  await bench("files-grep", "files-grep", { path: tree, query: "retry_" }, 3);
}

emit({ name: "mem-after-reads", ...mem(child.pid) });

// —— 读多次看内存涨不涨：长会话整份读 20 遍 ——
if (!ONLY.length || ONLY.includes("leak")) {
  const m0 = mem(child.pid);
  const rounds = Number(opt("--leak", 20));
  for (let k = 0; k < rounds; k++) {
    if (k % 20 === 19) emit({ name: `leak-round-${k + 1}`, ...mem(child.pid) });
    let offset = 0;
    let seq = 0;
    for (;;) {
      const r = await call("history-page", { path: big, offset, seq });
      if (!r.ok || r.data.eof || r.data.next <= offset) break;
      offset = r.data.next;
      seq = r.data.nextSeq;
    }
    await call("history-index", { path: big, offset: 0 });
    await call("history-search", { query: `case ${k}` });
  }
  emit({ name: "leak-whole-read", rounds, before: m0, after: mem(child.pid) });
}

// —— 实时追加：往一份会话末尾追加一行，量到 `line` 帧出来 ——
if (!ONLY.length || ONLY.includes("append")) {
  writeFileSync(join(home, ".claude", "sessions", `${process.pid}.json`), JSON.stringify({ pid: process.pid, sessionId: midSid, cwd: midCwd, startedAt: Date.now(), kind: "interactive" }));
  await sleep(800);
  const lat = [];
  const c0 = cpuTicks(child.pid);
  const tpl = readFileSync(mid, "utf8").split("\n").filter(Boolean).slice(-1)[0];
  for (let i = 0; i < 40; i++) {
    const marker = `perfmark${i}x${Date.now()}`;
    const p = new Promise((resolve) => frameWaiters.push({ test: (f) => f.kind === "line" && JSON.stringify(f).includes(marker), done: resolve }));
    const rec = JSON.parse(tpl);
    rec.uuid = marker;
    const s = performance.now();
    appendFileSync(mid, JSON.stringify(rec) + "\n");
    const at = await Promise.race([p, sleep(3000).then(() => null)]);
    lat.push(at === null ? null : at - s);
    await sleep(50);
  }
  const ok = lat.filter((x) => x !== null).sort((a, b) => a - b);
  emit({ name: "append-line", got: ok.length, of: lat.length, p50: ok.length ? +ok[Math.floor(ok.length / 2)].toFixed(1) : null, max: ok.length ? +ok[ok.length - 1].toFixed(1) : null, cpuMsTotal: (cpuTicks(child.pid) - c0) * 10 });
}

// —— 空闲 CPU ——
{
  await sleep(1000);
  const c0 = cpuTicks(child.pid);
  await sleep(IDLE_S * 1000);
  const ms = (cpuTicks(child.pid) - c0) * 10;
  emit({ name: "idle", seconds: IDLE_S, cpuMs: ms, pct: +((ms / (IDLE_S * 1000)) * 100).toFixed(2), ...mem(child.pid) });
}
child.stdin.end();
child.kill("SIGTERM");
emit({ name: "frames", n: frames, stderrTail: stderr.split("\n").filter(Boolean).slice(-5) });
process.exit(0);

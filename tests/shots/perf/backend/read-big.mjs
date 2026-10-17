// 读会话台架：一份大记录冷开 / 重启 / 追加之后再开时，后端读了多少字节、花了多久，各条冷开应答是什么。
// 记录由 `make-big.mjs` 造（占位字）。起一个沙箱后端（白名单环境 · 那个 HOME · stdio 载体），大记录挂一个活进程的 pidfile
// （本进程；小记录是历史会话），照界面的问法：宣告一到就从 0 问骨架索引（同时预读在跑），读完之后再从 0 问
// 尾段 · 轮次 · 用户输入 · 事实 · 主线外，每条翻页翻到底。
//
// 用法：
//   node read-big.mjs <后端二进制> <HOME> <输出目录> [--phase cold|warm|grown] [--append 3000] [--live 3000]
//     cold  ＝ 先删骨架目录（头一次开）· warm ＝ 骨架留着、记录没变（重启）· grown ＝ 骨架留着、起之前往大记录追加 `--append` 行
//     --live N：问完之后往大记录一口气追加 N 行，量 N 帧 `line` 出完要多久、后端花了多少 CPU
//   node read-big.mjs --compare <输出目录A> <输出目录B>
//     两趟的冷开应答逐字节比（`writers` · 事实令牌的 `rev` 每趟都不一样，比之前抹掉）；不一样的逐份印出，退出码 1。
// 输出：一行一条 JSON 读数；应答原样写进 <输出目录>/replies/<命令>.json（几页的按页一行）。
// 后端的 stderr 写进 <输出目录>/stderr.log（日志档照环境变量 READ_BIG_LOG，缺省 warn）。
// 读数里 `recordMb` 是后端进程这一趟从文件读进来的字节（/proc/<pid>/io 的 rchar，含骨架与其它小文件）；墙钟随机器负载抖，
// 开头结尾各记一次 loadavg。
import { spawn } from "node:child_process";
import { appendFileSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import os from "node:os";
import { join } from "node:path";
import { createInterface } from "node:readline";

const argv = process.argv.slice(2);
const opt = (k, d) => {
  const i = argv.indexOf(k);
  return i >= 0 ? argv[i + 1] : d;
};

if (argv[0] === "--compare") {
  const [a, b] = [argv[1], argv[2]];
  // 抹掉每趟都不一样的：活进程的 pid。
  const scrub = (s) => s.replace(/"writers":\[[0-9,]*\]/g, '"writers":[]');
  let bad = 0;
  const names = new Set([...readdirSync(join(a, "replies")), ...readdirSync(join(b, "replies"))]);
  for (const n of [...names].sort()) {
    const pa = join(a, "replies", n);
    const pb = join(b, "replies", n);
    if (!existsSync(pa) || !existsSync(pb)) {
      console.log(JSON.stringify({ file: n, same: false, why: "只有一边有" }));
      bad++;
      continue;
    }
    const [x, y] = [scrub(readFileSync(pa, "utf8")), scrub(readFileSync(pb, "utf8"))];
    const same = x === y;
    if (!same) bad++;
    console.log(JSON.stringify({ file: n, same, bytes: [x.length, y.length] }));
  }
  process.exit(bad ? 1 : 0);
}

const [bin, home, outDir] = argv;
if (!bin || !home || !outDir) throw new Error("usage: read-big.mjs <后端二进制> <HOME> <输出目录> [--phase …] [--append N] [--live N]");
const PHASE = opt("--phase", "cold");
const APPEND = Number(opt("--append", 3000));
const LIVE = Number(opt("--live", 0));
// --strace：后端经 strace 起（只记 read / pread64，带路径），停下之后按文件合计读了多少字节 —— 墙钟会被拖慢，这一趟只看字节。
const STRACE = argv.includes("--strace");
mkdirSync(join(outDir, "replies"), { recursive: true });

const projects = join(home, ".claude", "projects");
const files = readdirSync(projects)
  .flatMap((d) => readdirSync(join(projects, d)).map((f) => join(projects, d, f)))
  .filter((f) => f.endsWith(".jsonl"))
  .sort((x, y) => statSync(y).size - statSync(x).size);
const [big, small] = files;
const sidOf = (p) => p.slice(p.lastIndexOf("/") + 1, -".jsonl".length);
const cwdOf = (p) => JSON.parse(readFileSync(p, "utf8").slice(0, 65536).split("\n")[0]).cwd;

/** 往 `path` 末尾追加 `n` 行：照记录里最后一轮的样子换 uuid（`tag` 区分这一批）。 */
function append(path, n, tag) {
  const tail = readFileSync(path, "utf8").slice(-400000).split("\n").filter(Boolean).slice(-12);
  const rows = [];
  let parent = JSON.parse(tail[tail.length - 1]).uuid ?? null;
  for (let i = 0; i < n; i++) {
    const rec = JSON.parse(tail[i % tail.length]);
    if (rec.uuid) {
      rec.uuid = `${tag}-${i}-${rec.uuid.slice(-12)}`;
      rec.parentUuid = parent;
      parent = rec.uuid;
    }
    rows.push(JSON.stringify(rec));
  }
  appendFileSync(path, rows.join("\n") + "\n");
}

const emit = (o) => console.log(JSON.stringify(o));
emit({ name: "start", phase: PHASE, load: os.loadavg().map((x) => +x.toFixed(1)), bigMb: +(statSync(big).size / 1e6).toFixed(1), smallMb: +(statSync(small).size / 1e6).toFixed(1) });
if (PHASE === "cold") rmSync(join(home, ".cc-monitor", "cache", "skeleton"), { recursive: true, force: true });
if (PHASE === "grown") append(big, APPEND, "grow");

// 大记录挂一个活进程（本进程）的 pidfile，起来时宣告、预读；小记录不挂（历史会话，只经冷开那几问）。
const sessions = join(home, ".claude", "sessions");
mkdirSync(sessions, { recursive: true });
for (const f of readdirSync(sessions)) rmSync(join(sessions, f));
writeFileSync(join(sessions, `${process.pid}.json`), JSON.stringify({ pid: process.pid, sessionId: sidOf(big), cwd: cwdOf(big), startedAt: Date.now(), kind: "interactive" }));

const box = join(home, "..");
mkdirSync(join(box, "tmux"), { recursive: true });
mkdirSync(join(box, "run"), { recursive: true });
const env = { PATH: "/usr/bin:/bin", HOME: home, LANG: "C.UTF-8", TZ: "UTC", TMUX_TMPDIR: join(box, "tmux"), XDG_RUNTIME_DIR: join(box, "run"), RUST_LOG: process.env.READ_BIG_LOG ?? "warn" };
const t0 = performance.now();
const traceLog = join(outDir, "strace.log");
const child = STRACE
  ? spawn("strace", ["-f", "-qq", "-y", "-s", "0", "-e", "trace=read,pread64", "-e", "signal=none", "-o", traceLog, bin, "--", "--stream"], { env, stdio: ["pipe", "pipe", "pipe"] })
  : spawn(bin, ["--", "--stream"], { env, stdio: ["pipe", "pipe", "pipe"] });
/** 被量的那个进程：经 strace 起时是 strace 的孩子。 */
let pid = child.pid;
let stderr = "";
child.stderr.on("data", (d) => (stderr += d));

const at = {};
const waiting = new Map();
const frameWaiters = [];
let lines = 0;
let primedLines = null;
createInterface({ input: child.stdout, crlfDelay: Infinity }).on("line", (raw) => {
  let f;
  try {
    f = JSON.parse(raw);
  } catch {
    return;
  }
  const now = performance.now() - t0;
  if (f.kind === "reply" && waiting.has(f.id)) {
    const w = waiting.get(f.id);
    waiting.delete(f.id);
    w({ f, bytes: raw.length });
    return;
  }
  if (f.kind === "hello") at.hello ??= now;
  if (f.kind === "session_added" && f.sid === sidOf(big)) at.added ??= now;
  if (f.kind === "session_primed" && f.sid === sidOf(big)) {
    at.primed ??= now;
    primedLines = f.lines ?? null;
  }
  if (f.kind === "line") lines++;
  for (let i = frameWaiters.length - 1; i >= 0; i--) {
    if (frameWaiters[i]()) frameWaiters.splice(i, 1);
  }
});
const until = (test) => new Promise((resolve) => (test() ? resolve() : frameWaiters.push(() => (test() ? (resolve(), true) : false))));

let nid = 0;
function call(cmd, args) {
  const id = `r${++nid}`;
  return new Promise((resolve) => {
    waiting.set(id, resolve);
    child.stdin.write(JSON.stringify({ id, cmd, args, within_ms: 600000 }) + "\n");
  });
}
const cpuMs = () => {
  const st = readFileSync(`/proc/${pid}/stat`, "utf8");
  const rest = st.slice(st.lastIndexOf(")") + 2).split(" ");
  return (Number(rest[11]) + Number(rest[12])) * 10;
};
const rchar = () => Number(readFileSync(`/proc/${pid}/io`, "utf8").match(/rchar:\s+(\d+)/)[1]);
const mem = () => {
  const s = readFileSync(`/proc/${pid}/status`, "utf8");
  const g = (k) => Number((s.match(new RegExp(`${k}:\\s+(\\d+)`)) || [])[1] || 0);
  return { rssMb: +(g("VmRSS") / 1024).toFixed(1), hwmMb: +(g("VmHWM") / 1024).toFixed(1) };
};
const mb = (n) => +(n / 1e6).toFixed(1);

/** 一条冷开的问法：从 0 问、照 `next` 翻到底；每页原样（`data`）一行写进 replies/<名>.json。 */
async function cold(name, cmd, args, key) {
  const s = performance.now();
  const c0 = cpuMs();
  const r0 = rchar();
  const pages = [];
  let bytes = 0;
  let a = { ...args };
  for (;;) {
    const { f, bytes: b } = await call(cmd, a);
    bytes += b;
    if (!f.ok) {
      pages.push(JSON.stringify({ code: f.code }));
      break;
    }
    pages.push(JSON.stringify(f.data));
    const next = key ? f.data.next : null;
    if (next === null || next === undefined) break;
    a = { ...args, [key]: next };
  }
  writeFileSync(join(outDir, "replies", `${name}.json`), pages.join("\n") + "\n");
  const o = { name, ms: +(performance.now() - s).toFixed(1), pages: pages.length, replyMb: mb(bytes), cpuMs: cpuMs() - c0, recordMb: mb(rchar() - r0) };
  emit(o);
  return o;
}

await until(() => at.hello !== undefined);
if (STRACE) pid = Number(readFileSync(`/proc/${child.pid}/task/${child.pid}/children`, "utf8").trim().split(/\s+/)[0]);
await until(() => at.added !== undefined);
// 宣告一到就问骨架索引（界面切进来就要它），预读同时在跑。
const r0 = rchar();
const index = cold("index", "history-index", { path: big, offset: 0 }, "offset");
await until(() => at.primed !== undefined);
const atPrimed = { rchar: rchar() };
await index;
const indexDone = performance.now() - t0;
emit({ name: "timeline", helloMs: +at.hello.toFixed(0), addedMs: +at.added.toFixed(0), primedMs: +at.primed.toFixed(0), indexDoneMs: +indexDone.toFixed(0), primedLines, recordMbAtPrimed: mb(atPrimed.rchar - r0) });
await cold("tail", "history-tail", { path: big, n: 600 }, null);
await cold("turns", "history-turns", { path: big, from: 0 }, "from");
await cold("user-inputs", "history-user-inputs", { path: big, from: 0 }, "from");
await cold("facts", "history-facts", { path: big }, null);
await cold("branch", "history-branch", { path: big, from: 0 }, "from");
await cold("index-small", "history-index", { path: small, offset: 0 }, "offset");
await cold("turns-small", "history-turns", { path: small, from: 0 }, "from");
emit({ name: "after-cold", recordMbTotal: mb(rchar() - r0), cpuMsTotal: cpuMs(), ...mem() });

if (LIVE > 0) {
  const base = lines;
  const c0 = cpuMs();
  const s = performance.now();
  append(big, LIVE, "live");
  await Promise.race([until(() => lines - base >= LIVE), new Promise((r) => setTimeout(r, 60000))]);
  emit({ name: "live", appended: LIVE, got: lines - base, ms: +(performance.now() - s).toFixed(0), cpuMs: cpuMs() - c0 });
}

// 骨架在旁线程上落（几十 MB）：等它落完再停后端，不然下一趟（warm）当没有骨架、整份重扫。
{
  const dir = join(home, ".cc-monitor", "cache", "skeleton");
  const s = performance.now();
  const busy = () => existsSync(dir) && readdirSync(dir).some((f) => f.endsWith(".tmp"));
  await new Promise((r) => setTimeout(r, 300));
  while (busy() && performance.now() - s < 120000) await new Promise((r) => setTimeout(r, 100));
  emit({ name: "skeleton-settled", waitedMs: +(performance.now() - s).toFixed(0), files: existsSync(dir) ? readdirSync(dir).length : 0 });
}
writeFileSync(join(outDir, "stderr.log"), stderr);
emit({ name: "end", load: os.loadavg().map((x) => +x.toFixed(1)), ...mem() });
// 线程号 → 线程名（停之前抄下来；按线程分账用，早已退出的线程记「?」）
const comm = new Map();
if (STRACE) for (const t of readdirSync(`/proc/${pid}/task`)) comm.set(t, readFileSync(`/proc/${pid}/task/${t}/comm`, "utf8").trim());
child.stdin.end();
process.kill(pid, "SIGTERM");
if (STRACE) {
  await new Promise((r) => child.on("exit", r));
  // 按文件合计：`<pid> read(<fd></路径>, …) = <字节>`
  const per = new Map();
  // 几条线程同时在读时一次调用会被拆成两行（`read(7</路径>, <unfinished ...>` 与 `<pid> <... read resumed>…) = N`），按线程号接上。
  const open = new Map();
  for (const l of readFileSync(traceLog, "utf8").split("\n")) {
    const tid = l.split(" ", 1)[0];
    const start = l.match(/(?:read|pread64)\(\d+<([^>]*)>/);
    if (start && l.includes("<unfinished ...>")) {
      open.set(tid, start[1]);
      continue;
    }
    const path = start ? start[1] : l.includes("resumed>") ? open.get(tid) : undefined;
    if (!start && l.includes("resumed>")) open.delete(tid);
    const n = l.match(/\)\s+=\s+(\d+)\s*$/);
    if (path !== undefined && n) {
      const k = `${path}\u0000${comm.get(tid) ?? "?"}`;
      per.set(k, (per.get(k) ?? 0) + Number(n[1]));
    }
  }
  const name = (p) => (p === big ? "big" : p === small ? "small" : p.includes("/skeleton/") ? "skeleton" : "other");
  const sum = {};
  const byThread = {};
  for (const [k, n] of per) {
    const [p, t] = k.split("\u0000");
    sum[name(p)] = (sum[name(p)] ?? 0) + n;
    byThread[`${name(p)}@${t}`] = (byThread[`${name(p)}@${t}`] ?? 0) + n;
  }
  const mbs = (o) => Object.fromEntries(Object.entries(o).filter(([, v]) => v >= 1e5).map(([k, v]) => [k, mb(v)]));
  emit({ name: "bytes-read", ...mbs(sum), bigFileMb: mb(statSync(big).size), byThread: mbs(byThread) });
  // strace.log 留在输出目录里（对账用）
}
process.exit(0);

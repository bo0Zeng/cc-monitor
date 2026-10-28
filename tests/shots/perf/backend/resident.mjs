// 后端常驻台架（手跑，不进门禁）：沙箱 HOME 起一个后端，挂 N 条「在跑」的会话（pidfile 指向本脚本起的 sleep 进程），
// 等它起来、热完，再量一个窗口里的 CPU · 常驻 · 线程 · fd · inotify · 上下文切换 · 收到的帧。
// node resident.mjs <bin> <home> <live.json> <mode: idle0|idle10|live10|big1> <窗口秒> [--warm 秒] [--rate 每会话每秒行数]
//   [--search 热完先问两次全文搜索] [--append 非 live10 也追加] [--fork 追加时造分叉] [--skip N] [--env K=V,…] [--tag 名]
// <home> 是沙箱家（底下 .claude/projects/… 放只读拷进来的记录）；<live.json> 是相对 projects 的记录路径数组。
// live10 · --append 会往那几份记录尾巴上追加行（照它尾巴上的行换 uuid），量完不截回。
import { spawn } from "node:child_process";
import { appendFileSync, readFileSync, readdirSync, readlinkSync, mkdirSync, rmSync, writeFileSync, statSync, truncateSync, openSync, readSync, closeSync } from "node:fs";
import { join } from "node:path";
import { createInterface } from "node:readline";

const [bin, home, liveJson, mode, winS] = process.argv.slice(2);
const opt = (k, d) => { const i = process.argv.indexOf(k); return i > 0 ? process.argv[i + 1] : d; };
const WARM = Number(opt("--warm", 30));
const RATE = Number(opt("--rate", 0.5));
const WIN = Number(winS);
const projects = join(home, ".claude", "projects");
const live = JSON.parse(readFileSync(liveJson, "utf8")).map((r) => join(projects, r)).slice(Number(opt("--skip", 0)));
const nLive = mode === "idle0" ? 0 : mode === "big1" ? 1 : live.length;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

function head(p, n) { const fd = openSync(p, "r"); const b = Buffer.alloc(n); const k = readSync(fd, b, 0, n, 0); closeSync(fd); return b.subarray(0, k).toString("utf8"); }
function tail(p, n) { const sz = statSync(p).size; const fd = openSync(p, "r"); const b = Buffer.alloc(Math.min(n, sz)); readSync(fd, b, 0, b.length, sz - b.length); closeSync(fd); return b.toString("utf8"); }
const cwdOf = (p) => { for (const l of head(p, 4 << 20).split("\n")) { try { const o = JSON.parse(l); if (o.cwd) return o.cwd; } catch {} } return "/"; };
const sidOf = (p) => p.slice(p.lastIndexOf("/") + 1, -".jsonl".length);

// 原长，量完截回去
const sizes = live.map((p) => statSync(p).size);
const sessions = join(home, ".claude", "sessions");
mkdirSync(sessions, { recursive: true });
for (const f of readdirSync(sessions)) rmSync(join(sessions, f));
const sleepers = [];
for (let i = 0; i < nLive; i++) {
  const s = spawn("/bin/bash", ["-c", "exec -a claude /bin/sleep 100000"], { stdio: "ignore" });
  await sleep(30);
  sleepers.push(s);
  writeFileSync(join(sessions, `${s.pid}.json`), JSON.stringify({ pid: s.pid, sessionId: sidOf(live[i]), cwd: cwdOf(live[i]), startedAt: Date.now(), kind: "interactive" }));
}

const box = join(home, "..");
mkdirSync(join(box, "tmux"), { recursive: true });
mkdirSync(join(box, "run"), { recursive: true });
const env = { PATH: "/usr/bin:/bin", HOME: home, LANG: "C.UTF-8", TZ: "UTC", TMUX_TMPDIR: join(box, "tmux"), XDG_RUNTIME_DIR: join(box, "run"), RUST_LOG: "warn", CCM_RELAY_PORT: String(41000 + (process.pid % 2000)) };
for (const kv of (opt("--env", "") || "").split(",").filter(Boolean)) { const [k, v] = kv.split("="); env[k] = v; }
const TAG = opt("--tag", "");
const child = spawn(bin, ["--", "--stream"], { env, stdio: ["pipe", "pipe", "pipe"] });
const pid = child.pid;
let stderr = "";
child.stderr.on("data", (d) => (stderr += d));
let frames = 0, bytes = 0, hello = false;
const kinds = {};
createInterface({ input: child.stdout, crlfDelay: Infinity }).on("line", (raw) => {
  frames++; bytes += raw.length + 1;
  try { const f = JSON.parse(raw); kinds[f.kind] = (kinds[f.kind] ?? 0) + 1; if (f.kind === "hello") hello = true; } catch {}
});

const stat = () => { const r = readFileSync(`/proc/${pid}/stat`, "utf8"); const x = r.slice(r.lastIndexOf(")") + 2).split(" "); return +x[11] + +x[12]; };
const status = () => { const s = readFileSync(`/proc/${pid}/status`, "utf8"); const g = (k) => Number((s.match(new RegExp(`${k}:\\s+(\\d+)`)) || [])[1] || 0); return { rssMb: +(g("VmRSS") / 1024).toFixed(1), hwmMb: +(g("VmHWM") / 1024).toFixed(1), anonMb: +(g("RssAnon") / 1024).toFixed(1), threads: g("Threads") }; };
const ctx = () => { let v = 0, n = 0; for (const t of readdirSync(`/proc/${pid}/task`)) { try { const s = readFileSync(`/proc/${pid}/task/${t}/status`, "utf8"); v += +s.match(/^voluntary_ctxt_switches:\s+(\d+)/m)[1]; n += +s.match(/^nonvoluntary_ctxt_switches:\s+(\d+)/m)[1]; } catch {} } return v + n; };
const fds = () => { const by = {}; let watches = 0; for (const fd of readdirSync(`/proc/${pid}/fd`)) { let l; try { l = readlinkSync(`/proc/${pid}/fd/${fd}`); } catch { continue; } const k = l.startsWith("/") ? "file" : l.replace(/[0-9]+/g, "N"); by[k] = (by[k] ?? 0) + 1; if (l === "anon_inode:inotify") { try { watches += (readFileSync(`/proc/${pid}/fdinfo/${fd}`, "utf8").match(/^inotify/gm) || []).length; } catch {} } } return { n: Object.values(by).reduce((a, b) => a + b, 0), watches, by }; };
const threadNames = () => { const c = {}; for (const t of readdirSync(`/proc/${pid}/task`)) { try { const n = readFileSync(`/proc/${pid}/task/${t}/comm`, "utf8").trim().replace(/-?\d+$/, ""); c[n] = (c[n] ?? 0) + 1; } catch {} } return c; };

const t0 = performance.now();
while (!hello) { await sleep(10); if (performance.now() - t0 > 60000) throw new Error("no hello: " + stderr.slice(-2000)); }
const helloMs = performance.now() - t0;
await sleep(WARM * 1000);
let searched = null;
if (process.argv.includes("--search")) {
  const waiting = new Map(); let nid = 0;
  const rl2 = createInterface({ input: child.stdout, crlfDelay: Infinity });
  rl2.on("line", (raw) => { try { const f = JSON.parse(raw); if (f.kind === "reply" && waiting.has(f.id)) { waiting.get(f.id)(f); waiting.delete(f.id); } } catch {} });
  const call = (cmd, args) => new Promise((res) => { const id = `s${++nid}`; waiting.set(id, res); child.stdin.write(JSON.stringify({ id, cmd, args, within_ms: 600000 }) + "\n"); });
  const before = status(); const cs = stat(); const ts = performance.now();
  const a = await call("history-search", { query: "游标分页" });
  const b = await call("history-search", { query: "npm test", include_tools: true });
  await sleep(10000);
  searched = { ok: [a.ok, b.ok], ms: +(performance.now() - ts - 10000).toFixed(0), cpuS: +((stat() - cs) / 100).toFixed(1), rssBefore: before.rssMb, rssAfter10s: status().rssMb, hwm: status().hwmMb };
}
const tt = () => { const c = {}; for (const t of readdirSync(`/proc/${pid}/task`)) { try { const r = readFileSync(`/proc/${pid}/task/${t}/stat`, "utf8"); const x = r.slice(r.lastIndexOf(")") + 2).split(" "); c[t] = [readFileSync(`/proc/${pid}/task/${t}/comm`, "utf8").trim().replace(/-?\d+$/, ""), +x[11] + +x[12]]; } catch {} } return c; };
const tt0 = tt();
const m0 = status(); const c0 = stat(); const x0 = ctx(); const f0 = frames, b0 = bytes;
// 窗口里：live10 每会话每秒 RATE 行（照它自己尾巴上的行换 uuid）
let appended = 0;
const tpl = live.slice(0, nLive).map((p) => tail(p, 400000).split("\n").filter(Boolean).slice(-8));
const FORK = process.argv.includes("--fork");
const lastUuid = live.slice(0, nLive).map((p) => { const rows = tail(p, 400000).split("\n").filter(Boolean); for (let j = rows.length - 1; j >= 0; j--) { try { const o = JSON.parse(rows[j]); if (o.uuid) return o.uuid; } catch {} } return null; });
const tw = performance.now();
if (mode === "live10" || process.argv.includes("--append")) {
  const period = 1000 / (RATE * nLive);
  let i = 0;
  while (performance.now() - tw < WIN * 1000) {
    const k = i % nLive; const rows = tpl[k];
    let rec; try { rec = JSON.parse(rows[i % rows.length]); } catch { rec = { type: "x" }; }
    // 接在这份记录此刻的最后一条后面（不造分叉）；--fork 时故意接到尾巴里的旧行上（造分叉）
    if (rec.uuid) { rec.parentUuid = FORK ? rec.uuid : lastUuid[k]; rec.uuid = `p454-${process.pid}-${i}-${rec.uuid.slice(-12)}`; lastUuid[k] = rec.uuid; }
    appendFileSync(live[k], JSON.stringify(rec) + "\n"); appended++; i++;
    await sleep(period);
  }
} else {
  await sleep(WIN * 1000);
}
const secs = (performance.now() - tw) / 1000;
const tt1 = tt(); const byThr = {}; for (const [t, [n, v]] of Object.entries(tt1)) { const d = v - (tt0[t]?.[1] ?? 0); if (d > 0) byThr[n] = (byThr[n] ?? 0) + d; }
const m1 = status(); const c1 = stat(); const x1 = ctx(); const fd = fds();
console.log(JSON.stringify({ tag: TAG, searched, mode, helloMs: +helloMs.toFixed(0), warmS: WARM, winS: +secs.toFixed(1), cpuPct: +(((c1 - c0) / secs)).toFixed(2), ctxPerS: +((x1 - x0) / secs).toFixed(1), appended, framesPerS: +((frames - f0) / secs).toFixed(2), kbPerS: +((bytes - b0) / 1024 / secs).toFixed(1), rss0: m0.rssMb, rss1: m1.rssMb, anon1: m1.anonMb, hwm: m1.hwmMb, threads: m1.threads, fds: fd.n, watches: fd.watches, fdBy: fd.by, threadNames: threadNames(), cpuByThread: byThr, kinds, load: readFileSync("/proc/loadavg", "utf8").split(" ").slice(0, 3).join(" ") }));
child.kill("SIGTERM");
for (const s of sleepers) s.kill("SIGTERM");
// 不截回：截短会让下一趟后端认成「记录变短」、从 0 整份重读（台架自己造的假象）。追加的行留着，下一趟照「会话变长了」接着读。
void sizes; void truncateSync;
for (const f of readdirSync(sessions)) rmSync(join(sessions, f));
await sleep(500);
process.exit(0);

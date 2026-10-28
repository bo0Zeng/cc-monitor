// 起来那一下读了什么：沙箱后端（同 resident.mjs 的环境与 10 条在跑的会话），可选经 strace 记 read/pread64；
// 等到 CPU 连续 10 s 没动（或 300 s），报 rchar · CPU 秒 · 墙钟 · 常驻；带 --strace 时按文件类合计字节。
// node startup-reads.mjs <bin> <home> <live.json> [--strace <log>] [--frames <out.json>]
//   --frames：每条会话最后那一帧运行表（`session_runs`）照会话排好写进 <out.json>，改前改后两趟拿来逐项比。
import { spawn } from "node:child_process";
import { readFileSync, readdirSync, rmSync, mkdirSync, writeFileSync, openSync, readSync, closeSync } from "node:fs";
import { join } from "node:path";
import { createInterface } from "node:readline";
const [bin, home, liveJson] = process.argv.slice(2);
const si = process.argv.indexOf("--strace");
const traceLog = si > 0 ? process.argv[si + 1] : null;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const projects = join(home, ".claude", "projects");
const live = JSON.parse(readFileSync(liveJson, "utf8")).map((r) => join(projects, r));
const head = (p) => { const fd = openSync(p, "r"); const b = Buffer.alloc(4 << 20); const k = readSync(fd, b, 0, b.length, 0); closeSync(fd); return b.subarray(0, k).toString("utf8"); };
const cwdOf = (p) => { for (const l of head(p).split("\n")) { try { const o = JSON.parse(l); if (o.cwd) return o.cwd; } catch {} } return "/"; };
const sessions = join(home, ".claude", "sessions");
mkdirSync(sessions, { recursive: true });
for (const f of readdirSync(sessions)) rmSync(join(sessions, f));
const sleepers = [];
for (const p of live) {
  const s = spawn("/bin/bash", ["-c", "exec -a claude /bin/sleep 100000"], { stdio: "ignore" });
  sleepers.push(s); await sleep(30);
  writeFileSync(join(sessions, `${s.pid}.json`), JSON.stringify({ pid: s.pid, sessionId: p.slice(p.lastIndexOf("/") + 1, -6), cwd: cwdOf(p), startedAt: Date.now(), kind: "interactive" }));
}
const box = join(home, "..");
const env = { PATH: "/usr/bin:/bin", HOME: home, LANG: "C.UTF-8", TZ: "UTC", TMUX_TMPDIR: join(box, "tmux"), XDG_RUNTIME_DIR: join(box, "run"), RUST_LOG: "warn", CCM_RELAY_PORT: String(43000 + (process.pid % 1000)) };
const child = traceLog
  ? spawn("strace", ["-f", "-qq", "-y", "-s", "0", "-e", "trace=read,pread64", "-e", "signal=none", "-o", traceLog, bin, "--", "--stream"], { env, stdio: ["pipe", "pipe", "pipe"] })
  : spawn(bin, ["--", "--stream"], { env, stdio: ["pipe", "pipe", "pipe"] });
const fi = process.argv.indexOf("--frames");
const framesOut = fi > 0 ? process.argv[fi + 1] : null;
const runs = {};
createInterface({ input: child.stdout, crlfDelay: Infinity }).on("line", (raw) => {
  if (!framesOut || !raw.includes('"session_runs"')) return;
  try { const f = JSON.parse(raw); if (f.kind === "session_runs") runs[f.sid] = f; } catch {}
});
child.stderr.on("data", () => {});
const t0 = performance.now();
await sleep(1500);
const pidOf = () => { if (!traceLog) return child.pid; const c = readFileSync(`/proc/${child.pid}/task/${child.pid}/children`, "utf8").trim().split(" ")[0]; return Number(c); };
const pid = pidOf();
const ticks = () => { const r = readFileSync(`/proc/${pid}/stat`, "utf8"); const x = r.slice(r.lastIndexOf(")") + 2).split(" "); return +x[11] + +x[12]; };
const rchar = () => Number(readFileSync(`/proc/${pid}/io`, "utf8").match(/rchar:\s+(\d+)/)[1]);
let last = ticks(), quiet = 0, quietFrom = performance.now();
while (performance.now() - t0 < 300000) {
  await sleep(1000);
  const t = ticks();
  if (t - last <= 1) { if (++quiet === 10) break; } else { quiet = 0; quietFrom = performance.now(); }
  last = t;
}
const out = { busyS: +((quietFrom - t0) / 1000).toFixed(1), cpuS: +(ticks() / 100).toFixed(1), rcharMb: +(rchar() / 1e6).toFixed(0), rssMb: +(Number(readFileSync(`/proc/${pid}/status`, "utf8").match(/VmRSS:\s+(\d+)/)[1]) / 1024).toFixed(0) };
child.kill("SIGTERM");
for (const s of sleepers) s.kill("SIGTERM");
await sleep(1500);
if (traceLog) {
  const by = {};
  for (const l of readFileSync(traceLog, "utf8").split("\n")) {
    const m = l.match(/(?:read|pread64)\(\d+<([^>]*)>.*\)\s+=\s+(\d+)/);
    if (!m) continue;
    const p = m[1];
    const k = p.includes("/subagents/") ? "subagents" : p.includes("/.cc-monitor/") ? "cc-monitor:" + p.split("/.cc-monitor/")[1].split("/").slice(0, 2).join("/") : p.endsWith(".jsonl") ? "main-records" : p.startsWith("/") ? "other-files" : "non-file";
    by[k] = (by[k] ?? 0) + Number(m[2]);
  }
  out.byKindMb = Object.fromEntries(Object.entries(by).sort((a, b) => b[1] - a[1]).map(([k, v]) => [k, +(v / 1e6).toFixed(0)]));
}
for (const f of readdirSync(sessions)) rmSync(join(sessions, f));
if (framesOut) writeFileSync(framesOut, JSON.stringify(Object.keys(runs).sort().map((k) => runs[k]), null, 1));
console.log(JSON.stringify(out));
process.exit(0);

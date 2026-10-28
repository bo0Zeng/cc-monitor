// 冷开一条会话：照壳与界面的问法依次问后端，数每条几页（＝ 串行往返）· 回包字节 · 墙钟。读数不进判据。
//
// node cold.mjs <后端> <样本.jsonl> <工作目录>
//   样本拷进 <工作目录>/home 那个沙箱家目录（HOME · tmux · 运行目录都在工作目录里），后端 `-- --stream` 起在沙箱里。
//   环境变量 SSH_TO=<口>:<钥匙>:<known_hosts> ⇒ 不直接起后端，经 ssh 连回环上那个口（登录壳在那头起同一个沙箱的后端，见 cold.sh）。
// 印：每条命令一行 JSON，最后一行是这一趟的合计（tail_pages · open_trips · open_ms · hello_ms · rows_sha）。
import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { readFileSync, writeFileSync, mkdirSync, rmSync } from "node:fs";
import { join } from "node:path";
import { createInterface } from "node:readline";

const [bin, sample, work] = process.argv.slice(2);
const lines = readFileSync(sample, "utf8").split("\n").filter(Boolean);
const first = JSON.parse(lines.find((l) => l.includes('"sessionId"') && l.includes('"cwd"')));
rmSync(work, { recursive: true, force: true });
const home = join(work, "home");
const pdir = join(home, ".claude", "projects", "-p");
for (const d of [pdir, join(work, "tmux"), join(work, "run")]) mkdirSync(d, { recursive: true });
const path = join(pdir, `${first.sessionId}.jsonl`);
writeFileSync(path, lines.map((l) => l + "\n").join(""));

const env = { PATH: "/usr/bin:/bin", HOME: home, LANG: "C.UTF-8", TZ: "Asia/Shanghai", TMUX_TMPDIR: join(work, "tmux"), XDG_RUNTIME_DIR: join(work, "run"), RUST_LOG: "warn" };
const t0 = performance.now();
let child;
if (process.env.SSH_TO) {
  const [port, key, kh] = process.env.SSH_TO.split(":");
  const opts = ["-F", "/dev/null", "-o", `UserKnownHostsFile=${kh}`, "-o", "StrictHostKeyChecking=no", "-o", "IdentitiesOnly=yes", "-o", "Compression=no", "-i", key, "-p", port];
  child = spawn("ssh", [...opts, "127.0.0.1", "x"], { env: { PATH: "/usr/bin:/bin", HOME: home }, stdio: ["pipe", "pipe", "pipe"] });
} else {
  child = spawn(bin, ["--", "--stream"], { env, stdio: ["pipe", "pipe", "pipe"] });
}
child.stderr.on("data", () => {});
setTimeout(() => {
  console.error("cold.mjs: 120 秒没跑完");
  child.kill("SIGTERM");
  process.exit(1);
}, 120000).unref();

const waiting = new Map();
let helloAt = 0;
createInterface({ input: child.stdout }).on("line", (l) => {
  if (!helloAt && l.startsWith('{"kind":"hello"')) helloAt = performance.now();
  if (!l.startsWith('{"kind":"reply"')) return;
  const f = JSON.parse(l);
  const w = waiting.get(f.id);
  if (w) {
    waiting.delete(f.id);
    w({ f, bytes: Buffer.byteLength(l) + 1 });
  }
});
let n = 0;
const call = (cmd, args) =>
  new Promise((res) => {
    const id = `c${++n}`;
    const s = performance.now();
    waiting.set(id, ({ f, bytes }) => res({ ms: performance.now() - s, bytes, ok: f.ok, code: f.code, data: f.data }));
    child.stdin.write(JSON.stringify({ id, cmd, args, within_ms: 120000 }) + "\n");
  });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
while (!helloAt) await sleep(5);

const out = [];
const note = (name, pages, bytes, ms, last) => out.push({ name, pages, kb: +(bytes / 1024).toFixed(1), ms: Math.round(ms), ok: last.ok, code: last.code });
async function paged(name, cmd, mk, nextOf) {
  let at = 0, pages = 0, bytes = 0, ms = 0, last;
  for (;;) {
    last = await call(cmd, mk(at));
    pages++;
    bytes += last.bytes;
    ms += last.ms;
    if (!last.ok) break;
    const nx = nextOf(last.data);
    if (nx === null || nx === undefined || nx <= at) break;
    at = nx;
  }
  note(name, pages, bytes, ms, last);
  return pages;
}

// 冷开那一串（串行：骨架要等尾段给的 floor）。
const open0 = performance.now();
// 壳：尾段 600 行；一页没交完就照 next 用 history-read 接到 plan.end。
const t = await call("history-tail", { path, n: 600, held: null });
const rows = [...(t.data?.rows ?? [])];
let tailPages = 1, tailBytes = t.bytes, tailMs = t.ms;
if (t.ok) {
  let at = t.data.next, eof = t.data.eof;
  const end = t.data.plan?.end ?? Infinity;
  while (!eof && at < end) {
    const r = await call("history-read", { path, offset: at, until: end });
    tailPages++;
    tailBytes += r.bytes;
    tailMs += r.ms;
    if (!r.ok || r.data.next <= at) break;
    rows.push(...r.data.rows);
    eof = r.data.eof;
    at = r.data.next;
  }
}
note("history-tail(+read)", tailPages, tailBytes, tailMs, t);
// 界面：尾段之上那一截的骨架（到 floor 为止）· 会话事实。
const floor = t.data?.from ?? 0;
let got = 0;
const floorPages = await paged("history-index(floor)", "history-index", (at) => ({ path, offset: at, limit: Math.max(1, floor - got) }), (d) => {
  got += d.count;
  return got >= floor ? null : d.next;
});
const f = await call("history-facts", { path });
note("history-facts", 1, f.bytes, f.ms, f);
const openMs = performance.now() - open0;

// 其余几条分页（不在冷开那一串里，照样数）。
await paged("history-index(all)", "history-index", (at) => ({ path, offset: at }), (d) => d.next);
await paged("history-turns", "history-turns", (at) => ({ path, from: at }), (d) => d.next);
await paged("history-user-inputs", "history-user-inputs", (at) => ({ path, from: at }), (d) => d.next);
await paged("history-branch", "history-branch", (at) => ({ path, from: at }), (d) => d.next);

for (const o of out) console.log(JSON.stringify(o));
const rowsSha = createHash("sha256").update(JSON.stringify(rows)).digest("hex").slice(0, 16);
console.log(JSON.stringify({ total: true, mode: process.env.SSH_TO ? "ssh" : "direct", tail_pages: tailPages, tail_kb: +(tailBytes / 1024).toFixed(1), open_trips: tailPages + floorPages + 1, open_ms: Math.round(openMs), hello_ms: Math.round(helloAt - t0), rows: rows.length, rows_sha: rowsSha }));
child.kill("SIGTERM");
process.exit(0);

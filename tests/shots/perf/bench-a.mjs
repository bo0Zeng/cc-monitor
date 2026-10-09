/**
 * 性能台架 perfA（Chromium）：主窗口其余 · 独立查看窗 · agent 窗口。动作在页里（`actions.ts` 的 `__pa`），这里只开页、逐项调、记浏览器进程树 CPU。
 *
 *   node tests/shots/perf/bench-a.mjs [--runs 2] [--out <目录>] [--only idle,scroll-up,…] [--trace <项名>]
 *
 * 环境同 `bench.mjs`（生产构建 ＋ 页里假后端 ＋ 无头 Chromium，HOME 隔离进 `.build/perf-a-sandbox/`；不起后端、不起 claude、不碰 tmux）。
 * 世界：`world.ts::perfMainWorld`（二十四个 tab，四条几千条记录；几条在等你；最长那条带十二个子 agent 与三十条任务）。
 * 每趟：主窗口开一页，按 MAIN 的顺序逐项跑；查看窗开一页（最长那条）；agent 窗口开一页（300 轮的子运行）。
 * 读数：每项一行 —— CPU（浏览器全部进程）· 每步派发到画完 p50 / p95 / 最长 · 同步段最长 · >50 ms 帧个数 / 超出合计 / 最长帧 · 长任务 · 新建节点 · DOM 节点。
 */
import { spawn } from "node:child_process";
import { existsSync, mkdirSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import net from "node:net";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { Cdp, Page, sleep } from "../cdp.mjs";

const here = path.dirname(fileURLToPath(import.meta.url));
const repo = path.resolve(here, "../../..");
const args = parseArgs(process.argv.slice(2));
const runs = Number(args.runs ?? 2);
const out = path.resolve(args.out ?? path.join(repo, ".build/perf-a"));
const sandbox = path.join(repo, ".build/perf-a-sandbox");
const probeSrc = readFileSync(path.join(here, "probe.js"), "utf8");
// `--css <文件>`：开页时多插一段样式（试一刀之前先量它值不值）
const extraCss = args.css ? readFileSync(path.resolve(args.css), "utf8") : "";
const probe = extraCss
  ? `${probeSrc}\n;document.addEventListener("DOMContentLoaded", () => { const s = document.createElement("style"); s.textContent = ${JSON.stringify(extraCss)}; document.head.appendChild(s); });`
  : probeSrc;
export const MAIN = ["idle", "scroll-up", "scroll-down", "stream-active", "stream-background", "expand", "find", "outline", "needs", "palette", "acct", "new-session", "menu", "drawer", "agents", "jump", "idle"];
export const VIEWER = ["viewer-idle", "viewer-scroll", "viewer-find"];
const only = args.only ? new Set(String(args.only).split(",")) : null;
const LONGEST = "5e550100-0000-4000-8000-000000000100";

const children = [];
const cleanup = () => {
  for (const c of children.reverse()) {
    try {
      process.kill(c.pid, "SIGTERM");
    } catch {
      // 已经退了
    }
  }
};
process.on("exit", cleanup);
process.on("SIGINT", () => process.exit(130));
process.on("SIGTERM", () => process.exit(143));

function isolatedEnv(extra = {}) {
  const env = { ...process.env, HOME: path.join(sandbox, "home"), ...extra };
  for (const k of Object.keys(env)) if (/^(CCM_|CLAUDE_|ANTHROPIC_|TMUX)/.test(k) && !k.startsWith("CCM_SHOTS_")) delete env[k];
  for (const k of ["XDG_CONFIG_HOME", "XDG_CACHE_HOME", "XDG_DATA_HOME", "XDG_RUNTIME_DIR", "DBUS_SESSION_BUS_ADDRESS"]) delete env[k];
  return env;
}

if (args.merge) {
  const parts = String(args.merge).split(",").map((d) => JSON.parse(readFileSync(path.join(path.resolve(d), "perf-a.json"), "utf8")));
  const m = { ...parts[0], rows: parts.flatMap((p) => p.rows), load: { start: parts[0].load.start, end: parts.at(-1).load.end }, runs: parts.reduce((a, p) => a + p.runs, 0) };
  mkdirSync(out, { recursive: true });
  writeFileSync(path.join(out, "perf-a.json"), JSON.stringify(m, null, 1));
  writeFileSync(path.join(out, "perf-a.md"), summarize(m));
  console.log(summarize(m));
  process.exit(0);
}

rmSync(sandbox, { recursive: true, force: true });
mkdirSync(path.join(sandbox, "home"), { recursive: true });
mkdirSync(out, { recursive: true });

const load0 = os.loadavg();
const port = await freePort();
const port0 = args.port ? Number(args.port) : null;
let base;
if (port0) base = `http://127.0.0.1:${port0}`;
else {
  const srv = spawn(process.execPath, [path.join(here, "serve.mjs"), String(port)], { cwd: repo, env: isolatedEnv({ CCM_SHOTS_SANDBOX: sandbox }), stdio: ["ignore", "pipe", "inherit"] });
  children.push(srv);
  await new Promise((resolve, reject) => {
    let buf = "";
    srv.stdout.on("data", (d) => {
      buf += String(d);
      if (buf.includes(`READY ${port}`)) resolve();
    });
    srv.on("exit", (code) => reject(new Error(`伺服起不来（退出码 ${code}）`)));
  });
  base = `http://127.0.0.1:${port}`;
}
console.log(`伺服好了：${base}`);

const profile = path.join(sandbox, "chrome");
const browser = spawn(
  findChrome(),
  ["--headless=new", "--remote-debugging-port=0", `--user-data-dir=${profile}`, "--no-first-run", "--no-default-browser-check", "--no-sandbox", "--disable-gpu", "--password-store=basic", "--force-device-scale-factor=1", "--lang=zh-CN", "--window-size=1280,800", "--enable-precise-memory-info", "--js-flags=--expose-gc", "about:blank"],
  { env: isolatedEnv(), stdio: ["ignore", "ignore", "pipe"] },
);
children.push(browser);
const cdp = await Cdp.connect(await devtoolsUrl(profile));

const result = { when: new Date().toISOString(), engine: "chromium", runs, load: { start: load0 }, rows: [] };

async function open(url, ready) {
  const page = await Page.open(cdp, 1280, 800);
  await page.send("Page.addScriptToEvaluateOnNewDocument", { source: probe });
  await page.send("Performance.enable", { timeDomain: "timeTicks" });
  const t0 = Date.now();
  await Promise.race([page.goto(`${base}/${url}`), sleep(120_000).then(() => Promise.reject(new Error("开页 2 分钟没等到 load")))]);
  await page.waitFor("window.__shots && window.__shots.state !== 'booting'", 180_000);
  if ((await page.eval("window.__shots.state")) !== "done") throw new Error(`场景没起来：${await page.eval("window.__shots.error")}`);
  if (ready) await page.waitFor(ready, 60_000);
  await page.eval("__perf.quiet(1000, 60000)");
  return { page, openMs: Date.now() - t0 };
}

async function runAll(page, names, run, where) {
  for (const name of names) {
    if (only && !only.has(name)) continue;
    const c0 = cpuMs(browser.pid);
    const th0 = args.threads ? threadCpu(browser.pid) : null;
    let r;
    const go = () => page.eval(`__pa.run(${JSON.stringify(name)})`);
    try {
      r = args.trace === name && run === 0 ? await traced(page, `trace-${where}-${name}.json`, go) : await go();
    } catch (e) {
      r = { name, error: String(e).slice(0, 300) };
    }
    const cpu = cpuMs(browser.pid) - c0;
    if (th0) {
      // `--threads`：这一项里哪几个线程在吃 CPU（进程名 / 线程名）
      const th1 = threadCpu(browser.pid);
      const d = [...th1].map(([k, v]) => [k, v - (th0.get(k) ?? 0)]).filter(([, v]) => v > 20).sort((a, b) => b[1] - a[1]).slice(0, 12);
      console.log(`    线程：${d.map(([k, v]) => `${k} ${Math.round(v)}`).join(" · ")}`);
    }
    const row = { run, where, ...r, cpu };
    result.rows.push(row);
    console.log(`  ${where} ${name}：CPU ${Math.round(cpu)} · 画完 p95 ${Math.round(r.paintP95 ?? 0)} / 最长 ${Math.round(r.paintMax ?? 0)} · 卡帧 ${r.jankN ?? "-"} · 长任务 ${Math.round(r.ltMs ?? 0)}${r.error ? ` · 错：${r.error}` : ""}`);
  }
}

if (args.eval) {
  // 调试：开一页（`--page main|viewer|agent`），求一段表达式（可 await）
  const url = { main: "index.html?scene=perf-main", viewer: `viewer.html?scene=perf-viewer&viewer=${LONGEST}`, agent: `viewer.html?scene=perf-agent&viewer=${LONGEST}&run=agent-p0` }[args.page ?? "main"];
  const { page } = await open(url).catch(async (e) => ({ page: null, e: console.log(String(e)) }));
  if (page) {
    console.log(JSON.stringify(await page.eval(`(async () => { ${args.eval} })()`), null, 1));
    if (args.shot) writeFileSync(path.join(out, "shot.png"), await page.png());
  }
  process.exit(0);
}
for (let run = 0; run < runs; run++) {
  // 不在清单里的试验项（`overlay-probe` 之类）在主窗口跑
  const extra = only ? [...only].filter((n) => !MAIN.includes(n) && !VIEWER.includes(n)) : [];
  if (!only || MAIN.some((n) => only.has(n)) || extra.length) {
    const { page, openMs } = await open("index.html?scene=perf-main");
    console.log(`主窗口开好 ${openMs} ms`);
    await runAll(page, [...MAIN, ...extra], run, "main");
    await page.close();
  }
  if (!only || VIEWER.some((n) => only.has(n))) {
    const { page, openMs } = await open(`viewer.html?scene=perf-viewer&viewer=${LONGEST}`);
    console.log(`查看窗开好 ${openMs} ms`);
    await runAll(page, VIEWER, run, "viewer");
    await page.close();
    try {
      const a = await open(`viewer.html?scene=perf-agent&viewer=${LONGEST}&run=agent-p0`);
      console.log(`agent 窗口开好 ${a.openMs} ms`);
      await runAll(a.page, VIEWER, run, "agent");
      await a.page.close();
    } catch (e) {
      result.rows.push({ run, where: "agent", name: "open", error: String(e).slice(0, 300) });
      console.log(`agent 窗口没开起来：${String(e).slice(0, 300)}`);
    }
  }
}
result.load.end = os.loadavg();
writeFileSync(path.join(out, "perf-a.json"), JSON.stringify(result, null, 1));
writeFileSync(path.join(out, "perf-a.md"), summarize(result));
console.log(summarize(result));
cdp.close();
cleanup();
process.exit(0);

async function traced(page, name, fn) {
  const events = [];
  const off = cdp.on((msg) => {
    if (msg.sessionId === page.sessionId && msg.method === "Tracing.dataCollected") events.push(...msg.params.value);
  });
  const done = new Promise((resolve) => {
    const off2 = cdp.on((msg) => {
      if (msg.sessionId === page.sessionId && msg.method === "Tracing.tracingComplete") {
        off2();
        resolve();
      }
    });
  });
  await page.send("Tracing.start", { categories: "devtools.timeline,disabled-by-default-devtools.timeline,disabled-by-default-devtools.timeline.stack,blink,v8.execute,disabled-by-default-devtools.timeline.frame,toplevel,disabled-by-default-v8.cpu_profiler", transferMode: "ReportEvents" });
  const r = await fn();
  await page.send("Tracing.end");
  await done;
  off();
  writeFileSync(path.join(out, name), JSON.stringify({ traceEvents: events }));
  return r;
}

function f0(x) {
  return x === null || x === undefined || Number.isNaN(x) ? "—" : Math.round(x).toString();
}
function med(xs) {
  const s = xs.filter((x) => typeof x === "number").sort((a, b) => a - b);
  return s.length ? s[Math.floor(s.length / 2)] : null;
}

export function summarize(r) {
  const L = [];
  L.push(`# perfA 读数（${r.when} · ${r.engine} · ${r.runs} 趟 · loadavg 开头 ${r.load.start.map((x) => x.toFixed(1)).join("/")} 结尾 ${(r.load.end ?? []).map((x) => x.toFixed(1)).join("/")}）`);
  L.push("");
  L.push("各趟中位数（ms）。画完 ＝ 每步派发到其后第二个 rAF；卡帧 ＝ >50 ms 的帧。");
  L.push("");
  L.push("| 窗 | 项 | CPU | 步数 | 画完 p50 | 画完 p95 | 画完最长 | 同步段最长 | 卡帧个 | 卡帧超出 | 最长帧 | 长任务合计 | 最长长任务 | 新建节点 | DOM | 最慢几步 |");
  L.push("|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|");
  const keys = [];
  for (const x of r.rows) {
    const k = `${x.where}|${x.name}`;
    if (!keys.includes(k)) keys.push(k);
  }
  for (const k of keys) {
    const xs = r.rows.filter((x) => `${x.where}|${x.name}` === k && !x.error);
    const err = r.rows.find((x) => `${x.where}|${x.name}` === k && x.error);
    if (!xs.length) {
      L.push(`| ${k.replace("|", " | ")} | 错：${err?.error ?? "?"} |`);
      continue;
    }
    const m = (f) => f0(med(xs.map((x) => x[f])));
    L.push(`| ${k.replace("|", " | ")} | ${m("cpu")} | ${m("steps")} | ${m("paintP50")} | ${m("paintP95")} | ${m("paintMax")} | ${m("syncMax")} | ${m("jankN")} | ${m("jankMs")} | ${m("frameMax")} | ${m("ltMs")} | ${m("ltMax")} | ${m("nodesNew")} | ${m("dom")} | ${xs.map((x) => (x.worst ?? []).join(" ")).join(" ／ ")} |`);
  }
  const idles = r.rows.filter((x) => x.name?.endsWith("idle") && x.who);
  if (idles.length) {
    L.push("");
    L.push("空闲 5 s：");
    for (const x of idles) L.push(`- ${x.where} 第 ${x.run + 1} 趟：CPU ${f0(x.cpu)} ms · 在跑的动画 ${x.running}（${x.who.map(([a, b]) => `${a}×${b}`).join("，")}）· DOM 变更 ${x.mutations} · 回调 ${JSON.stringify(x.callbacks)} · DOM ${x.dom}`);
  }
  return L.join("\n");
}

/** 进程树里每个线程到此刻的 CPU 毫秒（键：进程名/线程名/tid）。 */
function threadCpu(root) {
  const kids = new Map();
  for (const d of readdirSync("/proc")) {
    if (!/^\d+$/.test(d)) continue;
    try {
      const st = readFileSync(`/proc/${d}/stat`, "utf8");
      const ppid = Number(st.slice(st.lastIndexOf(")") + 2).split(" ")[1]);
      if (!kids.has(ppid)) kids.set(ppid, []);
      kids.get(ppid).push(Number(d));
    } catch {
      // 进程刚退
    }
  }
  const out = new Map();
  const stack = [root];
  while (stack.length) {
    const pid = stack.pop();
    stack.push(...(kids.get(pid) ?? []));
    try {
      const cmd = readFileSync(`/proc/${pid}/cmdline`, "utf8").split("\0");
      const type = (cmd.find((a) => a.startsWith("--type=")) ?? "--type=browser").slice(7);
      for (const t of readdirSync(`/proc/${pid}/task`)) {
        try {
          const name = readFileSync(`/proc/${pid}/task/${t}/comm`, "utf8").trim();
          out.set(`${type}/${name}/${t}`, Number(readFileSync(`/proc/${pid}/task/${t}/schedstat`, "utf8").split(" ")[0]) / 1e6);
        } catch {
          // 线程刚退
        }
      }
    } catch {
      // 进程刚退
    }
  }
  return out;
}

function cpuMs(root) {
  const cpuSeen = (globalThis.__cpuSeen ??= new Map());
  const kids = new Map();
  for (const d of readdirSync("/proc")) {
    if (!/^\d+$/.test(d)) continue;
    try {
      const st = readFileSync(`/proc/${d}/stat`, "utf8");
      const ppid = Number(st.slice(st.lastIndexOf(")") + 2).split(" ")[1]);
      if (!kids.has(ppid)) kids.set(ppid, []);
      kids.get(ppid).push(Number(d));
    } catch {
      // 进程刚退
    }
  }
  const stack = [root];
  while (stack.length) {
    const pid = stack.pop();
    stack.push(...(kids.get(pid) ?? []));
    try {
      for (const t of readdirSync(`/proc/${pid}/task`)) {
        try {
          const v = Number(readFileSync(`/proc/${pid}/task/${t}/schedstat`, "utf8").split(" ")[0]);
          const key = `${pid}/${t}`;
          cpuSeen.set(key, Math.max(cpuSeen.get(key) ?? 0, v));
        } catch {
          // 线程刚退
        }
      }
    } catch {
      // 进程刚退
    }
  }
  let ns = 0;
  for (const v of cpuSeen.values()) ns += v;
  return ns / 1e6;
}

function findChrome() {
  const own = process.env.CCM_SHOTS_CHROME;
  if (own) return own;
  const cache = path.join(os.homedir(), ".cache/ms-playwright");
  const found = existsSync(cache)
    ? readdirSync(cache)
        .filter((d) => /^chromium-\d+$/.test(d))
        .map((d) => path.join(cache, d, "chrome-linux64/chrome"))
        .filter((p) => existsSync(p))
        .sort((a, b) => statSync(b).mtimeMs - statSync(a).mtimeMs)
    : [];
  if (found.length === 0) throw new Error("找不到 Chromium：设 CCM_SHOTS_CHROME");
  return found[0];
}

async function devtoolsUrl(dir) {
  const file = path.join(dir, "DevToolsActivePort");
  for (let i = 0; i < 600; i++) {
    if (existsSync(file)) {
      const [p, q] = readFileSync(file, "utf8").split("\n");
      if (p && q) return `ws://127.0.0.1:${p}${q}`;
    }
    await sleep(100);
  }
  throw new Error("浏览器没报出调试口");
}

function freePort() {
  return new Promise((resolve, reject) => {
    const s = net.createServer();
    s.listen(0, "127.0.0.1", () => {
      const { port: p } = s.address();
      s.close(() => resolve(p));
    });
    s.on("error", reject);
  });
}

function parseArgs(argv) {
  const o = {};
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (!a.startsWith("--")) continue;
    const k = a.slice(2);
    if (["runs", "out", "only", "trace", "merge", "port", "eval", "page", "css"].includes(k)) o[k] = argv[++i];
    else o[k] = true;
  }
  return o;
}

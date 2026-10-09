/**
 * 性能台架：量主窗口「切 tab」「连续快速切」「长会话首屏与滚动」，出一张读数表。
 *
 *   node tests/shots/perf/bench.mjs [--runs 3] [--out <目录>] [--dev] [--only switch,rapid,long]
 *
 * 环境：截图工具同一套（生产构建 ＋ 页里假后端 ＋ 无头 Chromium，HOME 隔离进 `.build/perf-sandbox/`；
 * 不起后端、不起 claude、不碰 tmux）。世界：`world.ts`（24 个 tab，四条几千条记录的长会话）。
 *
 * 量法（每一项都在一个新开的页里，开页后先等主线程安静）：
 * - **切一下**（switch）：按 tab 栏顺序逐个真点（CDP 鼠标事件，打到 tab 中心）。第一遍 ＝ 冷切（那个 tab 还没建过卡），
 *   第二遍 ＝ 热切（建过、DOM 留着）。每次点前等安静（300 ms 无长任务 / 无新建节点，最多 5 s），点后看 1.5 s：
 *   · 按下到画出：Event Timing 里那次 click 的 duration（输入 → 处理 → 下一帧画出；< 16 ms 的不报，记 16）；
 *   · 同步段：点击处理同步跑了多久；稳定：点下去到窗口里最后一个长任务结束（没有长任务 ＝ 那一帧）；
 *   · 长任务个数 / 合计；新建节点（可见流 / 后台流）；布局次数 · 样式重算次数 · 布局 / 样式 / 脚本毫秒（CDP Performance 计数差）。
 * - **连续快速切**（rapid）：同一串 20 下、每 200 ms 一下（长短会话混着），共 3 串；每串后等 2 s。
 *   · 每下的输入延迟（Event Timing 的 processingStart − startTime：前一下的活还没干完、这一下在排队）；
 *   · 串里 ＋ 之后 2 s 的长任务合计、最长一个；后台流（已经切走的）上还在新建的节点数；
 *   · 最后一下点完到安静；每串前后强制 GC 后的 JS 堆。
 * - **按住切**（keys）：按住「下一个 tab」（缺省键 `]`）30 下、每 40 ms 一下，3 串：每下按下到画出 / 输入排队 · > 50 ms 的帧 · 长任务 · 后台流新建节点 · 末下到安静。
 * - **长会话**（long）：新开页、第一下就点最长那条（冷），量首屏同上；然后在流上滚轮往上 60 下（每下 600 px、间隔 50 ms），
 *   量帧间隔 p50 / p95 / 最长、> 50 ms 的帧数、长任务。
 * 每项跑 `--runs` 趟（缺省 3），表里给的是全部趟合在一起的分位数；机器负载（loadavg）开头结尾各记一次。
 */
import { spawn } from "node:child_process";
import { existsSync, mkdirSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import net from "node:net";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { Cdp, Page, sleep } from "../cdp.mjs";

const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../../..");
const args = parseArgs(process.argv.slice(2));
const runs = Number(args.runs ?? 3);
const out = path.resolve(args.out ?? path.join(repo, ".build/perf"));
const sandbox = path.join(repo, ".build/perf-sandbox");
const only = new Set(String(args.only ?? "switch,rapid,keys,long").split(","));
const probeSrc = readFileSync(path.join(path.dirname(fileURLToPath(import.meta.url)), "probe.js"), "utf8");
// `--css <文件>`：开页时多插一段样式（试一刀之前先量它值不值）
const extraCss = args.css ? readFileSync(path.resolve(args.css), "utf8") : "";
const probe = extraCss
  ? `${probeSrc}\n;document.addEventListener("DOMContentLoaded", () => { const s = document.createElement("style"); s.textContent = ${JSON.stringify(extraCss)}; document.head.appendChild(s); });`
  : probeSrc;

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

rmSync(sandbox, { recursive: true, force: true });
mkdirSync(path.join(sandbox, "home"), { recursive: true });
mkdirSync(out, { recursive: true });

function isolatedEnv(extra = {}) {
  const env = { ...process.env, HOME: path.join(sandbox, "home"), ...extra };
  for (const k of Object.keys(env)) if (/^(CCM_|CLAUDE_|ANTHROPIC_|TMUX)/.test(k) && !k.startsWith("CCM_SHOTS_")) delete env[k];
  for (const k of ["XDG_CONFIG_HOME", "XDG_CACHE_HOME", "XDG_DATA_HOME", "XDG_RUNTIME_DIR"]) delete env[k];
  return env;
}

if (args.merge) {
  // 几次分开跑的读数合成一张（A/B 交替跑时用）：`--merge 目录1,目录2,… --out 目录`
  const parts = String(args.merge).split(",").map((d) => JSON.parse(readFileSync(path.join(path.resolve(d), "perf.json"), "utf8")));
  const m = { ...parts[0], runs: 0, switch: [], rapid: [], keys: [], long: [], boot: [], load: { start: parts[0].load.start, end: parts[parts.length - 1].load.end } };
  parts.forEach((p, k) => {
    m.runs += p.runs;
    for (const key of ["switch", "rapid", "keys", "long", "boot"]) m[key].push(...(p[key] ?? []).map((x) => ({ ...x, part: k })));
  });
  writeFileSync(path.join(out, "perf.json"), JSON.stringify(m, null, 1));
  const t = summarize(m);
  writeFileSync(path.join(out, "perf.md"), t);
  console.log(t);
  process.exit(0);
}
if (args.summarize) {
  // 只把已有的读数重新出表
  console.log(summarize(JSON.parse(readFileSync(path.join(out, "perf.json"), "utf8"))));
  process.exit(0);
}
const load0 = os.loadavg();
const t0 = Date.now();
const port = await freePort();
const srv = spawn(process.execPath, [path.join(repo, "tests/shots/perf/serve.mjs"), String(port), ...(args.dev ? ["--dev"] : [])], {
  cwd: repo,
  env: isolatedEnv({ CCM_SHOTS_SANDBOX: sandbox }),
  stdio: ["ignore", "pipe", "inherit"],
});
children.push(srv);
await new Promise((resolve, reject) => {
  let buf = "";
  srv.stdout.on("data", (d) => {
    buf += String(d);
    if (buf.includes(`READY ${port}`)) resolve();
  });
  srv.on("exit", (code) => reject(new Error(`伺服起不来（退出码 ${code}）`)));
});
console.log(`伺服好了（${args.dev ? "开发服务器" : "生产构建"}，${((Date.now() - t0) / 1000).toFixed(0)} s）`);
const base = `http://127.0.0.1:${port}`;

const profile = path.join(sandbox, "chrome");
const browser = spawn(
  findChrome(),
  [
    "--headless=new",
    "--remote-debugging-port=0",
    `--user-data-dir=${profile}`,
    "--no-first-run",
    "--no-default-browser-check",
    "--no-sandbox",
    "--disable-gpu",
    "--force-device-scale-factor=1",
    "--lang=zh-CN",
    "--window-size=1280,800",
    "--enable-precise-memory-info",
    "--js-flags=--expose-gc",
    "about:blank",
  ],
  { env: isolatedEnv(), stdio: ["ignore", "ignore", "pipe"] },
);
children.push(browser);
const cdp = await Cdp.connect(await devtoolsUrl(profile));

const result = { when: new Date().toISOString(), mode: args.dev ? "dev" : "build", runs, load: { start: load0 }, switch: [], rapid: [], keys: [], long: [], boot: [] };

if (args.shot) {
  // 看一眼世界长什么样：开页、点最长那条、截一张
  const page = await openPage();
  writeFileSync(path.join(out, "boot.png"), await page.png());
  const info = await page.eval(`[...document.querySelectorAll('#tab-bar .tab')].map((t) => t.textContent)`);
  writeFileSync(path.join(out, "tabs.txt"), info.join("\n"));
  await page.close();
}
if (args.eval) {
  // 调试：开页、按一下「下一个 tab」，在那之后的第一个 rAF 里求一段表达式（看切进来那一帧的几何）
  const page = await openPage();
  for (let k = 0; k < Number(args.presses ?? 3); k++) {
    await page.eval(`new Promise((res) => { window.__dbg = []; const ex = ${JSON.stringify(String(args.eval))}; requestAnimationFrame(() => {}); document.addEventListener('keydown', () => requestAnimationFrame(() => { try { window.__dbg.push(eval(ex)); } catch (e) { window.__dbg.push(String(e)); } }), { once: true }); res(0); })`);
    await page.send("Input.dispatchKeyEvent", { type: "keyDown", key: "]", code: "BracketRight", windowsVirtualKeyCode: 221, nativeVirtualKeyCode: 221 });
    await page.send("Input.dispatchKeyEvent", { type: "keyUp", key: "]", code: "BracketRight", windowsVirtualKeyCode: 221, nativeVirtualKeyCode: 221 });
    await sleep(600);
    console.log(JSON.stringify(await page.eval("window.__dbg")));
  }
  await page.close();
}
for (let r = 0; r < runs; r++) {
  if (only.has("switch")) result.switch.push(...(await benchSwitch(r)));
  if (only.has("rapid")) result.rapid.push(...(await benchRapid(r)));
  if (only.has("keys")) result.keys.push(...(await benchKeys(r)));
  if (only.has("long")) result.long.push(await benchLong(r));
}
result.load.end = os.loadavg();
writeFileSync(path.join(out, "perf.json"), JSON.stringify(result, null, 1));
const table = summarize(result);
writeFileSync(path.join(out, "perf.md"), table);
console.log(table);
cdp.close();
cleanup();
process.exit(0);

// ─────────────────────────────────────────── 各项 ───────────────────────────────────────────

async function openPage() {
  const page = await Page.open(cdp, 1280, 800);
  await page.send("Page.addScriptToEvaluateOnNewDocument", { source: probe });
  await page.send("Performance.enable", { timeDomain: "timeTicks" });
  const b0 = Date.now();
  await page.goto(`${base}/index.html?scene=perf-tabs`);
  await page.waitFor("window.__shots && window.__shots.state !== 'booting'", 120_000);
  const st = await page.eval("window.__shots.state");
  if (st !== "done") throw new Error(`场景没起来：${await page.eval("window.__shots.error")}`);
  const quiet = await page.eval("__perf.quiet(1000, 60000)");
  if (args.profile) {
    // `--dev --profile tests/shots/perf/profile.js`：给切换路上的方法挂计时（开发服务器下模块按源码路径 import 到的就是界面那一份）
    await page.eval(`(async () => { ${readFileSync(path.resolve(String(args.profile)), "utf8")} })()`);
  }
  const c0 = cpuMs(browser.pid);
  await sleep(1500);
  const idleCpu = cpuMs(browser.pid) - c0;
  // 开页安静之后还一张卡都没建的 tab（后台空闲物化没轮到 / 没进队）
  const virgin = await page.eval("[...document.querySelectorAll('#message-stream > .stream')].filter((s) => !s.querySelector('.card')).length");
  result.boot.push({ ms: Date.now() - b0, quietWait: quiet, idleCpu, virgin, ...(await metrics(page)) });
  return page;
}

async function metrics(page) {
  const { metrics: m } = await page.send("Performance.getMetrics");
  const o = {};
  for (const x of m) o[x.name] = x.value;
  return o;
}

async function heapAfterGc(page) {
  await page.send("HeapProfiler.collectGarbage");
  await sleep(200);
  await page.send("HeapProfiler.collectGarbage");
  return (await metrics(page)).JSHeapUsedSize;
}

/** tab 栏第 i 个：滚进视野、给出中心坐标与标题里的轮数。 */
async function tabAt(page, i) {
  return page.eval(`(() => {
    const t = document.querySelectorAll('#tab-bar .tab')[${i}];
    if (!t) return null;
    t.scrollIntoView({ block: 'nearest' });
    const b = t.getBoundingClientRect();
    const m = /t(\\d+)-perf/.exec(t.textContent || '');
    return { x: b.left + b.width / 2, y: b.top + b.height / 2, turns: m ? Number(m[1]) : null, active: t.classList.contains('active') };
  })()`);
}

async function clickAt(page, x, y) {
  await page.send("Input.dispatchMouseEvent", { type: "mouseMoved", x, y });
  await page.send("Input.dispatchMouseEvent", { type: "mousePressed", x, y, button: "left", clickCount: 1 });
  await page.send("Input.dispatchMouseEvent", { type: "mouseReleased", x, y, button: "left", clickCount: 1 });
}

/** 点一下、看 `watchMs`，交回这一下的读数。 */
async function measuredClick(page, i, watchMs = 1500) {
  const t = await tabAt(page, i);
  if (!t) throw new Error(`没有第 ${i} 个 tab`);
  await page.eval("__perf.quiet(300, 5000)");
  const m0 = await metrics(page);
  const since = await page.eval("performance.now()");
  const c0 = cpuMs(browser.pid);
  await clickAt(page, t.x, t.y);
  await sleep(watchMs);
  const cpu = cpuMs(browser.pid) - c0;
  const m1 = await metrics(page);
  const w = await page.eval(`__perf.since(${since})`);
  const click = w.clicks[0] ?? null;
  const ev = w.ev.find((e) => e.name === "click");
  const t0c = click ? click.t0 : since;
  const lts = w.lt.filter((x) => x.s + x.d >= t0c);
  const lastLt = lts.length ? Math.max(...lts.map((x) => x.s + x.d)) - t0c : 0;
  return {
    tab: i,
    turns: t.turns,
    cpu,
    inp: ev ? ev.d : 16,
    inputDelay: ev ? ev.ps - ev.s : 0,
    sync: click?.sync ?? null,
    frame: click?.frame ?? null,
    settle: Math.max(lastLt, click?.frame ?? 0),
    ltN: lts.length,
    ltMs: lts.reduce((a, x) => a + x.d, 0),
    ltMax: lts.reduce((a, x) => Math.max(a, x.d), 0),
    nodesOn: w.mut.reduce((a, x) => a + x.on, 0),
    nodesOff: w.mut.reduce((a, x) => a + x.off, 0),
    layouts: m1.LayoutCount - m0.LayoutCount,
    styles: m1.RecalcStyleCount - m0.RecalcStyleCount,
    layoutMs: (m1.LayoutDuration - m0.LayoutDuration) * 1000,
    styleMs: (m1.RecalcStyleDuration - m0.RecalcStyleDuration) * 1000,
    scriptMs: (m1.ScriptDuration - m0.ScriptDuration) * 1000,
    domNodes: m1.Nodes,
  };
}

async function benchSwitch(run) {
  const page = await openPage();
  const n = await page.eval("document.querySelectorAll('#tab-bar .tab').length");
  const rows = [];
  for (const pass of ["cold", "warm"]) {
    for (let i = 0; i < n; i++) {
      const t = await tabAt(page, i);
      if (t.active) continue; // 已经是当前的那个点了不切
      rows.push({ run, pass, ...(await measuredClick(page, i)) });
    }
    // 第二遍从另一个起点开始（不然最后一个已经是当前、第一个热切永远跳过）
    const first = await tabAt(page, 0);
    if (first.active) rows.push({ run, pass: "warm", ...(await measuredClick(page, 1)) });
  }
  if (args.trace === true && run === 0) await traceSome(page, n);
  const heap = await heapAfterGc(page);
  rows.forEach((r) => (r.heapEnd = heap));
  await page.close();
  console.log(`  切一下 第 ${run + 1} 趟：${rows.length} 次`);
  return rows;
}

/** `--trace`：热切六下录一份性能轨迹（`trace-warm.json`，DevTools 性能面板能直接打开）。 */
/** 录一段性能轨迹（`fn` 跑的那一段），存成 `name`（DevTools 性能面板能直接打开）。 */
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
  await page.send("Tracing.start", {
    categories: "devtools.timeline,disabled-by-default-devtools.timeline,blink,v8.execute,disabled-by-default-devtools.timeline.frame,toplevel,disabled-by-default-v8.cpu_profiler",
    transferMode: "ReportEvents",
  });
  const r = await fn();
  await page.send("Tracing.end");
  await done;
  off();
  writeFileSync(path.join(out, name), JSON.stringify({ traceEvents: events }));
  return r;
}

async function traceSome(page, n) {
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
  await page.send("Tracing.start", {
    categories: "devtools.timeline,disabled-by-default-devtools.timeline,blink,v8.execute,disabled-by-default-devtools.timeline.frame,toplevel",
    transferMode: "ReportEvents",
  });
  for (const i of [0, 3, 7, 12, 1, 5].map((x) => x % n)) {
    const t = await tabAt(page, i);
    if (!t.active) await measuredClick(page, i, 800);
  }
  await page.send("Tracing.end");
  await done;
  off();
  writeFileSync(path.join(out, "trace-warm.json"), JSON.stringify({ traceEvents: events }));
}

/** 20 下的串：长短混着（tab 栏顺序里挑），每 200 ms 一下。 */

async function benchRapid(run) {
  const page = await openPage();
  const n = await page.eval("document.querySelectorAll('#tab-bar .tab').length");
  const pos = [];
  for (let i = 0; i < n; i++) pos.push(await tabAt(page, i));
  const rows = [];
  for (let burst = 0; burst < 3; burst++) {
    const heap0 = await heapAfterGc(page);
    await page.eval("__perf.quiet(500, 8000)");
    const m0 = await metrics(page);
    const since = await page.eval("performance.now()");
    const c0 = cpuMs(browser.pid);
    const sent = [];
    for (const i of [0, 3, 7, 12, 1, 5, 0, 9, 3, 14, 7, 2, 12, 20, 0, 6, 3, 11, 7, 12]) {
      const p = await tabAt(page, i % n);
      sent.push(Date.now());
      await clickAt(page, p.x, p.y);
      await sleep(Math.max(0, 200 - (Date.now() - sent[sent.length - 1])));
    }
    const lastClick = await page.eval("performance.now()");
    const settled = await page.eval("__perf.quiet(300, 15000)");
    await sleep(Math.max(0, 2000 - settled));
    const cpu = cpuMs(browser.pid) - c0;
    const m1 = await metrics(page);
    const w = await page.eval(`__perf.since(${since})`);
    const heap1 = await heapAfterGc(page);
    const evs = w.ev.filter((e) => e.name === "click");
    rows.push({
      run,
      burst,
      cpu,
      clicks: w.clicks.length,
      inpP50: pct(evs.map((e) => e.d), 0.5, 16),
      inpMax: Math.max(16, ...evs.map((e) => e.d)),
      delayMax: Math.max(0, ...evs.map((e) => e.ps - e.s)),
      ltN: w.lt.length,
      ltMs: w.lt.reduce((a, x) => a + x.d, 0),
      ltMax: w.lt.reduce((a, x) => Math.max(a, x.d), 0),
      nodesOn: w.mut.reduce((a, x) => a + x.on, 0),
      nodesOff: w.mut.reduce((a, x) => a + x.off, 0),
      afterLast: settled,
      lastClickAt: lastClick - since,
      layouts: m1.LayoutCount - m0.LayoutCount,
      styles: m1.RecalcStyleCount - m0.RecalcStyleCount,
      layoutMs: (m1.LayoutDuration - m0.LayoutDuration) * 1000,
      styleMs: (m1.RecalcStyleDuration - m0.RecalcStyleDuration) * 1000,
      scriptMs: (m1.ScriptDuration - m0.ScriptDuration) * 1000,
      heap0,
      heap1,
      domNodes: m1.Nodes,
    });
  }
  await page.close();
  console.log(`  连续快速切 第 ${run + 1} 趟：3 串`);
  return rows;
}

/**
 * 按住「下一个 tab」（缺省键 `]`）连切：30 下、每 40 ms 一下（键盘自动重复的量级）。
 * 量：每下 keydown 的「按下到画出」与输入排队（Event Timing）· 长任务 · 帧间隔（> 50 ms 的帧）· 后台流上新建的节点 · 末下到安静。
 */
async function benchKeys(run) {
  const page = await openPage();
  const rows = [];
  if (args.profile) await page.eval("window.__prof && window.__prof.reset()");
  for (let burst = 0; burst < 3; burst++) {
    await page.eval("__perf.quiet(500, 8000)");
    const m0 = await metrics(page);
    const since = await page.eval("performance.now()");
    const c0 = cpuMs(browser.pid);
    await page.eval("__perf.frameStart()");
    const press = async () => {
      for (let k = 0; k < 30; k++) {
        const t = Date.now();
        await page.send("Input.dispatchKeyEvent", { type: "keyDown", key: "]", code: "BracketRight", windowsVirtualKeyCode: 221, nativeVirtualKeyCode: 221 });
        await page.send("Input.dispatchKeyEvent", { type: "keyUp", key: "]", code: "BracketRight", windowsVirtualKeyCode: 221, nativeVirtualKeyCode: 221 });
        await sleep(Math.max(0, 40 - (Date.now() - t)));
      }
      return page.eval("__perf.quiet(300, 15000)");
    };
    const settled = args.trace === "keys" && burst === 1 ? await traced(page, "trace-keys.json", press) : await press();
    const frames = await page.eval("__perf.frameStop()");
    const cpu = cpuMs(browser.pid) - c0;
    const m1 = await metrics(page);
    const w = await page.eval(`__perf.since(${since})`);
    const evs = w.ev.filter((e) => e.name === "keydown");
    rows.push({
      run,
      burst,
      cpu,
      keys: evs.length,
      inpP50: pct(evs.map((e) => e.d), 0.5, 16),
      inpMax: Math.max(16, ...evs.map((e) => e.d)),
      delayMax: Math.max(0, ...evs.map((e) => e.ps - e.s)),
      framesOver50: frames.filter((d) => d > 50).length,
      frameMax: Math.max(0, ...frames),
      ltN: w.lt.length,
      ltMs: w.lt.reduce((a, x) => a + x.d, 0),
      nodesOff: w.mut.reduce((a, x) => a + x.off, 0),
      afterLast: settled,
      layoutMs: (m1.LayoutDuration - m0.LayoutDuration) * 1000,
      styleMs: (m1.RecalcStyleDuration - m0.RecalcStyleDuration) * 1000,
      scriptMs: (m1.ScriptDuration - m0.ScriptDuration) * 1000,
    });
  }
  if (args.profile) writeFileSync(path.join(out, "prof-keys.json"), JSON.stringify(await page.eval("window.__prof ? { top: window.__prof.dump(), hidden: window.__prof.hidden() } : null"), null, 1));
  await page.close();
  console.log(`  按住切 第 ${run + 1} 趟：3 串`);
  return rows;
}

async function benchLong(run) {
  const page = await openPage();
  const n = await page.eval("document.querySelectorAll('#tab-bar .tab').length");
  let longest = 0;
  let most = -1;
  for (let i = 0; i < n; i++) {
    const t = await tabAt(page, i);
    if ((t.turns ?? 0) > most) {
      most = t.turns;
      longest = i;
    }
  }
  const first = await measuredClick(page, longest, 2500);
  await page.eval("__perf.quiet(500, 10000)");
  // 滚轮打在消息流中部
  const at = await page.eval(`(() => { const s = document.querySelector('.stream.active'); const b = s.getBoundingClientRect(); return { x: b.left + b.width / 2, y: b.top + b.height / 2 }; })()`);
  const m0 = await metrics(page);
  const since = await page.eval("performance.now()");
  await page.eval("__perf.frameStart()");
  const c0 = cpuMs(browser.pid);
  for (let k = 0; k < 60; k++) {
    await page.send("Input.dispatchMouseEvent", { type: "mouseWheel", x: at.x, y: at.y, deltaX: 0, deltaY: -600 });
    await sleep(50);
  }
  await sleep(1000);
  const cpu = cpuMs(browser.pid) - c0;
  const frames = await page.eval("__perf.frameStop()");
  const m1 = await metrics(page);
  const w = await page.eval(`__perf.since(${since})`);
  const top = await page.eval("document.querySelector('.stream.active').scrollTop");
  await page.close();
  console.log(`  长会话 第 ${run + 1} 趟`);
  return {
    run,
    first,
    cpu,
    frames: frames.length,
    frameP50: pct(frames, 0.5),
    frameP95: pct(frames, 0.95),
    frameMax: Math.max(0, ...frames),
    framesOver50: frames.filter((d) => d > 50).length,
    ltN: w.lt.length,
    ltMs: w.lt.reduce((a, x) => a + x.d, 0),
    ltMax: w.lt.reduce((a, x) => Math.max(a, x.d), 0),
    nodes: w.mut.reduce((a, x) => a + x.on + x.off, 0),
    layouts: m1.LayoutCount - m0.LayoutCount,
    layoutMs: (m1.LayoutDuration - m0.LayoutDuration) * 1000,
    styleMs: (m1.RecalcStyleDuration - m0.RecalcStyleDuration) * 1000,
    scriptMs: (m1.ScriptDuration - m0.ScriptDuration) * 1000,
    scrollTopEnd: top,
  };
}

// ─────────────────────────────────────────── 汇总 ───────────────────────────────────────────

function pct(xs, q, floor = 0) {
  if (xs.length === 0) return floor;
  const s = [...xs].sort((a, b) => a - b);
  return Math.max(floor, s[Math.min(s.length - 1, Math.floor(q * s.length))]);
}
function f0(x) {
  return x === null || x === undefined || Number.isNaN(x) ? "—" : Math.round(x).toString();
}
function mb(x) {
  return (x / 1048576).toFixed(1);
}

function summarize(r) {
  const L = [];
  L.push(`# 性能读数（${r.when} · ${r.mode === "build" ? "生产构建" : "开发服务器"} · ${r.runs} 趟 · loadavg 开头 ${r.load.start.map((x) => x.toFixed(1)).join("/")} 结尾 ${r.load.end.map((x) => x.toFixed(1)).join("/")}）`);
  L.push("");
  if (r.boot.length) {
    L.push(`开页到安静：p50 ${f0(pct(r.boot.map((b) => b.ms), 0.5))} ms · 安静时 1.5 s 的 CPU p50 ${f0(pct(r.boot.map((b) => b.idleCpu ?? 0), 0.5))} ms`);
    L.push(`开页安静后还没建卡的 tab p50 ${f0(pct(r.boot.map((b) => b.virgin ?? 0), 0.5))} 个 · DOM 节点 p50 ${f0(pct(r.boot.map((b) => b.Nodes), 0.5))} · JS 堆 p50 ${mb(pct(r.boot.map((b) => b.JSHeapUsedSize), 0.5))} MB`);
    L.push("");
  }
  const groups = [
    ["冷切（没建过卡）", r.switch.filter((x) => x.pass === "cold")],
    ["热切（建过）", r.switch.filter((x) => x.pass === "warm")],
    ["热切 · 长会话（≥200 轮）", r.switch.filter((x) => x.pass === "warm" && (x.turns ?? 0) >= 200)],
    ["冷切 · 长会话（≥200 轮）", r.switch.filter((x) => x.pass === "cold" && (x.turns ?? 0) >= 200)],
  ];
  if (r.switch.length) {
    L.push("## 切一下（p50 / p95，ms；次数）");
    L.push("");
    L.push("| 组 | 次 | CPU（浏览器全部进程） | 按下到画出 | 同步段 | 稳定 | 长任务个 | 长任务合计 | 新建节点 | 布局次 | 样式次 | 布局ms | 样式ms | 脚本ms |");
    L.push("|---|---|---|---|---|---|---|---|---|---|---|---|---|---|");
    for (const [name, xs] of groups) {
      if (!xs.length) continue;
      const pp = (k) => `${f0(pct(xs.map((x) => x[k] ?? 0), 0.5))} / ${f0(pct(xs.map((x) => x[k] ?? 0), 0.95))}`;
      L.push(`| ${name} | ${xs.length} | ${pp("cpu")} | ${pp("inp")} | ${pp("sync")} | ${pp("settle")} | ${pp("ltN")} | ${pp("ltMs")} | ${pp("nodesOn")} | ${pp("layouts")} | ${pp("styles")} | ${pp("layoutMs")} | ${pp("styleMs")} | ${pp("scriptMs")} |`);
    }
    L.push("");
  }
  if (r.rapid.length) {
    L.push("## 连续快速切（20 下 / 每 200 ms；每串一行的 p50 / 最大）");
    L.push("");
    const xs = r.rapid;
    const pm = (k) => `${f0(pct(xs.map((x) => x[k]), 0.5))} / ${f0(Math.max(...xs.map((x) => x[k])))}`;
    L.push("| 串数 | CPU | 按下到画出 p50 | 按下到画出 最大 | 输入排队 最大 | 长任务个 | 长任务合计 | 最长长任务 | 后台流新建节点 | 末下到安静 | 布局ms | 样式ms | 脚本ms |");
    L.push("|---|---|---|---|---|---|---|---|---|---|---|---|---|");
    L.push(`| ${xs.length} | ${pm("cpu")} | ${pm("inpP50")} | ${pm("inpMax")} | ${pm("delayMax")} | ${pm("ltN")} | ${pm("ltMs")} | ${pm("ltMax")} | ${pm("nodesOff")} | ${pm("afterLast")} | ${pm("layoutMs")} | ${pm("styleMs")} | ${pm("scriptMs")} |`);
    L.push("");
    L.push("JS 堆（强制 GC 后，MB）每趟三串：" + groupBy(xs, (x) => x.run).map((g) => g.map((x) => `${mb(x.heap0)}→${mb(x.heap1)}`).join(" · ")).join("；"));
    L.push("");
  }
  if (r.keys?.length) {
    L.push("## 按住「下一个 tab」（30 下 / 每 40 ms；各串 p50 / 最大）");
    L.push("");
    const xs = r.keys;
    const pm = (k) => `${f0(pct(xs.map((x) => x[k]), 0.5))} / ${f0(Math.max(...xs.map((x) => x[k])))}`;
    L.push("| 串数 | CPU | 按下到画出 p50 | 按下到画出 最大 | 输入排队 最大 | >50ms 帧 | 最长帧 | 长任务个 | 长任务合计 | 后台流新建节点 | 末下到安静 | 布局ms | 样式ms | 脚本ms |");
    L.push("|---|---|---|---|---|---|---|---|---|---|---|---|---|---|");
    L.push(`| ${xs.length} | ${pm("cpu")} | ${pm("inpP50")} | ${pm("inpMax")} | ${pm("delayMax")} | ${pm("framesOver50")} | ${pm("frameMax")} | ${pm("ltN")} | ${pm("ltMs")} | ${pm("nodesOff")} | ${pm("afterLast")} | ${pm("layoutMs")} | ${pm("styleMs")} | ${pm("scriptMs")} |`);
    L.push("");
  }
  if (r.long.length) {
    L.push("## 长会话（最长那条，冷切进去 ＋ 滚轮往上 60 下）");
    L.push("");
    L.push("| 趟 | 首屏 CPU | 滚动 CPU | 按下到画出 | 稳定 | 首屏长任务合计 | 帧 p50 | 帧 p95 | 最长帧 | >50ms 帧 | 滚动中长任务合计 | 最长长任务 | 新建节点 | 布局ms | 样式ms | 脚本ms |");
    L.push("|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|");
    for (const x of r.long) {
      L.push(`| ${x.run + 1} | ${f0(x.first.cpu)} | ${f0(x.cpu)} | ${f0(x.first.inp)} | ${f0(x.first.settle)} | ${f0(x.first.ltMs)} | ${f0(x.frameP50)} | ${f0(x.frameP95)} | ${f0(x.frameMax)} | ${x.framesOver50} | ${f0(x.ltMs)} | ${f0(x.ltMax)} | ${x.nodes} | ${f0(x.layoutMs)} | ${f0(x.styleMs)} | ${f0(x.scriptMs)} |`);
    }
    L.push("");
  }
  return L.join("\n");
}

function groupBy(xs, key) {
  const m = new Map();
  for (const x of xs) {
    const k = key(x);
    if (!m.has(k)) m.set(k, []);
    m.get(k).push(x);
  }
  return [...m.values()];
}

// ─────────────────────────────────────────── 杂项 ───────────────────────────────────────────

/**
 * 一棵进程树（浏览器连同它起的渲染 / GPU 进程）到此刻一共用了多少 CPU 毫秒：每个线程 `/proc/<pid>/task/<tid>/schedstat`
 * 第一格（纳秒，在 CPU 上跑的时长）相加。机器忙时墙钟会被别的活拉长，CPU 时长基本不受影响 ⇒ 两样都记。
 */
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
          // 按线程记最后一次读数、只往上加：中途退掉的进程（关页时它的渲染进程）留着最后的读数，合计不倒着走
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
  // 机器忙时浏览器起来要好一阵 ⇒ 最多等 60 s
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
    if (["runs", "out", "only", "css", "merge", "profile", "eval", "presses"].includes(k)) o[k] = argv[++i];
    else if (k === "trace" && argv[i + 1] && !argv[i + 1].startsWith("--")) o[k] = argv[++i]; // `--trace`（热切那几下）/ `--trace keys`（按住切那一串）
    else o[k] = true;
  }
  return o;
}

/**
 * 性能台架（设置窗 · 长开内存）：量设置窗开窗 / 每一页切换 / 滚动 / 扩展页筛选 / 关了再开，以及主窗口长开几轮的内存走势。
 *
 *   node tests/shots/perf/settings-bench.mjs [--runs 2] [--out <目录>] [--only open,pages,scroll,filter,reopen,soak] [--cycles 20]
 *   node tests/shots/perf/settings-bench.mjs --eval <表达式> [--page <导航项 id>]   # 调试：点那一页（缺省扩展）之后在页里求值
 *   node tests/shots/perf/settings-bench.mjs --dev --cpuprofile <导航项 id> [--out <目录>]   # 点那一页（首次可见；`--again` ＝ 回来那一下）录一份 CPU 画像，打印自身耗时前 25 的函数
 *
 * 环境与 `bench.mjs` 同一套（生产构建 ＋ 页里假后端 ＋ 无头 Chromium，HOME 隔离进 `.build/perf-sandbox/`；不起后端、不起 claude、不碰 tmux）。
 * 世界：设置窗 `settings-world.ts`（十几台机器 · 二十几个账号 · 三十条规则 · 几百个扩展）；长开那一项用主窗口 `world.ts`。
 *
 * 量法：
 * - **开窗**（open）：新开页到导航第一项出来 · 到机器子页全注册 · 到安静；DOMContentLoaded 之后的长任务；开窗 15 s 之后 3 s 的 CPU（空闲还在干活？探针那条 rAF 链停了之后才量）；节点数 · JS 堆。
 * - **切页**（pages）：导航每一项（顶层页 ＋ 每台机器页）真点两遍（第一遍 ＝ 首次可见、发 I/O 建内容；第二遍 ＝ 回来），
 *   再在本机页里把横向几栏各点两遍。每下：按下到画出（Event Timing）· 同步段 · 稳定（最后一个长任务结束）· 长任务 · 新建节点 · 布局 / 样式 / 脚本毫秒。
 * - **滚动**（scroll）：机器列表 · 扩展 · 本机账号栏 · 本机轮换栏，各在内容区滚轮往下 30 下（每下 400 px、50 ms），量帧间隔与长任务。
 * - **筛选**（filter）：扩展页搜索框逐字敲「tool-01」（每字 80 ms），每键按下到画出 · 长任务；再一键清空。
 * - **关了再开**（reopen）：Ctrl+W 藏窗 → 窗口重新拿到焦点（壳的那一帧）＝ 重跑一遍打开；`--cycles` 轮，每轮之后强制 GC 记 JS 堆 · 节点 · 监听器数。
 * - **长开**（soak）：主窗口开着，每轮按住「下一个 tab」走一圈 ＋ 开关命令面板，`--cycles` 轮，每轮之后强制 GC 记 JS 堆 · 节点 · 监听器数（越用越涨 ＝ 漏）。
 */
import { spawn } from "node:child_process";
import { mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { Cdp, Page, sleep } from "../cdp.mjs";
import { cpuMs, devtoolsUrl, findChrome, freePort, pct } from "./harness.mjs";

const here = path.dirname(fileURLToPath(import.meta.url));
const repo = path.resolve(here, "../../..");
const args = parseArgs(process.argv.slice(2));
const runs = Number(args.runs ?? 2);
const cycles = Number(args.cycles ?? 20);
const out = path.resolve(args.out ?? path.join(repo, ".build/perf-settings"));
const sandbox = path.join(repo, ".build/perf-sandbox");
const only = new Set(String(args.only ?? "open,pages,scroll,filter,reopen,soak").split(","));
/** 本机那一页（`remote-section.ts::LOCAL_MACHINE_PAGE_ID`）。 */
const LOCAL_PAGE = "machine:（本机）";
const probe = readFileSync(path.join(here, "probe.js"), "utf8");

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
  for (const k of ["XDG_CONFIG_HOME", "XDG_CACHE_HOME", "XDG_DATA_HOME", "XDG_RUNTIME_DIR", "DBUS_SESSION_BUS_ADDRESS"]) delete env[k];
  return env;
}

const load0 = os.loadavg();
const port = await freePort();
const srv = spawn(process.execPath, [path.join(here, "serve.mjs"), String(port), ...(args.dev ? ["--dev"] : [])], { cwd: repo, env: isolatedEnv({ CCM_SHOTS_SANDBOX: sandbox }), stdio: ["ignore", "pipe", "inherit"] });
children.push(srv);
await new Promise((resolve, reject) => {
  let buf = "";
  srv.stdout.on("data", (d) => {
    buf += String(d);
    if (buf.includes(`READY ${port}`)) resolve();
  });
  srv.on("exit", (code) => reject(new Error(`伺服起不来（退出码 ${code}）`)));
});
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
    "--password-store=basic",
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

const result = { when: new Date().toISOString(), runs, cycles, load: { start: load0 }, open: [], pages: [], scroll: [], filter: [], reopen: [], soak: [] };
const watchdog = (what, p) => Promise.race([p, new Promise((_, reject) => setTimeout(() => reject(new Error(`${what} 超过 8 分钟没跑完`)), 8 * 60_000))]);
if (args.eval) {
  // 调试：开设置窗、点 `--page` 那一页（缺省扩展），在页里求一段表达式
  const { page } = await openWin("settings", "perf-settings");
  await measuredClick(page, navSel(String(args.page ?? "ext")), 2500);
  console.log(JSON.stringify(await page.eval(String(args.eval))));
  cdp.close();
  cleanup();
  process.exit(0);
}
if (args.cpuprofile) {
  // 开发服务器下函数名是源码里的名字：看清那一下的时间花在谁身上
  const { page } = await openWin("settings", "perf-settings");
  if (args.again) {
    // `--again`：先去过一次、回到机器列表，录的是「回来」那一下
    await measuredClick(page, navSel(String(args.cpuprofile)), 2500);
    await measuredClick(page, navSel("machines"), 1000);
  }
  await page.send("Profiler.enable");
  await page.send("Profiler.setSamplingInterval", { interval: 200 });
  await page.send("Profiler.start");
  await measuredClick(page, navSel(String(args.cpuprofile)), 2000);
  const { profile: prof } = await page.send("Profiler.stop");
  writeFileSync(path.join(out, "click.cpuprofile"), JSON.stringify(prof));
  const self = new Map();
  const dt = prof.timeDeltas;
  const byId = new Map(prof.nodes.map((n) => [n.id, n]));
  prof.samples.forEach((id, i) => {
    const n = byId.get(id);
    const f = n.callFrame;
    const k = `${f.functionName || "(匿名)"} ${f.url.split("/").slice(-2).join("/")}:${f.lineNumber + 1}`;
    self.set(k, (self.get(k) ?? 0) + (dt[i] ?? 0) / 1000);
  });
  for (const [k, v] of [...self].sort((a, b) => b[1] - a[1]).slice(0, 25)) console.log(`${v.toFixed(1).padStart(8)} ms  ${k}`);
  cdp.close();
  cleanup();
  process.exit(0);
}
for (let r = 0; r < runs; r++) {
  if (only.has("open")) result.open.push(await watchdog("开窗", benchOpen(r)));
  if (only.has("pages")) result.pages.push(...(await watchdog("切页", benchPages(r))));
  if (only.has("scroll")) result.scroll.push(...(await watchdog("滚动", benchScroll(r))));
  if (only.has("filter")) result.filter.push(await watchdog("筛选", benchFilter(r)));
}
if (only.has("reopen")) result.reopen = await benchReopen();
if (only.has("soak")) result.soak = await benchSoak();
result.load.end = os.loadavg();
writeFileSync(path.join(out, "perf.json"), JSON.stringify(result, null, 1));
const table = summarize(result);
writeFileSync(path.join(out, "perf.md"), table);
console.log(table);
cdp.close();
cleanup();
process.exit(0);

// ─────────────────────────────────────────── 共用 ───────────────────────────────────────────

async function metrics(page) {
  const { metrics: m } = await page.send("Performance.getMetrics");
  const o = {};
  for (const x of m) o[x.name] = x.value;
  return o;
}

/** 强制 GC 两次之后的 JS 堆 · 文档里的节点 · 监听器数（`Memory.getDOMCounters`：脱离文档但还被引用的节点也算在内）。 */
async function memAfterGc(page) {
  await page.send("HeapProfiler.collectGarbage");
  await sleep(200);
  await page.send("HeapProfiler.collectGarbage");
  const c = await page.send("Memory.getDOMCounters");
  const m = await metrics(page);
  return { heap: m.JSHeapUsedSize, nodes: c.nodes, listeners: c.jsEventListeners, docs: c.documents };
}

/** 开设置窗（或主窗口），等场景说开好了、主线程安静。 */
async function openWin(file, scene, extraProbe = "") {
  const page = await Page.open(cdp, file === "settings" ? 960 : 1280, file === "settings" ? 740 : 800);
  await page.send("Page.addScriptToEvaluateOnNewDocument", { source: `${probe}\n${extraProbe}` });
  await page.send("Performance.enable", { timeDomain: "timeTicks" });
  const t0 = Date.now();
  await Promise.race([page.goto(`${base}/${file}.html?scene=${scene}`), sleep(120_000).then(() => Promise.reject(new Error("开页 2 分钟没等到 load")))]);
  await page.waitFor("window.__shots && window.__shots.state !== 'booting'", 180_000);
  const st = await page.eval("window.__shots.state");
  if (st !== "done") throw new Error(`场景没起来：${await page.eval("window.__shots.error")}\n${page.consoleLines.slice(-8).join("\n")}`);
  const ready = Date.now() - t0;
  const quiet = await page.eval("__perf.quiet(1000, 60000)");
  return { page, ready, quiet };
}

async function clickAt(page, x, y) {
  await page.send("Input.dispatchMouseEvent", { type: "mouseMoved", x, y });
  await page.send("Input.dispatchMouseEvent", { type: "mousePressed", x, y, button: "left", clickCount: 1 });
  await page.send("Input.dispatchMouseEvent", { type: "mouseReleased", x, y, button: "left", clickCount: 1 });
}

async function key(page, k, code, vk, modifiers = 0) {
  await page.send("Input.dispatchKeyEvent", { type: "keyDown", key: k, code, windowsVirtualKeyCode: vk, nativeVirtualKeyCode: vk, modifiers, ...(k.length === 1 && !modifiers ? { text: k } : {}) });
  await page.send("Input.dispatchKeyEvent", { type: "keyUp", key: k, code, windowsVirtualKeyCode: vk, nativeVirtualKeyCode: vk, modifiers });
}

/** 某个元素滚进视野、给出中心坐标（看不见 ⇒ null）。 */
async function centerOf(page, sel) {
  return page.eval(`(() => { const e = document.querySelector(${JSON.stringify(sel)}); if (!e) return null; e.scrollIntoView({ block: 'nearest' }); const b = e.getBoundingClientRect(); if (b.width === 0) return null; return { x: b.left + b.width / 2, y: b.top + b.height / 2 }; })()`);
}

/** 点一下（按选择器）、看 `watchMs`，交回这一下的读数。 */
async function measuredClick(page, sel, watchMs = 1500) {
  const waited = await page.eval("__perf.quiet(300, 5000)");
  const at = await centerOf(page, sel);
  if (!at) return null;
  const m0 = await metrics(page);
  const since = await page.eval("performance.now()");
  const c0 = cpuMs(browser.pid);
  await clickAt(page, at.x, at.y);
  await sleep(watchMs);
  const cpu = cpuMs(browser.pid) - c0;
  const m1 = await metrics(page);
  const w = await page.eval(`__perf.since(${since})`);
  const click = w.clicks[0] ?? null;
  const ev = w.ev.find((e) => e.name === "click" || e.name === "pointerup");
  const t0c = click ? click.t0 : since;
  const lts = w.lt.filter((x) => x.s + x.d >= t0c);
  const lastLt = lts.length ? Math.max(...lts.map((x) => x.s + x.d)) - t0c : 0;
  return {
    waited,
    cpu,
    inp: ev ? ev.d : 16,
    sync: click?.sync ?? null,
    settle: Math.max(lastLt, click?.frame ?? 0),
    ltN: lts.length,
    ltMs: lts.reduce((a, x) => a + x.d, 0),
    ltMax: lts.reduce((a, x) => Math.max(a, x.d), 0),
    nodes: w.mut.reduce((a, x) => a + x.on + x.off, 0),
    layoutMs: (m1.LayoutDuration - m0.LayoutDuration) * 1000,
    styleMs: (m1.RecalcStyleDuration - m0.RecalcStyleDuration) * 1000,
    scriptMs: (m1.ScriptDuration - m0.ScriptDuration) * 1000,
    domNodes: m1.Nodes,
  };
}

function navSel(id) {
  return `.settings-shell:not(.settings-shell-h) > .settings-nav .settings-nav-item[data-route-id="${id}"]`;
}
function tabSel(id) {
  return `.settings-page:not([hidden]) .settings-shell-h .settings-nav-item[data-route-id="${id}"]`;
}
// ─────────────────────────────────────────── 各项 ───────────────────────────────────────────

async function benchOpen(run) {
  const firstNav = ";(() => { const mo = new MutationObserver(() => { if (document.querySelector('.settings-nav-item')) { window.__firstNav = performance.now(); mo.disconnect(); } }); document.addEventListener('DOMContentLoaded', () => mo.observe(document.body, { childList: true, subtree: true })); })();";
  const c0 = cpuMs(browser.pid);
  const { page, ready, quiet } = await openWin("settings", "perf-settings", firstNav);
  const openCpu = cpuMs(browser.pid) - c0;
  const o = await page.eval("(() => { const dcl = performance.getEntriesByType('navigation')[0]?.domContentLoadedEventStart ?? 0; const lt = __perf.lt.filter((x) => x.s >= dcl); return { dcl, firstNav: window.__firstNav ?? null, ltN: lt.length, ltMs: lt.reduce((a, x) => a + x.d, 0), ltMax: lt.reduce((a, x) => Math.max(a, x.d), 0), nodes: document.getElementsByTagName('*').length }; })()");
  // 探针开窗那 15 s 自己起着一条 rAF 链（记开窗帧间隔）⇒ 空闲 CPU 等它停了再量
  await page.eval("new Promise((r) => setTimeout(r, Math.max(0, 15500 - (performance.now() - (performance.getEntriesByType('navigation')[0]?.domContentLoadedEventStart ?? 0)))))");
  const c1 = cpuMs(browser.pid);
  await sleep(3000);
  const idleCpu = cpuMs(browser.pid) - c1;
  const mem = await memAfterGc(page);
  await page.close();
  console.log(`  开窗 第 ${run + 1} 趟：导航 ${Math.round(o.firstNav - o.dcl)} ms · 开好 ${ready} ms`);
  return { run, ready, quiet, openCpu, idleCpu, ...o, ...mem };
}

async function benchPages(run) {
  const { page } = await openWin("settings", "perf-settings");
  const ids = await page.eval("[...document.querySelectorAll('.settings-shell:not(.settings-shell-h) > .settings-nav .settings-nav-item')].map((e) => e.dataset.routeId)");
  const rows = [];
  for (const pass of ["first", "again"]) {
    for (const id of ids) {
      const m = await measuredClick(page, navSel(id));
      if (m) rows.push({ run, pass, kind: id.startsWith("machine:") ? "machine" : "top", id, ...m });
    }
    // 本机页里横向几栏
    await measuredClick(page, navSel(LOCAL_PAGE));
    const tabs = await page.eval("[...document.querySelectorAll('.settings-page:not([hidden]) .settings-shell-h .settings-nav-item')].map((e) => e.dataset.routeId)");
    for (const id of [...tabs.slice(1), tabs[0]]) {
      const m = await measuredClick(page, tabSel(id));
      if (m) rows.push({ run, pass, kind: "tab", id, ...m });
    }
  }
  await page.close();
  console.log(`  切页 第 ${run + 1} 趟：${rows.length} 下`);
  return rows;
}

async function scrollOnce(page, label) {
  await page.eval("__perf.quiet(500, 8000)");
  const at = await page.eval("(() => { const s = document.querySelector('.settings-shell:not(.settings-shell-h) > .settings-content'); const b = s.getBoundingClientRect(); return { x: b.left + b.width / 2, y: b.top + b.height / 2, h: s.scrollHeight }; })()");
  const since = await page.eval("performance.now()");
  await page.eval("__perf.frameStart()");
  const c0 = cpuMs(browser.pid);
  for (let k = 0; k < 30; k++) {
    await page.send("Input.dispatchMouseEvent", { type: "mouseWheel", x: at.x, y: at.y, deltaX: 0, deltaY: 400 });
    await sleep(50);
  }
  await sleep(600);
  const cpu = cpuMs(browser.pid) - c0;
  const frames = await page.eval("__perf.frameStop()");
  const w = await page.eval(`__perf.since(${since})`);
  return { label, height: at.h, cpu, frameP50: pct(frames, 0.5), frameP95: pct(frames, 0.95), frameMax: Math.max(0, ...frames), over50: frames.filter((d) => d > 50).length, ltMs: w.lt.reduce((a, x) => a + x.d, 0) };
}

async function benchScroll(run) {
  const { page } = await openWin("settings", "perf-settings");
  const rows = [];
  await measuredClick(page, navSel("machines"));
  rows.push({ run, ...(await scrollOnce(page, "机器列表")) });
  await measuredClick(page, navSel("ext"), 2500);
  rows.push({ run, ...(await scrollOnce(page, "扩展")) });
  await measuredClick(page, navSel(LOCAL_PAGE));
  const tabs = await page.eval("[...document.querySelectorAll('.settings-page:not([hidden]) .settings-shell-h .settings-nav-item')].map((e) => e.dataset.routeId)");
  for (const id of tabs) {
    await measuredClick(page, tabSel(id), 2000);
    rows.push({ run, ...(await scrollOnce(page, `本机 · ${id.split("#").pop()}`)) });
  }
  await page.close();
  console.log(`  滚动 第 ${run + 1} 趟`);
  return rows;
}

async function benchFilter(run) {
  const { page } = await openWin("settings", "perf-settings");
  await measuredClick(page, navSel("ext"), 2500);
  await page.eval("__perf.quiet(500, 8000)");
  const at = await centerOf(page, ".settings-page:not([hidden]) .ext-search");
  await clickAt(page, at.x, at.y);
  await sleep(300);
  const since = await page.eval("performance.now()");
  const c0 = cpuMs(browser.pid);
  for (const ch of "tool-01") {
    const t = Date.now();
    await key(page, ch, ch === "-" ? "Minus" : /\d/.test(ch) ? `Digit${ch}` : `Key${ch.toUpperCase()}`, ch.toUpperCase().charCodeAt(0));
    await sleep(Math.max(0, 80 - (Date.now() - t)));
  }
  await page.eval("__perf.quiet(300, 5000)");
  const rowsLeft = await page.eval("document.querySelectorAll('.settings-page:not([hidden]) .ext-row:not(.ext-head)').length");
  // 一键清空（全选 ＋ 退格）：整张表回来
  await key(page, "a", "KeyA", 65, 2);
  const t1 = await page.eval("performance.now()");
  await key(page, "Backspace", "Backspace", 8);
  await page.eval("__perf.quiet(300, 5000)");
  const cpu = cpuMs(browser.pid) - c0;
  const w = await page.eval(`__perf.since(${since})`);
  const keys = w.ev.filter((e) => e.name === "keydown" || e.name === "input" || e.name === "keypress");
  const typed = keys.filter((e) => e.s < t1);
  const cleared = keys.filter((e) => e.s >= t1);
  await page.close();
  console.log(`  筛选 第 ${run + 1} 趟`);
  return {
    run,
    cpu,
    rowsLeft,
    keyP50: pct(typed.map((e) => e.d), 0.5, 16),
    keyMax: Math.max(16, ...typed.map((e) => e.d)),
    clearMax: Math.max(16, ...cleared.map((e) => e.d)),
    ltN: w.lt.length,
    ltMs: w.lt.reduce((a, x) => a + x.d, 0),
    ltMax: w.lt.reduce((a, x) => Math.max(a, x.d), 0),
    nodes: w.mut.reduce((a, x) => a + x.on + x.off, 0),
  };
}

/** 关了再开：Ctrl+W（藏）→ 壳推一帧「拿到焦点」（重跑打开）；每轮还在机器 · 扩展两页之间切一下。 */
async function benchReopen() {
  const { page } = await openWin("settings", "perf-settings");
  const rows = [{ cycle: 0, ...(await memAfterGc(page)) }];
  for (let c = 1; c <= cycles; c++) {
    await key(page, "w", "KeyW", 87, 2);
    await sleep(150);
    await page.eval("window.__TAURI_INTERNALS__.invoke('plugin:event|emit', { event: 'tauri://focus', payload: true })");
    await page.eval("__perf.quiet(300, 8000)");
    await measuredClick(page, navSel("ext"), 600);
    await measuredClick(page, navSel(LOCAL_PAGE), 600);
    await measuredClick(page, navSel("machines"), 600);
    rows.push({ cycle: c, ...(await memAfterGc(page)) });
  }
  const opened = await page.eval("document.querySelectorAll('.settings-nav-item').length");
  await page.close();
  console.log(`  关了再开 ${cycles} 轮（导航项 ${opened}）`);
  return rows;
}

/** 主窗口长开：每轮按住「下一个 tab」走一圈 ＋ 开关命令面板。 */
async function benchSoak() {
  const { page } = await openWin("index", "perf-tabs");
  const n = await page.eval("document.querySelectorAll('#tab-bar .tab').length");
  const rows = [{ cycle: 0, t: 0, ...(await memAfterGc(page)) }];
  const t0 = Date.now();
  for (let c = 1; c <= cycles; c++) {
    for (let k = 0; k < n; k++) {
      await key(page, "]", "BracketRight", 221);
      await sleep(120);
    }
    await page.eval("__perf.quiet(300, 8000)");
    await key(page, "k", "KeyK", 75, 2);
    await sleep(300);
    await key(page, "Escape", "Escape", 27);
    await page.eval("__perf.quiet(300, 8000)");
    rows.push({ cycle: c, t: Date.now() - t0, ...(await memAfterGc(page)) });
  }
  await page.close();
  console.log(`  主窗口长开 ${cycles} 轮`);
  return rows;
}

// ─────────────────────────────────────────── 汇总 ───────────────────────────────────────────

function f0(x) {
  return x === null || x === undefined || Number.isNaN(x) ? "—" : Math.round(x).toString();
}
function mb(x) {
  return (x / 1048576).toFixed(1);
}

function summarize(r) {
  const L = [];
  L.push(`# 设置窗 · 长开读数（${r.when} · ${r.runs} 趟 · loadavg 开头 ${r.load.start.map((x) => x.toFixed(1)).join("/")} 结尾 ${r.load.end.map((x) => x.toFixed(1)).join("/")}）`);
  L.push("");
  if (r.open.length) {
    L.push("## 开设置窗");
    L.push("");
    L.push("| 趟 | 导航出来（DCL 起 ms） | 开好（开页起 ms） | 开窗 CPU | 长任务个 | 长任务合计 | 最长 | 空闲 3 s CPU | 节点 | JS 堆 MB | 监听器 |");
    L.push("|---|---|---|---|---|---|---|---|---|---|---|");
    for (const x of r.open) L.push(`| ${x.run + 1} | ${f0(x.firstNav - x.dcl)} | ${x.ready} | ${f0(x.openCpu)} | ${x.ltN} | ${f0(x.ltMs)} | ${f0(x.ltMax)} | ${f0(x.idleCpu)} | ${x.nodes} | ${mb(x.heap)} | ${x.listeners} |`);
    L.push("");
  }
  if (r.pages.length) {
    L.push("## 切页（每组 p50 / 最大，ms）");
    L.push("");
    L.push("| 页 | 遍 | 次 | CPU | 按下到画出 | 同步段 | 稳定 | 长任务合计 | 最长长任务 | 新建节点 | 布局ms | 样式ms | 脚本ms |");
    L.push("|---|---|---|---|---|---|---|---|---|---|---|---|---|");
    const keys = new Map();
    for (const x of r.pages) {
      const name = x.kind === "machine" ? (x.id === LOCAL_PAGE ? "本机页" : "其余机器页") : x.id;
      const k = `${name}|${x.pass}`;
      if (!keys.has(k)) keys.set(k, []);
      keys.get(k).push(x);
    }
    for (const [k, xs] of keys) {
      const [name, pass] = k.split("|");
      const pm = (f) => `${f0(pct(xs.map((x) => x[f] ?? 0), 0.5))} / ${f0(Math.max(...xs.map((x) => x[f] ?? 0)))}`;
      L.push(`| ${name} | ${pass === "first" ? "首次" : "回来"} | ${xs.length} | ${pm("cpu")} | ${pm("inp")} | ${pm("sync")} | ${pm("settle")} | ${pm("ltMs")} | ${pm("ltMax")} | ${pm("nodes")} | ${pm("layoutMs")} | ${pm("styleMs")} | ${pm("scriptMs")} |`);
    }
    L.push("");
  }
  if (r.scroll.length) {
    L.push("## 滚动（内容区滚轮往下 30 下）");
    L.push("");
    L.push("| 页 | 趟 | 内容高 | CPU | 帧 p50 | 帧 p95 | 最长帧 | >50ms 帧 | 长任务合计 |");
    L.push("|---|---|---|---|---|---|---|---|---|");
    for (const x of r.scroll) L.push(`| ${x.label} | ${x.run + 1} | ${x.height} | ${f0(x.cpu)} | ${f0(x.frameP50)} | ${f0(x.frameP95)} | ${f0(x.frameMax)} | ${x.over50} | ${f0(x.ltMs)} |`);
    L.push("");
  }
  if (r.filter.length) {
    L.push("## 扩展页筛选（逐字敲 7 个字 ＋ 一键清空）");
    L.push("");
    L.push("| 趟 | CPU | 每键 p50 | 每键最大 | 清空那一键 | 长任务个 | 长任务合计 | 最长 | 新建节点 | 剩几行 |");
    L.push("|---|---|---|---|---|---|---|---|---|---|");
    for (const x of r.filter) L.push(`| ${x.run + 1} | ${f0(x.cpu)} | ${f0(x.keyP50)} | ${f0(x.keyMax)} | ${f0(x.clearMax)} | ${x.ltN} | ${f0(x.ltMs)} | ${f0(x.ltMax)} | ${x.nodes} | ${x.rowsLeft} |`);
    L.push("");
  }
  const trend = (title, xs) => {
    if (!xs.length) return;
    L.push(`## ${title}（强制 GC 后）`);
    L.push("");
    L.push("| 轮 | JS 堆 MB | 节点（含脱离文档仍被引用的） | 监听器 |");
    L.push("|---|---|---|---|");
    for (const x of xs.filter((_, i) => i === 0 || i === xs.length - 1 || i % 5 === 0)) L.push(`| ${x.cycle} | ${mb(x.heap)} | ${x.nodes} | ${x.listeners} |`);
    const a = xs[1] ?? xs[0];
    const b = xs[xs.length - 1];
    const per = xs.length > 2 ? (k) => ((b[k] - a[k]) / (xs.length - 2)).toFixed(k === "heap" ? 0 : 1) : () => "—";
    L.push("");
    L.push(`第 1 轮之后每轮平均涨：堆 ${per("heap")} B · 节点 ${per("nodes")} · 监听器 ${per("listeners")}`);
    L.push("");
  };
  trend("设置窗关了再开", r.reopen);
  trend("主窗口长开", r.soak);
  return L.join("\n");
}

function parseArgs(argv) {
  const o = {};
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (!a.startsWith("--")) continue;
    const k = a.slice(2);
    if (["runs", "out", "only", "cycles", "cpuprofile", "eval", "page"].includes(k)) o[k] = argv[++i];
    else o[k] = true;
  }
  return o;
}

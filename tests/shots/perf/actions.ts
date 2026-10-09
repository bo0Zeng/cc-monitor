/**
 * 性能台架 perfA 那几项的页内动作：主窗口其余（长会话滚动 · 跳轮 · 实时来消息 · 展开过程 · 查找 · 大纲 · 「需要你」·
 * 命令面板 · 账号面板 · 新建会话框 · 右键菜单 · 终端抽屉 · agent / 任务抽屉）、独立查看窗、agent 窗口。
 *
 * 动作全在页里跑（派发真 DOM 事件，产品照常处理），两个引擎（Chromium · WebKitGTK）同一套；页外驱动
 * （`bench-a.mjs` · `webkit-a.py`）只管开页、调 `__pa.run(名字)`、在外面记进程树 CPU。
 *
 * 每一项交回：每一步「派发到下一帧画完」（两个 rAF；同步段另记）· 整段的帧间隔（>50 ms 帧数 / 超出合计 / 最长）·
 * 长任务（只有 Chromium 有）· 新建节点 · 结束时 DOM 节点数。只给台架用；不进产品构建。
 */
import type { FakeBackend } from "../fake/backend";
import type { SessionStreamFrame } from "../../../src/frontend/ui/generated/SessionStreamFrame";
import { Convo } from "../fake/records";
import { turn } from "./world";
import { copyText } from "../../../src/frontend/ui/copy-table";

interface Perf {
  lt: { s: number; d: number }[];
  mut: { t: number; on: number; off: number }[];
  quiet(q: number, max: number): Promise<number>;
  frameStart(): void;
  frameStop(): number[];
  since(t: number): { lt: { s: number; d: number }[]; mut: { t: number; on: number; off: number }[] };
}
declare global {
  interface Window {
    __perf: Perf;
    __perfBackend?: FakeBackend;
    __pa?: unknown;
  }
}

const sleep = (ms: number): Promise<void> => new Promise((r) => setTimeout(r, ms));
const raf2 = (): Promise<void> => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(() => r())));
const $ = <T extends HTMLElement = HTMLElement>(sel: string): T | null => document.querySelector<T>(sel);

async function waitSel<T extends HTMLElement = HTMLElement>(sel: string, ms = 10_000): Promise<T> {
  const until = performance.now() + ms;
  for (;;) {
    const el = $<T>(sel);
    if (el) return el;
    if (performance.now() > until) throw new Error(`等不到 ${sel}`);
    await sleep(30);
  }
}

interface Step {
  what: string;
  sync: number;
  paint: number;
}

/** 一步：同步派发、量同步段，再等两个 rAF（这一步引起的那一帧整帧画完）。 */
async function step(steps: Step[], what: string, fn: () => void): Promise<void> {
  const t0 = performance.now();
  fn();
  const sync = performance.now() - t0;
  await raf2();
  steps.push({ what, sync, paint: performance.now() - t0 });
}

function keyOn(target: EventTarget, k: string, mods: { ctrl?: boolean; shift?: boolean; alt?: boolean; code?: string } = {}): void {
  const init = { key: k, code: mods.code ?? (k.length === 1 ? `Key${k.toUpperCase()}` : k), bubbles: true, cancelable: true, ctrlKey: !!mods.ctrl, shiftKey: !!mods.shift, altKey: !!mods.alt };
  target.dispatchEvent(new KeyboardEvent("keydown", init));
  target.dispatchEvent(new KeyboardEvent("keyup", init));
}
const key = (k: string, mods: Parameters<typeof keyOn>[2] = {}): void => keyOn((document.activeElement as HTMLElement | null) ?? document.body, k, mods);

function clickEl(el: Element): void {
  const r = el.getBoundingClientRect();
  const at = { bubbles: true, cancelable: true, clientX: r.left + r.width / 2, clientY: r.top + r.height / 2, button: 0 };
  el.dispatchEvent(new PointerEvent("pointerdown", at));
  el.dispatchEvent(new MouseEvent("mousedown", at));
  el.dispatchEvent(new PointerEvent("pointerup", at));
  el.dispatchEvent(new MouseEvent("mouseup", at));
  el.dispatchEvent(new MouseEvent("click", at));
}
function rightClickEl(el: Element): void {
  const r = el.getBoundingClientRect();
  el.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: r.left + 20, clientY: r.top + Math.min(r.height / 2, 12), button: 2 }));
}
function typeInto(el: HTMLInputElement | HTMLTextAreaElement, v: string): void {
  el.value = v;
  el.dispatchEvent(new Event("input", { bubbles: true }));
}
function byText(sel: string, text: string | RegExp): HTMLElement | null {
  for (const el of document.querySelectorAll<HTMLElement>(sel)) {
    const t = el.textContent ?? "";
    if (typeof text === "string" ? t.includes(text) : text.test(t)) return el;
  }
  return null;
}

const scroller = (): HTMLElement => {
  const s = $(".stream.active") ?? $(".session-viewer-stream");
  if (!s) throw new Error("没有消息流");
  return s;
};

function pct(xs: number[], q: number): number {
  if (xs.length === 0) return 0;
  const s = [...xs].sort((a, b) => a - b);
  return s[Math.min(s.length - 1, Math.floor(q * s.length))];
}

/** 一项：等安静 → 起帧链 → 跑 → 等安静 → 收读数。 */
async function measure(name: string, body: (steps: Step[]) => Promise<void>): Promise<Record<string, unknown>> {
  await window.__perf.quiet(500, 10_000);
  const since = performance.now();
  window.__perf.frameStart();
  const steps: Step[] = [];
  await body(steps);
  const settle = await window.__perf.quiet(300, 15_000);
  const frames = window.__perf.frameStop();
  const w = window.__perf.since(since);
  const over = frames.filter((d) => d > 50);
  const paints = steps.map((s) => s.paint);
  return {
    name,
    wall: performance.now() - since,
    settle,
    steps: steps.length,
    paintP50: pct(paints, 0.5),
    paintP95: pct(paints, 0.95),
    paintMax: Math.max(0, ...paints),
    syncMax: Math.max(0, ...steps.map((s) => s.sync)),
    worst: [...steps].sort((a, b) => b.paint - a.paint).slice(0, 3).map((s) => `${s.what}:${Math.round(s.paint)}`),
    frames: frames.length,
    frameP95: pct(frames, 0.95),
    frameMax: Math.max(0, ...frames),
    jankN: over.length,
    jankMs: over.reduce((a, d) => a + d - 16.7, 0),
    ltN: w.lt.length,
    ltMs: w.lt.reduce((a, x) => a + x.d, 0),
    ltMax: w.lt.reduce((a, x) => Math.max(a, x.d), 0),
    nodesNew: w.mut.reduce((a, x) => a + x.on + x.off, 0),
    dom: document.getElementsByTagName("*").length,
    heapMb: (performance as { memory?: { usedJSHeapSize: number } }).memory ? (performance as unknown as { memory: { usedJSHeapSize: number } }).memory.usedJSHeapSize / 1048576 : null,
  };
}

/** 最长那条会话（tab 标题里轮数最多的）切成当前。 */
async function toLongest(): Promise<void> {
  const tabs = [...document.querySelectorAll<HTMLElement>("#tab-bar .tab")];
  let best = tabs[0];
  let most = -1;
  for (const t of tabs) {
    const m = /t(\d+)-perf/.exec(t.textContent ?? "");
    const n = m ? Number(m[1]) : 0;
    if (n > most) {
      most = n;
      best = t;
    }
  }
  if (!best.classList.contains("active")) {
    clickEl(best);
    await sleep(800);
  }
  await window.__perf.quiet(500, 15_000);
}

async function closeOverlays(): Promise<void> {
  for (let i = 0; i < 3; i++) {
    key("Escape");
    await sleep(60);
  }
}

// ─────────────────────────── 主窗口 ───────────────────────────

/** 空闲：什么都不做 `ms`（CPU 在页外记）；交回此刻在跑的动画（谁在空闲时让页面一直重画）。 */
async function idle(ms = 5000): Promise<Record<string, unknown>> {
  await window.__perf.quiet(500, 10_000);
  // 探针开窗后 15 s 里自己在跑一条 rAF 链（记开窗那一段的帧）：等它停了再量空闲，免得把探针算进页面
  const nav = performance.getEntriesByType("navigation")[0] as PerformanceNavigationTiming | undefined;
  const left = (nav?.domContentLoadedEventStart ?? 0) + 15_500 - performance.now();
  if (left > 0) await sleep(left);
  const anims = (document.getAnimations?.() ?? []).filter((a) => a.playState === "running");
  const who = new Map<string, number>();
  for (const a of anims) {
    const t = (a.effect as KeyframeEffect | null)?.target as Element | null;
    const k = `${t?.tagName.toLowerCase() ?? "?"}.${(t?.className && typeof t.className === "string" ? t.className : "").split(" ").slice(0, 2).join(".")}${(a as CSSAnimation).animationName ? `@${(a as CSSAnimation).animationName}` : ""}${t && t.getClientRects().length === 0 ? "（不可见）" : ""}`;
    who.set(k, (who.get(k) ?? 0) + 1);
  }
  const t0 = performance.now();
  let timers = 0;
  // 空闲时页里还在跑的定时器 / rAF：数这一段里回调被叫了几次（包一层计数，结束拆掉）
  const st = window.setTimeout;
  const si = window.setInterval;
  const ra = window.requestAnimationFrame;
  const cnt: Record<string, number> = { timeout: 0, interval: 0, raf: 0 };
  window.setTimeout = ((f: TimerHandler, d?: number, ...r: unknown[]) => st(() => ((cnt.timeout += 1), typeof f === "function" ? f(...r) : undefined), d)) as typeof setTimeout;
  window.setInterval = ((f: TimerHandler, d?: number, ...r: unknown[]) => si(() => ((cnt.interval += 1), typeof f === "function" ? f(...r) : undefined), d)) as typeof setInterval;
  window.requestAnimationFrame = ((f: FrameRequestCallback) => ra((t) => ((cnt.raf += 1), f(t)))) as typeof requestAnimationFrame;
  const mo = new MutationObserver((rs) => (timers += rs.length));
  mo.observe(document.body, { subtree: true, childList: true, attributes: true, characterData: true });
  await new Promise((r) => st(r, ms));
  mo.disconnect();
  window.setTimeout = st;
  window.setInterval = si;
  window.requestAnimationFrame = ra;
  return { name: "idle", wall: performance.now() - t0, running: anims.length, who: [...who.entries()].sort((a, b) => b[1] - a[1]).slice(0, 12), mutations: timers, callbacks: cnt, dom: document.getElementsByTagName("*").length };
}

/** 长会话往上滚 60 下（每下 600 px、50 ms），再往下滚回去。 */
async function scroll(dir: "up" | "down"): Promise<Record<string, unknown>> {
  await toLongest();
  const s = scroller();
  if (dir === "down") s.scrollTop = Math.max(0, s.scrollHeight * 0.3);
  await sleep(300);
  return measure(`scroll-${dir}`, async (steps) => {
    for (let k = 0; k < 60; k++) {
      const t = performance.now();
      await step(steps, `${k}`, () => s.scrollBy(0, dir === "up" ? -600 : 600));
      await sleep(Math.max(0, 50 - (performance.now() - t)));
    }
  });
}

/** 跳轮：Alt+↑ 15 下（每 150 ms）· 点刻度 8 处 · End 回底。 */
async function jump(): Promise<Record<string, unknown>> {
  await toLongest();
  scroller().scrollTop = scroller().scrollHeight;
  await sleep(300);
  return measure("jump", async (steps) => {
    for (let k = 0; k < 15; k++) {
      await step(steps, "alt-up", () => key("ArrowUp", { alt: true }));
      await sleep(150);
    }
    const ticks = [...document.querySelectorAll<HTMLElement>(".turn-rail.active .turn-tick, .turn-rail:not([hidden]) .turn-tick")];
    for (const f of [0.1, 0.9, 0.3, 0.6, 0.05, 0.5, 0.75, 0.2]) {
      const t = ticks[Math.floor(f * ticks.length)];
      if (t) await step(steps, `tick${f}`, () => clickEl(t));
      await sleep(300);
    }
    await step(steps, "end", () => key("End", { code: "End" }));
    await sleep(500);
  });
}

/** 实时来消息：最长那条在底部，活卡出一轮（20 Hz 正文增量 5 s），然后那一轮落成记录（三轮，每轮一批），后台会话也同时在来。 */
async function stream(where: "active" | "background" = "active"): Promise<Record<string, unknown>> {
  await toLongest();
  const be = window.__perfBackend;
  if (!be) throw new Error("没有假后端句柄");
  const s = scroller();
  s.scrollTop = s.scrollHeight;
  await sleep(300);
  const active = be.world.sessions.reduce((a, b) => (b.records.length > a.records.length ? b : a));
  const target = where === "active" ? active : be.world.sessions[1];
  let n = 0;
  const ev = (e: unknown): unknown => ({ origin: target.origin, stream: target.sid, resp: 1, n: n++, ev: e });
  return measure(`stream-${where}`, async (steps) => {
    for (let round = 0; round < 3; round++) {
      await step(steps, "start", () => be.pushTap(target.origin, [ev({ t: "start", rid: `req_perf${round}` }), ev({ t: "block", i: 0, kind: "text" })]));
      for (let k = 0; k < 40; k++) {
        const t = performance.now();
        await step(steps, "delta", () => be.pushTap(target.origin, [ev({ t: "text", i: 0, s: `第 ${k} 段：把分页改成游标之后，调用点要跟着改，测试也要补。` })]));
        await sleep(Math.max(0, 50 - (performance.now() - t)));
      }
      await step(steps, "tool", () => be.pushTap(target.origin, [ev({ t: "block", i: 1, kind: "tool", tool: "Bash" })]));
      await sleep(200);
      // 这一轮落成记录（真后端：记录文件写了 ⇒ 一批 line 格）
      const c = new Convo(target.sid, target.cwd, "2026-10-02T08:00:00Z");
      turn(c, target.cwd, 1000 + round, false);
      const from = target.records.length;
      target.records.push(...c.records);
      const frames: SessionStreamFrame[] = c.records.map((message, i) => ({ line: { session_id: target.sid, cwd: target.cwd, path: `${target.cwd}/${target.sid}.jsonl`, seq: from + i, origin: target.origin, message } }) as SessionStreamFrame);
      await step(steps, "lines", () => be.pushFrames(target.origin, frames));
      await sleep(500);
    }
  });
}

/** 展开过程：最长那条里点开 8 轮的过程 · Ctrl+O 全部展开 / 收起。 */
async function expand(): Promise<Record<string, unknown>> {
  await toLongest();
  scroller().scrollTop = scroller().scrollHeight;
  await sleep(300);
  return measure("expand", async (steps) => {
    for (let k = 0; k < 8; k++) {
      const lines = [...document.querySelectorAll<HTMLElement>(".stream.active .proc-line")];
      const l = lines[lines.length - 1 - k];
      if (!l) break;
      l.scrollIntoView({ block: "center" });
      await sleep(100);
      await step(steps, "proc", () => l.click());
      await sleep(250);
    }
    await step(steps, "ctrl-o", () => key("o", { ctrl: true }));
    await sleep(1500);
    await step(steps, "ctrl-o-back", () => key("o", { ctrl: true }));
    await sleep(1000);
  });
}

/** 查找：Ctrl+F，逐字打「游标分页」（每 120 ms 一个字），回车往下跳 10 次，Esc。 */
async function find(): Promise<Record<string, unknown>> {
  await toLongest();
  return measure("find", async (steps) => {
    await step(steps, "ctrl-f", () => key("f", { ctrl: true }));
    const input = await waitSel<HTMLInputElement>('[data-role="session-find-panel"]:not([hidden]) [data-role="find-input"], [data-role="find-input"]');
    const word = "游标分页";
    for (let i = 1; i <= word.length; i++) {
      await step(steps, `type${i}`, () => typeInto(input, word.slice(0, i)));
      await sleep(120);
    }
    // 自动查（300 ms 去抖）之后结果到齐
    await sleep(800);
    for (let k = 0; k < 10; k++) {
      await step(steps, "enter", () => keyOn(input, "Enter", { code: "Enter" }));
      await sleep(200);
    }
    await step(steps, "esc", () => keyOn(input, "Escape", { code: "Escape" }));
    await sleep(300);
  });
}

/** 大纲：打开查找面板、切到大纲，点 6 条（跳到那一轮），收起。 */
async function outline(): Promise<Record<string, unknown>> {
  await toLongest();
  return measure("outline", async (steps) => {
    await step(steps, "ctrl-f", () => key("f", { ctrl: true }));
    const panel = await waitSel('[data-role="session-find-panel"]:not([hidden])');
    const tab = [...panel.querySelectorAll<HTMLElement>('[role="tab"]')][1];
    if (!tab) throw new Error("查找面板里没有大纲那一格");
    await step(steps, "outline", () => clickEl(tab));
    await sleep(800);
    const items = () => [...panel.querySelectorAll<HTMLElement>('button, [role="option"], li')].filter((e) => e.offsetParent !== null && !e.closest('[role="tablist"]'));
    for (const f of [0.1, 0.8, 0.4, 0.6, 0.2, 0.95]) {
      const xs = items();
      const it = xs[Math.floor(f * xs.length)];
      if (it) await step(steps, `item${f}`, () => clickEl(it));
      await sleep(400);
    }
    await step(steps, "esc", () => key("Escape"));
    await sleep(300);
  });
}

/** 「需要你」：Ctrl+J 跳下一个在等你的 6 下（每 400 ms）。 */
async function needs(): Promise<Record<string, unknown>> {
  return measure("needs", async (steps) => {
    for (let k = 0; k < 6; k++) {
      await step(steps, "ctrl-j", () => key("j", { ctrl: true }));
      await sleep(400);
    }
  });
}

/** 命令面板：Ctrl+K 开、逐字打「新建」「轮换」「设置」、Esc；三次。 */
async function palette(): Promise<Record<string, unknown>> {
  return measure("palette", async (steps) => {
    // 打的词取文案表里的字（命令面板里那几条就叫这个）
    for (const word of [copyText("newSession.title.plain"), copyText("acct.rot.title"), "打开设置"]) {
      await step(steps, "ctrl-k", () => key("k", { ctrl: true }));
      const input = await waitSel<HTMLInputElement>("[data-role=command-input]");
      for (let i = 1; i <= word.length; i++) {
        await step(steps, `type`, () => typeInto(input, word.slice(0, i)));
        await sleep(100);
      }
      await step(steps, "down", () => keyOn(input, "ArrowDown", { code: "ArrowDown" }));
      await step(steps, "esc", () => keyOn(input, "Escape", { code: "Escape" }));
      await sleep(300);
    }
  });
}

/** 账号面板：点状态栏账号开、时间轴 6h / 24h / 7d 各点一下、关；两次。 */
async function acct(): Promise<Record<string, unknown>> {
  return measure("acct", async (steps) => {
    for (let k = 0; k < 2; k++) {
      const chip = await waitSel(".status-account");
      await step(steps, "open", () => clickEl(chip));
      await waitSel('aside[role="dialog"]');
      await sleep(600);
      for (const v of ["7d", "24h", "6h"]) {
        const b = byText("[data-tl-view] button", v);
        if (b) await step(steps, `tl-${v}`, () => clickEl(b));
        await sleep(400);
      }
      await step(steps, "close", () => key("Escape"));
      await sleep(400);
    }
  });
}

/** 新建会话框：命令面板「新建会话」回车开框、等账号那一格填好、Esc；两次。 */
async function newSession(): Promise<Record<string, unknown>> {
  return measure("new-session", async (steps) => {
    for (let k = 0; k < 2; k++) {
      key("k", { ctrl: true });
      const input = await waitSel<HTMLInputElement>("[data-role=command-input]");
      typeInto(input, copyText("newSession.title.plain"));
      await sleep(200);
      await step(steps, "enter", () => keyOn(input, "Enter", { code: "Enter" }));
      await waitSel('[role="dialog"] button[aria-label="账号"]:not([data-value=""])');
      await sleep(400);
      await step(steps, "esc", () => key("Escape"));
      await sleep(400);
    }
  });
}

/** 右键菜单：消息流里的卡右键 4 次（每次 Esc 关）· tab 右键 2 次 · 会话头「更多」2 次。 */
async function menu(): Promise<Record<string, unknown>> {
  await toLongest();
  return measure("menu", async (steps) => {
    const cards = [...document.querySelectorAll<HTMLElement>(".stream.active .card")].filter((c) => c.getClientRects().length > 0);
    for (let k = 0; k < 4; k++) {
      const c = cards[cards.length - 1 - k];
      if (!c) break;
      await step(steps, "card", () => rightClickEl(c));
      await sleep(250);
      await step(steps, "esc", () => key("Escape"));
      await sleep(200);
    }
    for (let k = 0; k < 2; k++) {
      const t = document.querySelectorAll<HTMLElement>("#tab-bar .tab")[k + 1];
      if (t) await step(steps, "tab", () => rightClickEl(t));
      await sleep(250);
      await step(steps, "esc", () => key("Escape"));
      await sleep(200);
    }
    for (let k = 0; k < 2; k++) {
      const more = $("#session-head button:has([data-icon='more'])");
      if (more) await step(steps, "more", () => clickEl(more));
      await sleep(250);
      await step(steps, "esc", () => key("Escape"));
      await sleep(200);
    }
  });
}

/** 一屏终端画面（`k` 变出不同的行：每帧都真变）。 */
function screen(k: number): unknown {
  const lines = Array.from({ length: 40 }, (_, i) => {
    const text = `     transforming (${k * 40 + i}) packages/mod-${(k + i) % 23}/src/page.ts  ${"·".repeat((k + i) % 30)}`;
    return { text, spans: [{ from: 5, to: 17, fg: "bright-black" }, { from: 24, to: 52, fg: "blue" }] };
  });
  lines.push({ text: `✶ Building… (${k}s · ↑ ${k}.2k tokens · esc to interrupt)`, spans: [{ from: 0, to: 11, fg: "#d97757" }] });
  return { lines, screen: `0000000000${String(k).padStart(6, "0")}`, captured_at: Math.floor(Date.now() / 1000), captured_at_text: "10:42:05" };
}

/** 终端抽屉：最长那条的终端开在抽屉里，实时画面 30 帧 / 秒推 5 s，关抽屉。 */
async function drawer(): Promise<Record<string, unknown>> {
  await toLongest();
  const be = window.__perfBackend;
  if (!be) throw new Error("没有假后端句柄");
  const active = be.world.sessions.reduce((a, b) => (b.records.length > a.records.length ? b : a));
  return measure("drawer", async (steps) => {
    const btn = await waitSel("#session-head button[aria-label='看它的终端']");
    await step(steps, "open", () => clickEl(btn));
    await waitSel("#bottom-drawer pre", 10_000);
    await sleep(500);
    for (let k = 0; k < 150; k++) {
      const t = performance.now();
      await step(steps, "frame", () => be.pushScreen(active.origin, screen(k)));
      await sleep(Math.max(0, 33 - (performance.now() - t)));
    }
    await step(steps, "close", () => key("`", { ctrl: true, code: "Backquote" }));
    await sleep(300);
  });
}

/** agent / 任务抽屉：A 开 agent 抽屉、点开第一个（300 轮那个子运行）、T 切任务、关。 */
async function agents(): Promise<Record<string, unknown>> {
  await toLongest();
  return measure("agents", async (steps) => {
    const a = await waitSel(".status-agents");
    await step(steps, "agents", () => clickEl(a));
    const row = await waitSel("#bottom-drawer .agent-row");
    await sleep(300);
    await step(steps, "row", () => clickEl(row));
    await sleep(2500);
    const t = $(".status-tasks");
    if (t) await step(steps, "tasks", () => clickEl(t));
    await sleep(600);
    if (t) await step(steps, "close", () => clickEl(t));
    await sleep(300);
  });
}

// ─────────────────────────── 查看窗 / agent 窗口 ───────────────────────────

/** 查看窗里：往上滚 60 下 · 查找打字回车 · 回到底。 */
async function viewerScroll(): Promise<Record<string, unknown>> {
  const s = await waitSel(".session-viewer-stream");
  return measure("viewer-scroll", async (steps) => {
    for (let k = 0; k < 60; k++) {
      const t = performance.now();
      await step(steps, `${k}`, () => s.scrollBy(0, -600));
      await sleep(Math.max(0, 50 - (performance.now() - t)));
    }
    for (let k = 0; k < 30; k++) {
      const t = performance.now();
      await step(steps, `d${k}`, () => s.scrollBy(0, 1200));
      await sleep(Math.max(0, 50 - (performance.now() - t)));
    }
  });
}

async function viewerFind(): Promise<Record<string, unknown>> {
  return measure("viewer-find", async (steps) => {
    await step(steps, "ctrl-f", () => key("f", { ctrl: true }));
    const input = await waitSel<HTMLInputElement>('[data-role="find-input"]');
    const word = "游标分页";
    for (let i = 1; i <= word.length; i++) {
      await step(steps, `type${i}`, () => typeInto(input, word.slice(0, i)));
      await sleep(120);
    }
    await sleep(800);
    for (let k = 0; k < 10; k++) {
      await step(steps, "enter", () => keyOn(input, "Enter", { code: "Enter" }));
      await sleep(200);
    }
    await step(steps, "esc", () => keyOn(input, "Escape", { code: "Escape" }));
  });
}

/**
 * 慢 DOM 调用追踪（WebKit 没有性能面板）：`fn` 跑的期间把几类会逼样式 / 布局 / 改 DOM 的调用包一层，
 * 单次超过 `ms` 的记下来（哪个调用 · 多久 · 调用栈）。生产构建带 sourcemap（`CCM_SHOTS_SOURCEMAP=1`）时栈能对回源码。
 */
async function traceDom(fn: () => Promise<void>, ms = 15): Promise<Array<{ api: string; ms: number; stack: string }>> {
  const slow: Array<{ api: string; ms: number; stack: string }> = [];
  const undo: Array<() => void> = [];
  const wrapMethod = (proto: object, name: string): void => {
    const d = Object.getOwnPropertyDescriptor(proto, name);
    if (!d || typeof d.value !== "function") return;
    const orig = d.value as (...a: unknown[]) => unknown;
    Object.defineProperty(proto, name, {
      ...d,
      value: function (this: unknown, ...a: unknown[]) {
        const t0 = performance.now();
        try {
          return orig.apply(this, a);
        } finally {
          const dt = performance.now() - t0;
          if (dt > ms) slow.push({ api: name, ms: Math.round(dt), stack: (new Error().stack ?? "").split("\n").slice(1, 9).join(" | ") });
        }
      },
    });
    undo.push(() => Object.defineProperty(proto, name, d));
  };
  const wrapGetter = (proto: object, name: string): void => {
    const d = Object.getOwnPropertyDescriptor(proto, name);
    if (!d?.get) return;
    const g = d.get;
    Object.defineProperty(proto, name, {
      ...d,
      get: function (this: unknown) {
        const t0 = performance.now();
        try {
          return g.call(this);
        } finally {
          const dt = performance.now() - t0;
          if (dt > ms) slow.push({ api: name, ms: Math.round(dt), stack: (new Error().stack ?? "").split("\n").slice(1, 9).join(" | ") });
        }
      },
    });
    undo.push(() => Object.defineProperty(proto, name, d));
  };
  for (const n of ["getBoundingClientRect", "getClientRects", "scrollIntoView", "setAttribute", "removeAttribute", "replaceChildren", "remove", "append", "after", "before", "replaceWith", "toggleAttribute"]) wrapMethod(Element.prototype, n);
  for (const n of ["focus", "blur", "click"]) wrapMethod(HTMLElement.prototype, n);
  for (const n of ["appendChild", "insertBefore", "removeChild", "replaceChild"]) wrapMethod(Node.prototype, n);
  for (const n of ["offsetHeight", "offsetWidth", "offsetTop", "offsetParent"]) wrapGetter(HTMLElement.prototype, n);
  for (const n of ["clientHeight", "clientWidth", "scrollHeight", "scrollWidth", "scrollTop"]) wrapGetter(Element.prototype, n);
  const gcs = window.getComputedStyle;
  window.getComputedStyle = ((el: Element, p?: string | null) => {
    const t0 = performance.now();
    const r = gcs(el, p);
    const dt = performance.now() - t0;
    if (dt > ms) slow.push({ api: "getComputedStyle", ms: Math.round(dt), stack: (new Error().stack ?? "").split("\n").slice(1, 9).join(" | ") });
    return r;
  }) as typeof getComputedStyle;
  undo.push(() => (window.getComputedStyle = gcs));
  try {
    await fn();
  } finally {
    for (const u of undo.reverse()) u();
  }
  return slow;
}

/** 新建会话框 / 命令面板 / 账号面板各开关两次，记下慢的 DOM 调用。 */
async function slowDom(): Promise<Record<string, unknown>> {
  await toLongest();
  await window.__perf.quiet(500, 10_000);
  const out: Record<string, unknown> = {};
  const desc = (el: Element | null): string => {
    if (!el) return "null";
    const where = el.closest(".stream") ? `stream${el.closest(".stream.active") ? "(当前)" : "(后台)"}` : el.closest("#tab-bar") ? "tab-bar" : el.closest("#status-bar") ? "status-bar" : el.closest("[data-role=session-find-panel]") ? "find-panel" : el.parentElement?.closest("[id]")?.id ?? "?";
    return `${el.tagName.toLowerCase()}.${String((el as HTMLElement).className).split(" ")[0]}[${el.getAttribute("data-role") ?? el.getAttribute("aria-label") ?? ""}] in ${where} · 有几何 ${el.getClientRects().length > 0}`;
  };
  const timed = async (what: string, f: () => void | Promise<void>): Promise<void> => {
    (out[`${what} · 之前焦点`] ??= [] as unknown[]) as unknown[];
    (out[`${what} · 之前焦点`] as unknown[]).push(desc(document.activeElement));
    const t0 = performance.now();
    const slow = await traceDom(async () => {
      await f();
    });
    (out[what] ??= [] as unknown[]) as unknown[];
    (out[what] as unknown[]).push({ ms: Math.round(performance.now() - t0), slow });
  };
  for (let k = 0; k < 2; k++) {
    key("k", { ctrl: true });
    const input = await waitSel<HTMLInputElement>("[data-role=command-input]");
    typeInto(input, copyText("newSession.title.plain"));
    await sleep(200);
    await timed("新建会话框：回车开", () => keyOn(input, "Enter", { code: "Enter" }));
    await waitSel('[role="dialog"] button[aria-label="账号"]:not([data-value=""])');
    await sleep(400);
    await timed("新建会话框：Esc 关", () => key("Escape"));
    await sleep(400);
    await timed("命令面板：Ctrl+K 开", () => key("k", { ctrl: true }));
    const inp = await waitSel<HTMLInputElement>("[data-role=command-input]");
    await timed("命令面板：打字", () => typeInto(inp, copyText("acct.rot.title")));
    await sleep(200);
    await timed("命令面板：Esc 关", () => keyOn(inp, "Escape", { code: "Escape" }));
    await sleep(300);
    const chip = await waitSel(".status-account");
    await timed("账号面板：开", () => clickEl(chip));
    await sleep(600);
    await timed("账号面板：Esc 关", () => key("Escape"));
    await sleep(400);
  }
  return { name: "slow-dom", slowDom: out };
}

const ACTIONS: Record<string, () => Promise<Record<string, unknown>>> = {
  "slow-dom": slowDom,
  idle: () => idle(),
  "scroll-up": () => scroll("up"),
  "scroll-down": () => scroll("down"),
  jump,
  "stream-active": () => stream("active"),
  "stream-background": () => stream("background"),
  expand,
  find,
  outline,
  needs,
  palette,
  acct,
  "new-session": newSession,
  menu,
  drawer,
  agents,
  "viewer-idle": () => idle(),
  "viewer-scroll": viewerScroll,
  "viewer-find": viewerFind,
  // 浮层开关慢在哪：几样最小动作各自逼一次样式 ＋ 布局要多久（同步段）
  "overlay-probe": async () => {
    const desc = (el: Element | null): string => (el ? `${el.tagName.toLowerCase()}${el.id ? "#" + el.id : ""}.${String((el as HTMLElement).className).split(" ")[0]} in ${el.closest(".stream") ? "stream" : el.closest("#tab-bar") ? "tab-bar" : el.closest("body > *")?.id ?? "?"}` : "null");
    const focusedAtStart = desc(document.activeElement);
    const t = (f: () => void): number => {
      void document.body.offsetHeight;
      const t0 = performance.now();
      f();
      void document.body.offsetHeight;
      return performance.now() - t0;
    };
    const host = document.createElement("div");
    host.style.cssText = "position:fixed;left:0;top:0;width:10px;height:10px";
    document.body.appendChild(host);
    const input = document.createElement("input");
    host.appendChild(input);
    const out: Record<string, number[]> = {};
    const rec = (k: string, f: () => void): void => void (out[k] ??= []).push(t(f));
    for (let k = 0; k < 3; k++) {
      const d = document.createElement("div");
      rec("body 尾巴加一个 div", () => document.body.appendChild(d));
      rec("body 尾巴摘一个 div", () => d.remove());
      const e = document.createElement("div");
      rec("固定容器里加一个 div", () => host.appendChild(e));
      rec("固定容器里摘一个 div", () => e.remove());
      rec("根元素写自定义属性", () => document.documentElement.style.setProperty("--perf-probe", `${k}px`));
      rec("根元素删自定义属性", () => document.documentElement.style.removeProperty("--perf-probe"));
      rec("焦点进输入框", () => input.focus());
      rec("焦点回 body", () => input.blur());
      const tabEl = document.querySelector<HTMLElement>("#tab-bar .tab:not(.active)");
      if (tabEl) {
        rec("焦点到 tab 栏的一项", () => tabEl.focus());
        rec("焦点从 tab 栏回 body", () => tabEl.blur());
      }
      const sEl = document.querySelector<HTMLElement>(".stream.active");
      if (sEl) {
        rec("焦点到当前流", () => sEl.focus());
        rec("焦点从当前流回 body", () => sEl.blur());
      }
      rec("Ctrl+K 开命令面板", () => key("k", { ctrl: true }));
      await sleep(150);
      rec("Esc 关命令面板", () => key("Escape"));
      await sleep(150);
      rec("Ctrl+K 开命令面板（再）", () => key("k", { ctrl: true }));
      const inp = document.querySelector<HTMLInputElement>("[data-role=command-input]");
      if (inp) rec("命令面板里打字", () => typeInto(inp, copyText("newSession.title.plain")));
      await sleep(150);
      if (inp) rec("命令面板 ↓", () => keyOn(inp, "ArrowDown", { code: "ArrowDown" }));
      if (inp) rec("Esc 关命令面板（打过字）", () => keyOn(inp, "Escape", { code: "Escape" }));
      await sleep(150);
      rec("焦点进输入框（再）", () => input.focus());
      rec("body.focus()（焦点从输入框过来）", () => document.body.focus());
      rec("焦点进输入框（再再）", () => input.focus());
      rec("body.focus({preventScroll})", () => document.body.focus({ preventScroll: true }));
      rec("body.focus()（本来就在 body）", () => document.body.focus());
      rec("body 写一个 data 属性", () => (document.body.dataset.perfProbe = String(k)));
      rec("body 删 data 属性", () => delete document.body.dataset.perfProbe);
      await sleep(200);
    }
    host.remove();
    return { name: "overlay-probe", focusedAtStart, focusedAtEnd: desc(document.activeElement), probe: Object.fromEntries(Object.entries(out).map(([k, v]) => [k, v.map((x) => Math.round(x))])), dom: document.getElementsByTagName("*").length };
  },
  // 试一刀：账号面板开关时不往根元素上写自定义属性（看 WebKit 上那一下慢是不是它）
  "acct-noroot": async () => {
    const st = document.documentElement.style;
    const set = st.setProperty.bind(st);
    const rm = st.removeProperty.bind(st);
    st.setProperty = (k: string, v: string | null, p?: string) => (k === "--kit-drawer-right" ? undefined : set(k, v, p));
    st.removeProperty = (k: string) => (k === "--kit-drawer-right" ? "" : rm(k));
    try {
      return { ...(await acct()), name: "acct-noroot" };
    } finally {
      st.setProperty = set;
      st.removeProperty = rm;
    }
  },
};

window.__pa = {
  names: Object.keys(ACTIONS),
  async run(name: string): Promise<Record<string, unknown>> {
    const f = ACTIONS[name];
    if (!f) throw new Error(`没有这一项：${name}`);
    try {
      return await f();
    } finally {
      await closeOverlays();
    }
  },
};

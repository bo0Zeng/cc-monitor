/**
 * 场景动作的小工具：等元素、点、按键、右键。全在页里跑（产品代码看到的是真 DOM 事件）。
 */

export const sleep = (ms: number): Promise<void> => new Promise((r) => setTimeout(r, ms));

/** 等一个选择器出现（可见与否不论）；到时限 ⇒ 抛（这一张算没截成）。 */
export async function waitFor<T extends Element = HTMLElement>(sel: string, ms = 8000): Promise<T> {
  const until = performance.now() + ms;
  for (;;) {
    const el = document.querySelector<T>(sel);
    if (el) return el;
    if (performance.now() > until) throw new Error(`等不到 ${sel}`);
    await sleep(50);
  }
}

/** 等到「有 n 个」。 */
export async function waitCount(sel: string, n: number, ms = 8000): Promise<void> {
  const until = performance.now() + ms;
  while (document.querySelectorAll(sel).length < n) {
    if (performance.now() > until) throw new Error(`等不到 ${n} 个 ${sel}（现有 ${document.querySelectorAll(sel).length}）`);
    await sleep(50);
  }
}

export async function click(sel: string | Element): Promise<void> {
  const el = typeof sel === "string" ? await waitFor(sel) : sel;
  const r = el.getBoundingClientRect();
  const at = { bubbles: true, cancelable: true, clientX: r.left + r.width / 2, clientY: r.top + r.height / 2, button: 0 };
  el.dispatchEvent(new PointerEvent("pointerdown", at));
  el.dispatchEvent(new MouseEvent("mousedown", at));
  el.dispatchEvent(new PointerEvent("pointerup", at));
  el.dispatchEvent(new MouseEvent("mouseup", at));
  el.dispatchEvent(new MouseEvent("click", at));
  await sleep(120);
}

/** 按文字找一个可点的元素（按钮、菜单项）。 */
export async function byText(sel: string, text: string | RegExp, ms = 8000): Promise<HTMLElement> {
  const until = performance.now() + ms;
  for (;;) {
    for (const el of document.querySelectorAll<HTMLElement>(sel)) {
      const t = el.textContent ?? "";
      if (typeof text === "string" ? t.includes(text) : text.test(t)) return el;
    }
    if (performance.now() > until) throw new Error(`等不到 ${sel} 里写着 ${String(text)} 的那个`);
    await sleep(50);
  }
}

export async function rightClick(sel: string | Element): Promise<void> {
  const el = typeof sel === "string" ? await waitFor(sel) : sel;
  const r = el.getBoundingClientRect();
  el.dispatchEvent(
    new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: r.left + 20, clientY: r.top + r.height / 2, button: 2 }),
  );
  await sleep(200);
}

export async function hover(sel: string | Element): Promise<void> {
  const el = typeof sel === "string" ? await waitFor(sel) : sel;
  const r = el.getBoundingClientRect();
  const at = { bubbles: true, clientX: r.left + r.width / 2, clientY: r.top + r.height / 2 };
  el.dispatchEvent(new PointerEvent("pointerover", at));
  el.dispatchEvent(new MouseEvent("mouseover", at));
  el.dispatchEvent(new MouseEvent("mouseenter", { ...at, bubbles: false }));
  el.dispatchEvent(new MouseEvent("mousemove", at));
  await sleep(150);
}

declare global {
  interface Window {
    /** 截图工具（`cdp.mjs`）在每一页上装的绑定：请它从调试口按一个真键。 */
    __shotsKey?: (payload: string) => void;
    /** 截图工具按完回这一声。 */
    __shotsKeyDone?: (id: number, err: string | null) => void;
  }
}

const keyWaits = new Map<number, { done: () => void; fail: (e: Error) => void }>();
let keySeq = 0;

/**
 * 按一个键（`"k"` · `"Escape"` …），修饰键照给。**真按键**：经页上的绑定请截图工具从调试口发（`cdp.mjs` 的 `Page.key`），
 * 落在当前焦点上 —— 与人按的同一条路，`:focus-visible` 照亮、按钮上的 Enter 照点、没修饰的字照进框。
 * 不在截图工具开的页里（没有那条绑定）⇒ 抛，不退回合成事件（合成的证明不了焦点样式）。
 */
export async function key(k: string, mods: { ctrl?: boolean; shift?: boolean; alt?: boolean; meta?: boolean; code?: string } = {}): Promise<void> {
  const send = window.__shotsKey;
  if (!send) throw new Error("没有真按键通道（__shotsKey）：这一页不是截图工具开的");
  window.__shotsKeyDone ??= (id, err) => {
    const w = keyWaits.get(id);
    keyWaits.delete(id);
    if (err === null) w?.done();
    else w?.fail(new Error(err));
  };
  const id = ++keySeq;
  await new Promise<void>((done, fail) => {
    keyWaits.set(id, { done, fail });
    send(JSON.stringify({ id, key: k, ...mods }));
  });
  await sleep(150);
}

/** 往输入框里打字（逐个 input 事件，界面按真输入反应）。 */
export async function type(sel: string, text: string): Promise<void> {
  const el = await waitFor<HTMLInputElement | HTMLTextAreaElement>(sel);
  el.focus();
  el.value = text;
  el.dispatchEvent(new Event("input", { bubbles: true }));
  await sleep(200);
}

/** 主窗口画好了：tab 都在、批模式出来了（批末 300ms 宽限之后才切实时）。 */
export async function mainReady(tabs: number): Promise<void> {
  await waitCount("#tab-bar .tab", tabs, 15_000);
  await sleep(900);
}

/**
 * 等一个滚动容器的内容落定：连续 20 帧 `scrollHeight` 与 `scrollTop` 都没动（懒建卡 · 骨架换实高 · 按尾部贴底都做完了）。
 * 用它代替「睡一个固定的时长再截」：后端应答早晚几十毫秒，睡固定时长截到的是排版进行到哪一步（长会话那几张差一屏滚动就是这么来的）。
 */
export async function settled(el: HTMLElement, ms = 10_000): Promise<void> {
  const t0 = performance.now();
  let last = "";
  let still = 0;
  while (still < 20) {
    if (performance.now() - t0 > ms) throw new Error(`排版 ${ms}ms 没落定（scrollHeight 一直在变）`);
    await new Promise((r) => requestAnimationFrame(() => r(null)));
    const now = `${el.scrollHeight}:${el.scrollTop}`;
    still = now === last ? still + 1 : 0;
    last = now;
  }
}

/** 等 `get()` 给出东西（每帧问一次）。 */
export async function until<T>(get: () => T | null | undefined, ms = 10_000): Promise<T> {
  const t0 = performance.now();
  for (;;) {
    const v = get();
    if (v !== null && v !== undefined) return v;
    if (performance.now() - t0 > ms) throw new Error(`${ms}ms 没等到`);
    await new Promise((r) => requestAnimationFrame(() => r(null)));
  }
}

/** 消息流此刻那一个滚动容器。 */
export function streamScroller(): HTMLElement | null {
  const box = document.querySelector<HTMLElement>("#message-stream");
  return [...(box?.querySelectorAll<HTMLElement>("*") ?? [])].find((e) => e.scrollHeight > e.clientHeight + 100) ?? null;
}

/** 点第 i 个 tab（从 0 数）。 */
export async function openTab(i: number): Promise<void> {
  const tabs = document.querySelectorAll<HTMLElement>("#tab-bar .tab");
  const t = tabs[i];
  if (!t) throw new Error(`没有第 ${i} 个 tab`);
  await click(t);
  await sleep(500);
}

/** 消息流滚到某处：`"top"` · `"bottom"` · 某个选择器那张卡。 */
export async function scrollStream(to: "top" | "bottom" | string): Promise<void> {
  const box = await waitFor("#message-stream");
  const scroller = (box.querySelector(".tab-stream:not([hidden])") as HTMLElement | null) ?? box;
  const all = [scroller, ...scroller.querySelectorAll<HTMLElement>("*")].filter((e) => e.scrollHeight > e.clientHeight + 4 && getComputedStyle(e).overflowY !== "visible");
  const s = all[0] ?? scroller;
  if (to === "top") s.scrollTop = 0;
  else if (to === "bottom") s.scrollTop = s.scrollHeight;
  else {
    const el = await waitFor(to);
    el.scrollIntoView({ block: "start" });
  }
  await sleep(400);
}

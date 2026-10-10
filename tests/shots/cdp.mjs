/**
 * 截图工具用的 Chrome DevTools 协议小客户端：只够「开页 · 求值 · 截屏」，零依赖（Node 22 自带 WebSocket）。
 */

export class Cdp {
  /** @param {string} wsUrl 浏览器级的调试地址（DevToolsActivePort 里读出来的那一条）。 */
  static async connect(wsUrl) {
    const ws = new WebSocket(wsUrl);
    await new Promise((resolve, reject) => {
      ws.addEventListener("open", resolve, { once: true });
      ws.addEventListener("error", () => reject(new Error(`连不上浏览器调试口：${wsUrl}`)), { once: true });
    });
    return new Cdp(ws);
  }

  constructor(ws) {
    this.ws = ws;
    this.nextId = 1;
    this.pending = new Map();
    this.listeners = new Set();
    ws.addEventListener("message", (ev) => {
      const msg = JSON.parse(String(ev.data));
      if (msg.id !== undefined) {
        const p = this.pending.get(msg.id);
        if (!p) return;
        this.pending.delete(msg.id);
        if (msg.error) p.reject(new Error(`${p.method}: ${msg.error.message}`));
        else p.resolve(msg.result);
      } else {
        for (const fn of this.listeners) fn(msg);
      }
    });
  }

  /** 发一条命令；`sessionId` 给了 ⇒ 发到那一页（flatten 模式）。 */
  send(method, params = {}, sessionId = undefined) {
    const id = this.nextId++;
    const msg = { id, method, params };
    if (sessionId) msg.sessionId = sessionId;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject, method });
      this.ws.send(JSON.stringify(msg));
    });
  }

  on(fn) {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  }

  close() {
    this.ws.close();
  }
}

/** 一页：开一个新标签、挂上会话，给出截图要的几样动作。 */
export class Page {
  static async open(cdp, width, height, scale = 1) {
    const { targetId } = await cdp.send("Target.createTarget", { url: "about:blank" });
    const { sessionId } = await cdp.send("Target.attachToTarget", { targetId, flatten: true });
    const page = new Page(cdp, targetId, sessionId);
    await page.send("Page.enable");
    await page.send("Runtime.enable");
    // 场景里的 `key()`（`scenes/helpers.ts`）经这条绑定请这边发真按键；绑定跨导航留着。
    await page.send("Runtime.addBinding", { name: "__shotsKey" });
    await page.size(width, height, scale);
    return page;
  }

  constructor(cdp, targetId, sessionId) {
    this.cdp = cdp;
    this.targetId = targetId;
    this.sessionId = sessionId;
    this.consoleLines = [];
    cdp.on((msg) => {
      if (msg.sessionId !== this.sessionId) return;
      if (msg.method === "Runtime.consoleAPICalled") {
        const text = msg.params.args.map((a) => a.value ?? a.description ?? "").join(" ");
        this.consoleLines.push(`[${msg.params.type}] ${text}`);
      } else if (msg.method === "Runtime.bindingCalled" && msg.params.name === "__shotsKey") {
        void this.answerKey(msg.params.payload);
      } else if (msg.method === "Runtime.exceptionThrown") {
        const d = msg.params.exceptionDetails;
        this.consoleLines.push(`[exception] ${d.exception?.description ?? d.text}`);
      }
    });
  }

  send(method, params = {}) {
    return this.cdp.send(method, params, this.sessionId);
  }

  /**
   * 按一个真键（按下 ＋ 抬起，经调试口进渲染进程，与人按的同一条路）：`:focus-visible` 照亮、按钮上的 Enter 照点、字照进框。
   * `k` ＝ `KeyboardEvent.key` 的写法（`"Enter"` · `"ArrowUp"` · `"f"` · `"?"`）；`mods.code` 不给 ⇒ 按 `k` 推。
   */
  async key(k, mods = {}) {
    const { code, vk } = keyCode(k, mods.code);
    const modifiers = (mods.alt ? 1 : 0) | (mods.ctrl ? 2 : 0) | (mods.meta ? 4 : 0) | (mods.shift ? 8 : 0);
    const chord = mods.ctrl || mods.alt || mods.meta;
    const text = k === "Enter" ? "\r" : k.length === 1 && !chord ? k : undefined;
    const base = { key: k, code, windowsVirtualKeyCode: vk, nativeVirtualKeyCode: vk, modifiers };
    await this.send("Input.dispatchKeyEvent", { type: text === undefined ? "rawKeyDown" : "keyDown", ...base, ...(text === undefined ? {} : { text, unmodifiedText: text }) });
    await this.send("Input.dispatchKeyEvent", { type: "keyUp", ...base });
  }

  /** 页里 `__shotsKey(payload)` 请按的那一下：按完回一声（`__shotsKeyDone(id, 出错?)`）。 */
  async answerKey(payload) {
    let id = 0;
    let err = null;
    try {
      const req = JSON.parse(payload);
      id = req.id;
      await this.key(req.key, req);
    } catch (e) {
      err = e instanceof Error ? e.message : String(e);
    }
    await this.send("Runtime.evaluate", { expression: `window.__shotsKeyDone?.(${JSON.stringify(id)}, ${JSON.stringify(err)})` }).catch(() => {});
  }

  size(width, height, scale = 1) {
    return this.send("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor: scale, mobile: false });
  }

  async goto(url) {
    this.consoleLines = [];
    const loaded = new Promise((resolve) => {
      const off = this.cdp.on((msg) => {
        if (msg.sessionId === this.sessionId && msg.method === "Page.loadEventFired") {
          off();
          resolve();
        }
      });
    });
    await this.send("Page.navigate", { url });
    await loaded;
  }

  /** 页里求一个表达式的值（可以是 Promise）。 */
  async eval(expression) {
    const r = await this.send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
    if (r.exceptionDetails) {
      throw new Error(r.exceptionDetails.exception?.description ?? r.exceptionDetails.text);
    }
    return r.result.value;
  }

  /** 等页里那个表达式为真；到时限还没真 ⇒ 抛。 */
  async waitFor(expression, ms) {
    const until = Date.now() + ms;
    for (;;) {
      if (await this.eval(`Boolean(${expression})`)) return;
      if (Date.now() > until) throw new Error(`等了 ${ms}ms 还没等到：${expression}`);
      await sleep(100);
    }
  }

  async png() {
    const { data } = await this.send("Page.captureScreenshot", { format: "png", captureBeyondViewport: false });
    return Buffer.from(data, "base64");
  }

  close() {
    return this.cdp.send("Target.closeTarget", { targetId: this.targetId });
  }
}

export function sleep(ms) {
  return new Promise((resolve) => {
    const t = setTimeout(() => {
      clearTimeout(t);
      resolve();
    }, ms);
  });
}

/** 有名字的键 ⇒ `code` 与 Windows 虚拟键码（Chromium 认虚拟键码做默认动作：Tab 移焦点、Enter 点按钮、方向键滚动）。 */
const NAMED = {
  Enter: 13, Escape: 27, Tab: 9, Backspace: 8, Delete: 46, " ": 32,
  ArrowLeft: 37, ArrowUp: 38, ArrowRight: 39, ArrowDown: 40, Home: 36, End: 35, PageUp: 33, PageDown: 34,
};
/** 标点 ⇒ [code, 虚拟键码]（美式键位；按了 Shift 出的那个字也认）。 */
const PUNCT = {
  "]": ["BracketRight", 221], "[": ["BracketLeft", 219], "}": ["BracketRight", 221], "{": ["BracketLeft", 219],
  "`": ["Backquote", 192], "~": ["Backquote", 192], ",": ["Comma", 188], "<": ["Comma", 188], ".": ["Period", 190], ">": ["Period", 190],
  "/": ["Slash", 191], "?": ["Slash", 191], "-": ["Minus", 189], "_": ["Minus", 189], "=": ["Equal", 187], "+": ["Equal", 187],
  ";": ["Semicolon", 186], ":": ["Semicolon", 186], "'": ["Quote", 222], '"': ["Quote", 222], "\\": ["Backslash", 220], "|": ["Backslash", 220],
};

/** `key` 的写法 ⇒ `{code, vk}`；认不出 ⇒ 抛（场景写错了键名，宁可整张图报错也不发一个没人认的键）。 */
export function keyCode(k, code) {
  if (/^F([1-9]|1[0-2])$/.test(k)) return { code: code ?? k, vk: 111 + Number(k.slice(1)) };
  if (k in NAMED) return { code: code ?? (k === " " ? "Space" : k), vk: NAMED[k] };
  if (/^[a-zA-Z]$/.test(k)) return { code: code ?? `Key${k.toUpperCase()}`, vk: k.toUpperCase().charCodeAt(0) };
  if (/^[0-9]$/.test(k)) return { code: code ?? `Digit${k}`, vk: k.charCodeAt(0) };
  if (k in PUNCT) return { code: code ?? PUNCT[k][0], vk: PUNCT[k][1] };
  throw new Error(`截图工具认不出这个键：${JSON.stringify(k)}`);
}

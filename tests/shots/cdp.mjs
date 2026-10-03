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
  static async open(cdp, width, height) {
    const { targetId } = await cdp.send("Target.createTarget", { url: "about:blank" });
    const { sessionId } = await cdp.send("Target.attachToTarget", { targetId, flatten: true });
    const page = new Page(cdp, targetId, sessionId);
    await page.send("Page.enable");
    await page.send("Runtime.enable");
    await page.size(width, height);
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
      } else if (msg.method === "Runtime.exceptionThrown") {
        const d = msg.params.exceptionDetails;
        this.consoleLines.push(`[exception] ${d.exception?.description ?? d.text}`);
      }
    });
  }

  send(method, params = {}) {
    return this.cdp.send(method, params, this.sessionId);
  }

  size(width, height) {
    return this.send("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor: 1, mobile: false });
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

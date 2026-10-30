/**
 * 截图台架发到浏览器调试口的每一问都有上限（`cdp.mjs`）。
 *
 * 起因：整套跑到某一张时，开页 · 求值 · 截屏 · 关页里有一问浏览器一直不回话，台架就一直等，整趟挂住、问题表里一行没有。
 *
 * 人群：`Cdp.send`（每一问）与 `Page.goto`（等 load）。
 * 买不到：真浏览器为什么不回不在这里造（假调试口：收下命令、照表回或不回）。
 */
import { describe, expect, it } from "vitest";

const MOD = "./cdp.mjs";
type Listener = (ev: { data: string }) => void;
const { Cdp, Page } = (await import(MOD)) as {
  Cdp: new (ws: unknown) => { send: (m: string, p?: object, s?: string, ms?: number) => Promise<unknown> };
  Page: new (cdp: unknown, targetId: string, sessionId: string) => { goto: (url: string, ms?: number) => Promise<void> };
};

/** 假调试口：`answers[方法名]` 给了就照回（下一拍），没给就不回。 */
function fakeWs(answers: Record<string, object>) {
  const listeners: Listener[] = [];
  return {
    addEventListener: (_: string, fn: Listener) => listeners.push(fn),
    send(raw: string) {
      const msg = JSON.parse(raw) as { id: number; method: string };
      const r = answers[msg.method];
      if (r) queueMicrotask(() => listeners.forEach((fn) => fn({ data: JSON.stringify({ id: msg.id, result: r }) })));
    },
    close() {},
  };
}

describe("调试口每一问的上限", () => {
  it("回了话 ⇒ 照常交", async () => {
    const cdp = new Cdp(fakeWs({ "Runtime.evaluate": { result: { value: 7 } } }));
    await expect(cdp.send("Runtime.evaluate", {}, undefined, 200)).resolves.toEqual({ result: { value: 7 } });
  });
  it("不回话 ⇒ 到上限抛，说清是哪一问", async () => {
    const cdp = new Cdp(fakeWs({}));
    await expect(cdp.send("Page.captureScreenshot", {}, undefined, 50)).rejects.toThrow("Page.captureScreenshot");
  });
  it("开页一直不来 load ⇒ 到上限抛", async () => {
    const cdp = new Cdp(fakeWs({ "Page.navigate": {} }));
    const page = new Page(cdp, "t", "s");
    await expect(page.goto("http://127.0.0.1:1/x", 50)).rejects.toThrow("load");
  });
});

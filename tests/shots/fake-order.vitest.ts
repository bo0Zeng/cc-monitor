/**
 * 截图台架的假适配层交订阅那几样，与壳的真句柄同一个时刻算「成了」：`chan_subscribe` / `chan_want` / `chan_stop` / `watch_sessions`
 * 等无头壳那一侧收下了才回（产品里这几条 Tauri 命令是登记完才返回的，主界面 `await` 完订阅才发 `frontend-ready`）。
 * 从前假适配层当场回、另起一问交给台架：几问各走各的连接，就绪点可能抢在订阅前面到无头壳 ⇒ 那条订阅永远过不了就绪点、一格都不来。
 *
 * 人群：`fake/backend.ts` 的 `FakeBackend.invoke` 里交给无头壳的那几条。
 * 买不到：无头壳那一侧照收到的先后办（`src/frontend/shell/src/shots_shell.rs` 一行一条读），这里不起它。
 */
import { afterEach, describe, expect, it, vi } from "vitest";
import { FakeBackend } from "./fake/backend";
import { defaultWorld } from "./fake/world";

type Asked = { url: string; body: Record<string, unknown>; answer: () => void };

function rig(): { asked: Asked[]; backend: FakeBackend } {
  const asked: Asked[] = [];
  vi.stubGlobal(
    "fetch",
    (url: string, init: { body: string }) =>
      new Promise((resolve) => {
        const ok = () => resolve(new Response("{}", { status: 200 }));
        // 起世界那一问与开页前交的每台那一轮（`machine-round`）当场答；交给无头壳的那几问由用例决定什么时候答。
        const body = url === "/__ccm/world" ? {} : (JSON.parse(init.body) as Record<string, unknown>);
        if (url === "/__ccm/world") ok();
        else if (body.op === "machine-round") resolve(new Response('{"ok":true,"body":"null"}', { status: 200 }));
        else asked.push({ url, body, answer: ok });
      }),
  );
  vi.stubGlobal(
    "EventSource",
    class {
      onopen: (() => void) | null = null;
      onmessage: unknown = null;
      constructor() {
        setTimeout(() => this.onopen?.(), 0);
      }
    },
  );
  return { asked, backend: new FakeBackend(defaultWorld()) };
}

async function until(f: () => boolean): Promise<void> {
  while (!f()) await new Promise((r) => setTimeout(r, 0));
}

describe("假适配层交给无头壳的订阅：无头壳收下了才算成", () => {
  afterEach(() => vi.unstubAllGlobals());

  for (const [cmd, args, url] of [
    ["chan_subscribe", { id: 7, origin: "<local>", kind: "session-lines", want: 10 }, "/__ccm/sub"],
    ["watch_sessions", { sessions: [] }, "/__ccm/watch"],
    ["kept_sessions", { sessions: [] }, "/__ccm/kept"],
  ] as const) {
    it(`${cmd}：那一问答了之前不回`, async () => {
      const { asked, backend } = rig();
      let back = false;
      void Promise.resolve(backend.invoke(cmd, { ...args })).then(() => (back = true));
      await until(() => asked.some((a) => a.url === url));
      await new Promise((r) => setTimeout(r, 0));
      expect(back, `${cmd} 在无头壳收下之前就回了`).toBe(false);
      asked.find((a) => a.url === url)?.answer();
      await until(() => back);
    });
  }

  it("chan_want / chan_stop：那一问答了之前不回", async () => {
    const { asked, backend } = rig();
    const sub = Promise.resolve(backend.invoke("chan_subscribe", { id: 7, origin: "<local>", kind: "session-lines", want: 10 }));
    await until(() => asked.length === 1);
    asked[0]?.answer();
    await sub;
    for (const [cmd, url] of [
      ["chan_want", "/__ccm/want"],
      ["chan_stop", "/__ccm/stop"],
    ] as const) {
      let back = false;
      void Promise.resolve(backend.invoke(cmd, { id: 7, more: 5 })).then(() => (back = true));
      await until(() => asked.some((a) => a.url === url));
      await new Promise((r) => setTimeout(r, 0));
      expect(back, `${cmd} 在无头壳收下之前就回了`).toBe(false);
      asked.find((a) => a.url === url)?.answer();
      await until(() => back);
    }
  });
});

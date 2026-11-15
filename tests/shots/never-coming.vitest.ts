/**
 * 截图台架等「不会来」的东西当场判失败，不白等到兜底上限（以前每张没截成的白等五分钟，一整趟撞上开发调度的两小时上限）。
 *
 * 人群：`fake/backend.ts` 的 `FakeBackend.busy`（场景的等照它判「假后端手上还有没落定的事」，`scenes/helpers.ts::neverComing`）
 *   与 `FakeBackend.settled`（界面订了世界里没起后端的那一台 ⇒ 清单不会来）。
 * 买不到：页面「多久没变」那一半（要真页里的 DOM，`run.mjs --only <场景>` 看）。
 */
import { afterEach, describe, expect, it, vi } from "vitest";
import { FakeBackend } from "./fake/backend";
import { defaultWorld } from "./fake/world";
import type { World } from "./fake/types";

type Asked = { url: string; answer: () => void };

function rig(world: World): { asked: Asked[]; backend: FakeBackend } {
  const asked: Asked[] = [];
  vi.stubGlobal(
    "fetch",
    (url: string) =>
      new Promise((resolve) => {
        const ok = () => resolve(new Response("{}", { status: 200 }));
        if (url === "/__ccm/world") ok();
        else asked.push({ url, answer: ok });
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
  return { asked, backend: new FakeBackend(world) };
}

async function until(f: () => boolean): Promise<void> {
  while (!f()) await new Promise((r) => setTimeout(r, 0));
}

const LOCAL = "<local>";

describe("台架等不会来的东西当场判失败", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("界面订了世界里没起后端的那一台：settled 当场抛、点名那一台（不等到上限）", async () => {
    const w = defaultWorld();
    const { asked, backend } = rig({ ...w, machines: [LOCAL] });
    const sub = Promise.resolve(backend.invoke("chan_subscribe", { id: 9, origin: "devbox", kind: "session-lines", want: 10 }));
    await until(() => asked.length === 1);
    asked[0]?.answer();
    await sub;
    const t0 = performance.now();
    await expect(backend.settled()).rejects.toThrow(/世界里没起那台后端[^]*devbox/);
    expect(performance.now() - t0).toBeLessThan(5_000);
    // 那一台不会来 ⇒ 不算「手上还有没落定的事」（不然场景的等永远判不出「不会来了」）。
    expect(backend.busy()).toBeNull();
  });

  it("问在途 ⇒ busy 说在途；答了 ⇒ 落定", async () => {
    const { asked, backend } = rig(defaultWorld());
    const sub = Promise.resolve(backend.invoke("watch_sessions", { sessions: [] }));
    await until(() => asked.length === 1);
    expect(backend.busy()).toMatch(/在途/);
    asked[0]?.answer();
    await sub;
    expect(backend.busy()).toBeNull();
  });

  it("场景的等：假后端落定 ＋ 页面静过 QUIET_MS ⇒ 当场抛「不会来了」；假后端有事 · 页面在变 ⇒ 照旧等", async () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "performance"] });
    try {
      vi.resetModules();
      const { waitFor, neverComing, QUIET_MS } = await import("./scenes/helpers");
      let busy: string | null = "1 问在途";
      window.__shots = { state: "booting", finished: Promise.resolve(), error: null, unhandled: [], layout: [], busy: () => busy };
      const start = performance.now();
      vi.advanceTimersByTime(QUIET_MS + 1);
      expect(neverComing(start), "假后端还有事就不判").toBeNull();
      // 一问刚答完：从答完那一刻重新数（机器忙时真后端算了很久、页面还没来得及画，不误判）。
      busy = null;
      expect(neverComing(start), "刚落定就判了").toBeNull();
      vi.advanceTimersByTime(QUIET_MS + 1);
      expect(neverComing(start)).toMatch(/不会来了/);
      // 这一次等刚开始 ⇒ 从等开始那一刻数（前一段静不算进来）。
      expect(neverComing(performance.now()), "刚开始等就判了").toBeNull();
      // 页面一变 ⇒ 重新数静了多久。
      document.body.append(document.createElement("div"));
      await Promise.resolve();
      expect(neverComing(start)).toBeNull();
      const t0 = performance.now();
      const gave = waitFor("#never-there").catch((e: Error) => e.message);
      await vi.advanceTimersByTimeAsync(QUIET_MS + 200);
      expect(await gave).toMatch(/等不到 #never-there（不会来了/);
      expect(performance.now() - t0).toBeLessThan(QUIET_MS + 1_000);
    } finally {
      delete window.__shots;
      vi.useRealTimers();
    }
  });

  it("订了、世界里起了的那一台还没报完清单 ⇒ busy 点名那一台（照旧等）", async () => {
    const { asked, backend } = rig(defaultWorld());
    const sub = Promise.resolve(backend.invoke("chan_subscribe", { id: 7, origin: LOCAL, kind: "session-lines", want: 10 }));
    await until(() => asked.length === 1);
    asked[0]?.answer();
    await sub;
    expect(backend.busy()).toMatch(/还没报完清单/);
  });
});

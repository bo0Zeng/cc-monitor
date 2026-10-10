/**
 * 壳推来的出错（`UiErrorPayload`）怎么上屏：标题就是壳照文案键写好的那一句，码 · 键 · 原话一个字都不上屏，
 * 原话只在［复制详情］里；那台连不上 ⇒［重新连接］在［看日志］前面。
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { copyText } from "../../../src/frontend/ui/copy-table";
import type { UiErrorPayload } from "../../../src/frontend/ui/generated/UiErrorPayload";

type Mod = typeof import("../../../src/frontend/ui/backend-errors");
let mod: Mod;

const toasts = (): HTMLElement[] => [...(document.getElementById("kit-toast-stack")?.children ?? [])] as HTMLElement[];

const payload = (over: Partial<UiErrorPayload> = {}): UiErrorPayload => ({
  code: "local-exited",
  key: "rsUiError.localExited.down",
  args: {},
  said: "夹具那一句 · 本机连不上",
  detail: "时刻：夹具\n原话：[夹具账] origin=<local> 判定=夹具",
  reconnect: "<local>",
  at: 0,
  ...over,
});

describe("壳推来的出错 toast", () => {
  beforeEach(async () => {
    vi.useFakeTimers();
    document.body.innerHTML = "";
    vi.resetModules();
    mod = await import("../../../src/frontend/ui/backend-errors");
  });
  afterEach(() => {
    vi.runOnlyPendingTimers();
    vi.useRealTimers();
  });

  it("标题是那一句；码 · 键 · 原话都不在提示条上；有详情 ⇒［复制详情］", () => {
    const p = payload();
    mod.showUiError(p);
    const [t] = toasts();
    expect(t, "一条都没出").toBeDefined();
    const text = t.textContent ?? "";
    expect(text).toContain("夹具那一句");
    for (const leak of [p.code, p.key, "origin=", "[夹具账]"]) expect(text, `提示条上露了「${leak}」`).not.toContain(leak);
    const labels = [...t.querySelectorAll("button")].map((b) => b.textContent ?? "");
    expect(labels).toContain(copyText("detail.act.copy"));
  });

  it("连不上（reconnect 有值）⇒［重新连接］在［看日志］前；没有 ⇒ 只［看日志］", () => {
    const reconnect = copyText("errorToast.showErrorToast.reconnect");
    const log = copyText("errorToast.showErrorToast.openLog");
    expect(mod.uiErrorActions(payload()).map((a) => a.label)).toEqual([reconnect, log]);
    expect(mod.uiErrorActions(payload({ reconnect: undefined })).map((a) => a.label)).toEqual([log]);
  });
});

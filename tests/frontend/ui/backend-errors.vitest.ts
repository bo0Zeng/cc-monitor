/**
 * 壳推来的出错（`UiErrorPayload`）怎么上屏：标题就是壳照文案键写好的那一句，码 · 键 · 原话一个字都不上屏，
 * 原话只在［复制详情］里；那台断开 ⇒［重新连接］在［看日志］前面。
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { copyText } from "../../../src/frontend/ui/copy-table";
import type { UiErrorPayload } from "../../../src/frontend/ui/generated/UiErrorPayload";
import { UI_ERROR } from "../../../src/frontend/ui/generated/shell-names";

type Mod = typeof import("../../../src/frontend/ui/backend-errors");
let mod: Mod;

const toasts = (): HTMLElement[] => [...(document.getElementById("kit-toast-stack")?.children ?? [])] as HTMLElement[];

const payload = (over: Partial<UiErrorPayload> = {}): UiErrorPayload => ({
  code: "local-exited",
  key: "rsUiError.localExited.down",
  args: {},
  said: "夹具那一句 · 本机断开",
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

describe("界面只有一个出错 listener", () => {
  it("生产代码里订的壳推出错事件（生成物的 `UI_ERROR`）只有一处；`remote-health` 零处（带正控）", async () => {
    const { productionTsFiles } = await import("../../test-support/production-sources.ts");
    const { stripComments } = await import("../../test-support/strip-comments.ts");
    const files = productionTsFiles("src/frontend/ui");
    expect(files.length, "一份界面生产代码都没读到").toBeGreaterThan(20);
    // `first` 是 `listen(` 的第一个实参原样：带引号的事件名，或生成物里那个常量名。
    const listened = (first: string): string[] =>
      files.filter((f) => new RegExp(`\\blisten(<[^>]*>)?\\(\\s*${first}\\b`).test(stripComments(f.text, "ts"))).map((f) => f.file);
    expect(listened(`"remote-health"`), "界面还在订 `remote-health`").toEqual([]);
    expect(listened("UI_ERROR"), "订壳推出错事件的不是恰好 backend-errors.ts 一处").toEqual(["src/frontend/ui/backend-errors.ts"]);
    expect(listened(`"${UI_ERROR}"`), "有一处绕过生成物、手写事件名在订出错事件").toEqual([]);
    // 正控：同一个认法认得出带类型参数的那一形。
    expect(new RegExp(`\\blisten(<[^>]*>)?\\(\\s*UI_ERROR\\b`).test(`listen<P>(UI_ERROR, f)`)).toBe(true);
  });
});

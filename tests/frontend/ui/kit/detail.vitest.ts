/**
 * 复制详情（条带 §5.1）：空串不出按钮 · 复制的是「屏上那句 ＋ 详情」· 已复制 1.5s 回默认、连点重计 · 写不进就地展开全选 · 合流 ×N 段间空一行 · 取详情只认带 `detail` 的错。
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { copyText } from "../../../../src/frontend/ui/copy-table";
import { copyDetailButton, detailOf, detailBody, COPIED_MS } from "../../../../src/frontend/ui/kit/detail";

const label = (b: HTMLElement): string => b.querySelector("button")?.textContent ?? "";

describe("复制详情按钮", () => {
  let written: string[];
  beforeEach(() => {
    vi.useFakeTimers();
    document.body.innerHTML = "";
    written = [];
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText: (t: string) => (written.push(t), Promise.resolve()) },
    });
  });
  afterEach(() => vi.useRealTimers());

  it("详情是空的 ⇒ 不出按钮", () => {
    expect(copyDetailButton("端口限 1–65535", "")).toBeNull();
    expect(copyDetailButton("x", "   \n")).toBeNull();
  });

  it("复制出去的首行是屏上那句，下面原样是详情；已复制 1.5s 回默认、连点重计", async () => {
    const el = copyDetailButton("结束 orders 失败", "码：kill_failed\n原话：boom")!;
    document.body.appendChild(el);
    expect(label(el)).toBe(copyText("detail.act.copy"));
    el.querySelector("button")!.click();
    await vi.advanceTimersByTimeAsync(0);
    expect(written).toEqual(["结束 orders 失败\n码：kill_failed\n原话：boom"]);
    expect(label(el)).toBe(copyText("detail.act.copied"));
    expect(el.textContent).toContain(copyText("detail.aria.copied"));
    await vi.advanceTimersByTimeAsync(COPIED_MS - 200);
    el.querySelector("button")!.click();
    await vi.advanceTimersByTimeAsync(COPIED_MS - 100);
    expect(label(el)).toBe(copyText("detail.act.copied"));
    await vi.advanceTimersByTimeAsync(200);
    expect(label(el)).toBe(copyText("detail.act.copy"));
  });

  it("剪贴板写不进 ⇒ 复制失败 ＋ 就地展开原文全选，Esc 收起", async () => {
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText: () => Promise.reject(new Error("denied")) },
    });
    const host = document.createElement("div");
    const el = copyDetailButton("读取失败", "码：x")!;
    host.appendChild(el);
    document.body.appendChild(host);
    el.querySelector("button")!.click();
    await vi.advanceTimersByTimeAsync(0);
    expect(label(el)).toBe(copyText("detail.act.failed"));
    const box = host.querySelector("textarea")!;
    expect(box.value).toBe("读取失败\n码：x");
    expect(box.readOnly).toBe(true);
    expect(document.activeElement).toBe(box);
    expect(host.textContent).toContain(copyText("detail.fallback.hint", { copyKey: "Ctrl+C" }));
    box.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    expect(host.querySelector("textarea")).toBeNull();
  });

  it("窄 ⇒ 只剩图标，读屏名仍是「复制详情」", () => {
    const el = copyDetailButton("x", "码：y", { iconOnly: true })!;
    const b = el.querySelector("button")!;
    expect(b.getAttribute("aria-label")).toBe(copyText("detail.act.copy"));
    expect(b.textContent).toBe("");
  });

  it("合流 ×N：每段「那句 ＋ 详情」，段间空一行，首行带 ×N", () => {
    expect(detailBody([["读取 cc-bus 无应答", "码：a"]])).toBe("读取 cc-bus 无应答\n码：a");
    expect(
      detailBody([
        ["读取 cc-bus 无应答", "码：a"],
        ["读取 cc-bus 无应答", "码：b"],
      ]),
    ).toBe(`读取 cc-bus 无应答 ${copyText("kit.toast.count", { n: 2 })}\n\n码：a\n\n码：b`);
  });

  it("取详情：带字符串 detail 的错才有，别的一律空", () => {
    const e = Object.assign(new Error("s"), { detail: "码：x" });
    expect(detailOf(e)).toBe("码：x");
    expect(detailOf(new Error("s"))).toBe("");
    expect(detailOf("s")).toBe("");
    expect(detailOf({ detail: "码：x" })).toBe("");
  });
});

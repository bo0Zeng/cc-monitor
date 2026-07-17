// F88a（#52）用量视图的 jsdom 测试：open→扫描（aggregate_usage_all Channel 流式）→渲染 pivot 表 +
// 合计 + 硬边界标注；切维度按钮 active + 重渲；空结果提示。

import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  Channel: class {
    onmessage: ((v: unknown) => void) | null = null;
  },
}));
vi.mock("../keybindings/registry", () => ({
  dispatcher: { pushOverlay: vi.fn(), popOverlay: vi.fn() },
}));
vi.mock("../error-toast", () => ({ showActionFailureToast: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import { UsageView } from "./usage-view";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;

function setupRows(rows: unknown[]): void {
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string, args: { onRow?: { onmessage?: (v: unknown) => void } }) => {
    if (cmd === "aggregate_usage_all") {
      for (const r of rows) args.onRow?.onmessage?.(r);
      return Promise.resolve(rows.length);
    }
    return Promise.resolve(undefined);
  });
}

const row = (over: Record<string, unknown> = {}) => ({
  sessionId: "s1",
  projectPath: "/a",
  projectName: "A",
  buckets: [
    {
      model: "claude-opus-4-8",
      day: "2026-07-17",
      totals: { input: 100, cacheCreation: 10, cacheRead: 500, output: 20, msgs: 1 },
    },
  ],
  ...over,
});

describe("UsageView (F88a #52)", () => {
  beforeEach(() => {
    document.body.replaceChildren();
  });

  it("open → 扫描 → 渲染 pivot 表 + 合计行 + 硬边界标注", async () => {
    setupRows([row()]);
    const view = new UsageView();
    await view.open();
    expect(document.querySelector(".usage-table")).toBeTruthy();
    // 默认按天 → 出现 2026-07-17
    expect(document.body.textContent).toContain("2026-07-17");
    expect(document.querySelector(".usage-total-row")).toBeTruthy();
    // 硬边界：已花费≠配额
    expect(document.querySelector(".usage-note")?.textContent).toContain("配额");
    // 合计 token = 100+10+500+20 = 630（toLocaleString 带千分位）
    expect(document.body.textContent).toContain("630");
  });

  it("空结果 → 提示无用量", async () => {
    setupRows([]);
    const view = new UsageView();
    await view.open();
    expect(document.querySelector(".usage-status")?.textContent).toContain("尚无用量");
    expect(document.querySelector(".usage-table")).toBeNull();
  });

  it("切「按模型」维度 → active 态 + 重渲出 model", async () => {
    setupRows([row()]);
    const view = new UsageView();
    await view.open();
    const modelBtn = [...document.querySelectorAll<HTMLButtonElement>(".usage-dim-btn")].find(
      (b) => b.dataset.dim === "model",
    )!;
    modelBtn.click();
    expect(modelBtn.classList.contains("active")).toBe(true);
    expect(document.body.textContent).toContain("claude-opus-4-8");
  });
});

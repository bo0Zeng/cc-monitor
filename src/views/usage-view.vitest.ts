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

/** 表头文本（含 ▼/▲ 指示符）。 */
const headText = (): string[] =>
  [...document.querySelectorAll<HTMLElement>(".usage-table th")].map((th) => th.textContent ?? "");
/** 数据行首列（维度键）文本，按渲染顺序；不含表头与合计行。 */
const keyCol = (): string[] =>
  [...document.querySelectorAll<HTMLElement>(".usage-table tr")]
    .filter((tr) => !tr.classList.contains("usage-total-row") && !tr.querySelector("th"))
    .map((tr) => tr.querySelector("td")?.textContent ?? "");
const clickTh = (prefix: string): void => {
  [...document.querySelectorAll<HTMLElement>(".usage-table th")]
    .find((t) => (t.textContent ?? "").startsWith(prefix))!
    .click();
};
const clickDim = (dim: string): void => {
  [...document.querySelectorAll<HTMLButtonElement>(".usage-dim-btn")]
    .find((b) => b.dataset.dim === dim)!
    .click();
};
const bucket = (day: string, input: number, output: number, msgs: number) => ({
  model: "m",
  day,
  totals: { input, cacheCreation: 0, cacheRead: 0, output, msgs },
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

// #67：表头排序的 DOM 状态机。纯函数测(usage-pivot.test.ts)钉不住"视图有没有真接上"——
// 若 addEventListener 被删、或 ▼ 又被写死在等效∑上，纯函数测仍全绿。这里从 DOM 侧钉死。
describe("#67 表头排序（DOM 状态机）", () => {
  beforeEach(() => {
    document.body.replaceChildren();
  });
  // token 多的是**较早**的 07-16 → 等效∑序与日期序相反，可区分"按天到底按哪个排"。
  const twoDays = [
    row({ sessionId: "big", buckets: [bucket("2026-07-16", 1000, 100, 9)] }),
    row({ sessionId: "small", buckets: [bucket("2026-07-18", 5, 1, 1)] }),
  ];

  it("默认「按天」→ 日期降序(最近在上) + 指示符只挂当前列", async () => {
    setupRows(twoDays);
    await new UsageView().open();
    expect(headText()[0]).toContain("按天");
    expect(headText()[0]).toContain("▼");
    // ★区分：等效∑序会把 token 多的 07-16 排前；按天默认必须最近的 07-18 在上
    expect(keyCol()).toEqual(["2026-07-18", "2026-07-16"]);
    expect(headText().filter((t) => t.includes("▼") || t.includes("▲")).length).toBe(1);
  });

  it("点「合计」→ ▼ 移到合计、按天列无箭头，行序按合计降序", async () => {
    setupRows(twoDays);
    await new UsageView().open();
    clickTh("合计");
    expect(headText()[0]).not.toContain("▼");
    expect(headText().find((t) => t.startsWith("合计"))).toContain("▼");
    expect(keyCol()).toEqual(["2026-07-16", "2026-07-18"]);
  });

  it("同列再点 → ▲ 且行序反转", async () => {
    setupRows(twoDays);
    await new UsageView().open();
    clickTh("合计");
    clickTh("合计");
    expect(headText().find((t) => t.startsWith("合计"))).toContain("▲");
    expect(keyCol()).toEqual(["2026-07-18", "2026-07-16"]);
  });

  it("切维度 → 排序重置为该维度默认（等效∑ ▼）", async () => {
    setupRows(twoDays);
    await new UsageView().open();
    clickTh("回复"); // 先切到别的列
    clickDim("project");
    expect(headText().find((t) => t.startsWith("等效∑"))).toContain("▼");
  });

  it("点**已激活**的维度按钮 → 不抹掉用户排序", async () => {
    setupRows(twoDays);
    await new UsageView().open();
    clickTh("回复");
    clickDim("day"); // 当前已是 day
    expect(headText().find((t) => t.startsWith("回复"))).toContain("▼");
  });

  it("按项目点首列 → 按**显示的名字**排，不是按隐藏的全路径", async () => {
    setupRows([
      row({
        sessionId: "s1",
        projectPath: "/a/zebra",
        projectName: "Zebra",
        buckets: [bucket("2026-07-16", 10, 1, 1)],
      }),
      row({
        sessionId: "s2",
        projectPath: "/b/alpha",
        projectName: "Alpha",
        buckets: [bucket("2026-07-16", 20, 1, 1)],
      }),
    ]);
    await new UsageView().open();
    clickDim("project");
    clickTh("按项目"); // → desc
    clickTh("按项目"); // → asc
    // ★区分：按隐藏 key(全路径 /a/zebra < /b/alpha)会得到 Zebra 在前；按 label 才是 Alpha 在前
    expect(keyCol()).toEqual(["Alpha", "Zebra"]);
  });
});

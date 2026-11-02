/**
 * 多选计数只一种写法（A2-17-7-4）：菜单一律「动作（n）」，确认框「结束会话（n）」；`×{n}` 不出现在多选那几条里。
 */
import { describe, it, expect } from "vitest";
import table from "../../../src/shared/copy/table.json";
import { copyText } from "../../../src/frontend/ui/copy-table";

const entries = (table as { entries: Record<string, { zh?: string }> }).entries;

describe("多选计数写法", () => {
  it("菜单项与结束确认框（tabBatch.menu.* · tabBatch.stop.*）没有一条写 ×{n}；结局里「失败 ×n」照文案规矩 C-W11 留着", () => {
    const crossed = Object.entries(entries)
      .filter(([k, v]) => (k.startsWith("tabBatch.menu.") || k.startsWith("tabBatch.stop.")) && (v.zh ?? "").includes("×"))
      .map(([k]) => k);
    expect(crossed).toEqual([]);
  });

  it("结束确认框：标题「结束会话（n）」、按钮「结束（n）」（文案规矩不许问号与量词）", () => {
    expect(copyText("tabBatch.menu.stop", { n: 3 })).toBe("结束会话（3）");
    expect(copyText("tabBatch.stop.action", { n: 3 })).toBe("结束（3）");
  });
});

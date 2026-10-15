/**
 * 「没人答时自动答 ▸」那一组（`tab-menu.ts::autoAnswerItems`）：勾在会话事实那一格上、点一档交那一档（`0` ＝ 关），点着勾的那一档不交。
 */
import { describe, it, expect, vi } from "vitest";
import { autoAnswerItems, AUTO_ANSWER_MINUTES } from "../../../src/frontend/ui/tab-menu";
import { copyText } from "../../../src/frontend/ui/copy-table";
import { fmtDur } from "../../../src/frontend/ui/duration-format";

describe("没人答时自动答 ▸", () => {
  it("关着 ⇒ 勾在「关」；几档照短时长写；点一档交那一档", () => {
    const set = vi.fn<(min: number) => void>();
    const items = autoAnswerItems({ autoAnswer: null }, set);
    expect(items.map((i) => [i.label, i.checked, i.radio])).toEqual(
      AUTO_ANSWER_MINUTES.map((m) => [m === 0 ? copyText("tabMenu.autoAnswer.off") : fmtDur(m * 60), m === 0, "autoAnswer"]),
    );
    items[0].onClick!();
    expect(set, "点着勾的那一档不交").not.toHaveBeenCalled();
    items[2].onClick!();
    expect(set).toHaveBeenCalledWith(AUTO_ANSWER_MINUTES[2]);
  });

  it("那台设的不在这几档里（命令行写的）⇒ 多列一项、勾在它上，点「关」交 0", () => {
    const set = vi.fn<(min: number) => void>();
    const items = autoAnswerItems({ autoAnswer: 7 }, set);
    const on = items.filter((i) => i.checked).map((i) => i.label);
    expect(on).toEqual([fmtDur(7 * 60)]);
    expect(items.length).toBe(AUTO_ANSWER_MINUTES.length + 1);
    items[0].onClick!();
    expect(set).toHaveBeenCalledWith(0);
  });
});

/**
 * 「没人答时自动答 ▸」那一组（`tab-menu.ts::autoAnswerItems`）：照会话事实 `autoAnswerChoices` 列档、字照抄，勾在 `min == autoAnswer` 那一项上；
 * 点一档交那一档（`0` ＝ 关），点着勾的那一档不交。几档与字都是核心给的（`control::asks::auto_choices`），界面不自带表。
 */
import { describe, it, expect, vi } from "vitest";
import { autoAnswerItems } from "../../../src/frontend/ui/tab-menu";

const choice = (min: number) => ({ min, text: `核心·${min}` });

describe("没人答时自动答 ▸", () => {
  it("关着 ⇒ 勾在 0 那一档；几档与字照事实列；点一档交那一档", () => {
    const set = vi.fn<(min: number) => void>();
    const autoAnswerChoices = [0, 5, 15, 30].map(choice);
    const items = autoAnswerItems({ autoAnswer: null, autoAnswerChoices }, set);
    expect(items.map((i) => [i.label, i.checked, i.radio])).toEqual(autoAnswerChoices.map((c) => [c.text, c.min === 0, "autoAnswer"]));
    items[0].onClick!();
    expect(set, "点着勾的那一档不交").not.toHaveBeenCalled();
    items[2].onClick!();
    expect(set).toHaveBeenCalledWith(15);
  });

  it("事实多列了此刻那一档（命令行写的 7 分钟）⇒ 照列、勾在它上，点 0 那一档交 0", () => {
    const set = vi.fn<(min: number) => void>();
    const autoAnswerChoices = [0, 5, 7, 15, 30].map(choice);
    const items = autoAnswerItems({ autoAnswer: 7, autoAnswerChoices }, set);
    expect(items.map((i) => i.label)).toEqual(autoAnswerChoices.map((c) => c.text));
    expect(items.filter((i) => i.checked).map((i) => i.label)).toEqual(["核心·7"]);
    items[0].onClick!();
    expect(set).toHaveBeenCalledWith(0);
  });

  it("界面不自带几档：事实没给 ⇒ 一项都不列", () => {
    expect(autoAnswerItems({ autoAnswer: 7, autoAnswerChoices: [] }, vi.fn())).toEqual([]);
  });
});

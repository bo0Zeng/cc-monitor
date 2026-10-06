/**
 * `src/frontend/ui/views/history-time.ts` 的判据：历史页的分段与行尾时间（设计稿「文件与历史」乙4-③ · 审稿 F8）。
 *
 * 1. 分段：今天 · 昨天 · 本周（周一起，周日算上一周的末尾）· 再往前按月（今年 `9 月`、往年 `2025 年 9 月`）。
 * 2. 行尾：今天 `HH:MM` · 昨天 `MM-DD HH:MM` · 更早 `MM-DD` · 往年 `YYYY-MM-DD`。
 * 3. 内容头那一段：同一天只写一次日期（今天连日期都不写）。
 */
import { describe, it, expect } from "vitest";
import { rowTime, sectionKey, sectionLabel, spanText } from "../../../../src/frontend/ui/views/history-time";

// 本地日历：2026-10-07 是周三。
const at = (y: number, m: number, d: number, h = 12, mi = 0): number => new Date(y, m - 1, d, h, mi).getTime();
const NOW = at(2026, 10, 7, 15, 30);

describe("分段", () => {
  it.each([
    [at(2026, 10, 7, 0, 1), "d0", "今天"],
    [at(2026, 10, 6, 23, 59), "d1", "昨天"],
    [at(2026, 10, 5, 8), "w0", "本周"],
    [at(2026, 10, 4, 22), "m2026-10", "10 月"],
    [at(2026, 9, 30), "m2026-09", "9 月"],
    [at(2025, 12, 31), "m2025-12", "2025 年 12 月"],
  ])("%s ⇒ %s", (t, key, label) => {
    expect(sectionKey(t, NOW)).toBe(key);
    expect(sectionLabel(key, NOW)).toBe(label);
  });
  it("周一那天：昨天是周日 ⇒ 本周那一段是空的（昨天之前直接按月）", () => {
    const mon = at(2026, 10, 5, 9);
    expect(sectionKey(at(2026, 10, 4, 9), mon)).toBe("d1");
    expect(sectionKey(at(2026, 10, 3, 9), mon)).toBe("m2026-10");
  });
});

describe("行尾时间", () => {
  it.each([
    [at(2026, 10, 7, 14, 2), "14:02"],
    [at(2026, 10, 6, 22, 10), "10-06 22:10"],
    [at(2026, 9, 30, 3), "09-30"],
    [at(2025, 9, 30, 3), "2025-09-30"],
  ])("%s ⇒ %s", (t, want) => {
    expect(rowTime(t, NOW)).toBe(want);
  });
});

describe("内容头那一段", () => {
  it("今天 · 同一天 · 跨天", () => {
    expect(spanText(at(2026, 10, 7, 2, 1), at(2026, 10, 7, 14, 2), NOW)).toBe("02:01–14:02");
    expect(spanText(at(2026, 9, 30, 2, 1), at(2026, 9, 30, 14, 2), NOW)).toBe("09-30 02:01–14:02");
    expect(spanText(at(2026, 9, 30, 3), at(2026, 10, 1, 4, 57), NOW)).toBe("09-30 03:00 – 10-01 04:57");
  });
});

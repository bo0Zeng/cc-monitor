/**
 * 终端画面的解码与画法（`src/frontend/ui/terminal-screen.ts`）：后端 `terminal-preview` 给的每行 `text` ＋ `spans` ⇒ 带颜色的 DOM。
 *
 * | 性质 | 判据 |
 * |---|---|
 * | 读得懂后端真出的成品（跨语言金样 `terminals.golden.json`）；`spans` 缺 ＝ 没颜色；形状不对 ⇒ 抛 | 「解码」 |
 * | `from` / `to` 按字符计（后端按 Unicode 标量数列），中文与表情不错位 | 「按字符」 |
 * | 16 色名走 `data-fg` / `data-bg`（CSS 映射到 `--term-*`）；`#rrggbb` 原样；认不出的颜色不画；粗 · 暗 · 斜 · 下划线照画；反显把前景背景对调 | 「画法」 |
 * | 段外的字原样是文字节点；整屏用换行接、与纯文字逐字相同 | 「文字不变」 |
 */
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { decodeScreenLines, renderScreen, screenText, type ScreenLine } from "../../../src/frontend/ui/terminal-screen";
import { REPO_ROOT } from "../../test-support/repo-root";
import { copyPattern } from "../../test-support/copy-pattern";

const golden = JSON.parse(readFileSync(resolve(REPO_ROOT, "tests/__fixtures__/terminals.golden.json"), "utf8")) as Record<string, { reply: Record<string, unknown> }>;
const PREVIEW = golden["terminal-preview"].reply;

function painted(lines: ScreenLine[]): HTMLPreElement {
  const pre = document.createElement("pre");
  renderScreen(pre, lines);
  return pre;
}

describe("解码", () => {
  it("★★ 金样：每行的字与颜色段都读出来（第一行前四个字红色加粗）", () => {
    const lines = decodeScreenLines("devbox", PREVIEW);
    expect(lines).toEqual([
      { text: "line one", spans: [{ from: 0, to: 4, fg: "red", bold: true }] },
      { text: "line two", spans: [] },
    ]);
  });

  it("★ 没有 `spans` ＝ 没颜色；多出来的格照收；形状不对 ⇒ 抛", () => {
    expect(decodeScreenLines("devbox", { lines: [{ text: "a" }] })).toEqual([{ text: "a", spans: [] }]);
    expect(decodeScreenLines("devbox", { lines: [{ text: "a", spans: [{ from: 0, to: 1, fg: "red", blink: true }] }], extra: 1 })).toEqual([
      { text: "a", spans: [{ from: 0, to: 1, fg: "red" }] },
    ]);
    for (const v of [{}, { lines: [{ text: "a", spans: "x" }] }, { lines: [{ text: "a", spans: [{ from: "0", to: 1 }] }] }, { lines: [{ text: "a", spans: [{ from: 2, to: 1 }] }] }]) {
      expect(() => decodeScreenLines("devbox", v), JSON.stringify(v)).toThrow(copyPattern("peerVersion.said.unreadable"));
    }
  });
});

describe("画法", () => {
  it("★★ 16 色名走 data 属性、`#rrggbb` 原样；段外的字是文字节点；整屏文字不变", () => {
    const lines: ScreenLine[] = [{ text: "ab cd ef", spans: [{ from: 0, to: 2, fg: "red", bold: true }, { from: 3, to: 5, fg: "#87afff", bg: "bright-black" }] }, { text: "plain", spans: [] }];
    const pre = painted(lines);
    expect(pre.textContent).toBe("ab cd ef\nplain");
    expect(pre.textContent).toBe(screenText(lines));
    const spans = [...pre.querySelectorAll("span")];
    expect(spans.map((s) => s.textContent)).toEqual(["ab", "cd"]);
    expect(spans[0].dataset).toMatchObject({ fg: "red", bold: "" });
    expect(spans[1].dataset.fg, "真彩不走名字").toBeUndefined();
    expect(spans[1].style.color).toBe("rgb(135, 175, 255)");
    expect(spans[1].dataset.bg).toBe("bright-black");
  });

  it("★ 按字符计：中文与表情占一个下标，不按 UTF-16 错位", () => {
    const pre = painted([{ text: "中文😀ok", spans: [{ from: 2, to: 3, fg: "green" }, { from: 3, to: 5, italic: true }] }]);
    expect([...pre.querySelectorAll("span")].map((s) => s.textContent)).toEqual(["😀", "ok"]);
    expect(pre.textContent).toBe("中文😀ok");
  });

  it("★ 暗 · 斜 · 下划线照画；认不出的颜色不画；反显把前景背景对调（缺的用画面的默认色）", () => {
    const pre = painted([{ text: "abcdef", spans: [{ from: 0, to: 2, dim: true, underline: true }, { from: 2, to: 4, fg: "chartreuse" }, { from: 4, to: 6, fg: "cyan", inverse: true }] }]);
    const [a, b, c] = [...pre.querySelectorAll("span")];
    expect(a.dataset).toMatchObject({ dim: "", underline: "" });
    expect(b.dataset.fg, "认不出的颜色名不许上屏").toBeUndefined();
    expect(b.style.color).toBe("");
    expect([c.dataset.fg, c.dataset.bg]).toEqual(["default-bg", "cyan"]);
  });

  it("★ 段越界 / 重叠：越界截到行尾，重叠的后一段从前一段结束处起（字一个不丢不重）", () => {
    const pre = painted([{ text: "abcd", spans: [{ from: 0, to: 3, fg: "red" }, { from: 2, to: 9, fg: "blue" }] }]);
    expect([...pre.querySelectorAll("span")].map((s) => [s.textContent, s.dataset.fg])).toEqual([
      ["abc", "red"],
      ["d", "blue"],
    ]);
    expect(pre.textContent).toBe("abcd");
  });
});

describe("实时画面一帧一帧换：只换变了的行", () => {
  const L = (text: string, spans: ScreenLine["spans"] = []): ScreenLine => ({ text, spans });
  const red = (from: number, to: number) => [{ from, to, fg: "red" as const }];

  it("换一帧：没变的行节点原样留着（不整屏拆了重建）；换完与从头画一模一样 —— 行数变多 / 变少 / 首行变空也一样", () => {
    const frames: ScreenLine[][] = [
      [L("● Bash(npm run build)", red(0, 1)), L("  ⎿  Running…"), L(""), L("✶ Building… (1s)", red(0, 1))],
      [L("● Bash(npm run build)", red(0, 1)), L("  ⎿  Running…"), L("     ✓ 812 modules"), L("✶ Building… (2s)", red(0, 1))],
      [L("● Bash(npm run build)", red(0, 1)), L("  ⎿  Running…"), L("     ✓ 812 modules"), L("     dist/x.js 12 kB"), L("✶ Building… (3s)", red(0, 1))],
      [L(""), L("  ⎿  Running…"), L("✶ Building… (4s)", red(0, 1))],
      [L(""), L("  ⎿  Running…")],
      [],
      [L("$ ", red(0, 1))],
    ];
    const pre = document.createElement("pre");
    renderScreen(pre, frames[0]);
    for (let k = 1; k < frames.length; k++) {
      const prev = frames[k - 1];
      const before = [...pre.childNodes];
      const keepFirst = prev[1]?.text === frames[k][1]?.text ? before.find((n) => n.textContent === "  ⎿  Running…") : undefined;
      renderScreen(pre, frames[k]);
      expect(pre.innerHTML, `第 ${k} 帧`).toBe(painted(frames[k]).innerHTML);
      if (keepFirst) expect([...pre.childNodes].includes(keepFirst), `第 ${k} 帧：没变的那一行该原样留着`).toBe(true);
    }
  });

  it("别处动过这块画面（不是上一帧画出来的样子）⇒ 整屏重画，不在别人的节点上打补丁", () => {
    const pre = document.createElement("pre");
    renderScreen(pre, [L("a"), L("b")]);
    pre.append("外人加的");
    renderScreen(pre, [L("a"), L("c")]);
    expect(pre.innerHTML).toBe(painted([L("a"), L("c")]).innerHTML);
  });
});

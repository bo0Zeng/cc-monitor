/**
 * 命令面板的 ↗ 那一项在非 Windows 上收起（`src/frontend/ui/terminal-front-command.ts`）。
 *
 * 两格：
 * 1. **行为**：门本身 —— 四种 OS 各一格，与 tab 上那颗 ↗ 按钮同一道门（`terminalFrontAvailable`）：
 *    Windows 列、Linux / macOS 不列、测不出 OS 照常列（失败方向照 `terminal-front.ts` 头注）。
 * 2. **接线**：`main.ts` 的命令表里 `term-front` 恰好一处，而且就是经这道门展开进去的那一处。
 *    ⚠ 这一格是**文本**判据 —— `main.ts` 是入口模块、一个 export 都没有，命令表在闭包里，
 *    行为判据够不着它（`commands.vitest.ts` 的 `K-P5h` 那条为同一个理由也是文本判据）。
 *    它只买「那一项还接在门后面、没有第二份绕过门的副本」，买不到「面板真的少了一行」。
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { __setHostFactsForTests, type HostOs } from "../../../src/frontend/ui/settings/host-os";
import { terminalFrontCommand } from "../../../src/frontend/ui/terminal-front-command";
import { REPO_ROOT } from "../../test-support/repo-root";
import { stripComments } from "../../test-support/strip-comments";
import { copyText } from "../../../src/frontend/ui/copy-table";
import { factsOn } from "../../test-support/host-facts";

describe("命令面板的 ↗：非 Windows 灰着、第二行「仅 Windows」", () => {
  afterEach(() => __setHostFactsForTests(null));

  const cases: ReadonlyArray<readonly [HostOs, boolean]> = [
    ["windows", true],
    ["linux", false],
    ["macos", false],
    ["unknown", true], // 测不出 OS ⇒ 当能用（错灰的代价是 Windows 用户点不了它）
  ];
  for (const [os, ok] of cases) {
    it(`${os} ⇒ ${ok ? "能用" : "灰着"}`, () => {
      __setHostFactsForTests(factsOn(os));
      const item = { id: "term-front" };
      const got = terminalFrontCommand(item);
      expect(got.length).toBe(1);
      if (ok) expect(got[0]).toBe(item); // 原样那一项，不是副本
      else expect(got[0]).toEqual({ id: "term-front", disabled: copyText("terminalFront.unavailable.short") });
    });
  }

  it("★ 接线：main.ts 里 term-front 恰好一处，且就是经 terminalFrontCommand 展开的那一处", () => {
    const code = stripComments(readFileSync(resolve(REPO_ROOT, "src/frontend/ui/main.ts"), "utf8"), "ts");
    // 抽取器自检：命令表真的读到了（隔壁那几项都在）。
    expect(code).toContain('id: "toggle-tasks"');
    expect(code).toContain('id: "win-fullscreen"');
    expect((code.match(/"term-front"/g) ?? []).length, "term-front 那一项的份数").toBe(1);
    expect(
      (code.match(/\.\.\.terminalFrontCommand\(\{\s*id:\s*"term-front"/g) ?? []).length,
      "term-front 没有经 terminalFrontCommand 展开 —— 非 Windows 上它又会列出来",
    ).toBe(1);
  });
});

/**
 * 独立只读窗（`entry-viewer.ts`）顶栏那颗「↗ 终端」也过同一道门。
 *
 * 只给 tab 上那颗和命令面板那一项加了门，viewer 顶栏那颗在非 Windows 上照样渲（每点必败）。
 * ⚠ **文本**判据，理由同上一格：`entry-viewer.ts` 是入口模块，一 import 就挂 DOMContentLoaded、
 * 装全局错误捕获，顶栏在 `bootstrapViewer` 闭包里建 —— 行为判据够不着。
 * 它只买「那颗按钮挂进顶栏的那一处恰好一处、而且就在门后面」，买不到「真窗口里少了一颗按钮」。
 */
describe("〔S4〕viewer 顶栏的 ↗：与 tab 上那颗同一道门", () => {
  it("★ 接线：termBtn 挂进顶栏恰好一处，且那一处就是门后面那一句", () => {
    const code = stripComments(readFileSync(resolve(REPO_ROOT, "src/frontend/ui/entry-viewer.ts"), "utf8"), "ts");
    // 抽取器自检：顶栏右端真的读到了（隔壁那颗「打开目录」按钮在）。
    expect(code).toContain('hint: copyText("history.menu.openDir")');
    // 「切到终端」只在一处建，且就在门后面（在跑 ＆ 这台系统上 ↗ 的最后一跳不是桩）。
    expect((code.match(/copyText\("tabBarView\.tab\.terminalHint"\)/g) ?? []).length, "「切到终端」那颗的字出现的份数（读屏名 ＋ 悬停）").toBe(2);
    expect(
      (code.match(/if \(live && terminalFrontAvailable\(\)\) \{\s*const origin = r\.origin \?\? LOCAL_ORIGIN;\s*out\.push\(\s*button\(\{\s*label: copyText\("tabBarView\.tab\.terminalHint"\)/g) ?? []).length,
      "「切到终端」没有挂在 terminalFrontAvailable 门后面 —— 非 Windows 上它又会渲出来",
    ).toBe(1);
  });
});

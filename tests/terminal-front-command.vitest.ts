/**
 * 〔U2 · 第三波〕命令面板的 ↗ 那一项在非 Windows 上收起（`src/terminal-front-command.ts`）。
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
import { __setHostOsForTests, type HostOs } from "../src/settings/host-os";
import { terminalFrontCommand } from "../src/terminal-front-command";
import { REPO_ROOT } from "./test-support/repo-root";
import { stripComments } from "./test-support/strip-comments";

describe("〔U2〕命令面板的 ↗：非 Windows 不列", () => {
  afterEach(() => __setHostOsForTests(null));

  const cases: ReadonlyArray<readonly [HostOs, number]> = [
    ["windows", 1],
    ["linux", 0],
    ["macos", 0],
    ["unknown", 1], // 测不出 OS ⇒ 照常列（错藏的代价是 Windows 用户找不到它）
  ];
  for (const [os, n] of cases) {
    it(`${os} ⇒ 列 ${n} 项`, () => {
      __setHostOsForTests(os);
      const item = { id: "term-front" };
      const got = terminalFrontCommand(item);
      expect(got.length).toBe(n);
      if (n === 1) expect(got[0]).toBe(item); // 原样那一项，不是副本
    });
  }

  it("★ 接线：main.ts 里 term-front 恰好一处，且就是经 terminalFrontCommand 展开的那一处", () => {
    const code = stripComments(readFileSync(resolve(REPO_ROOT, "src/main.ts"), "utf8"), "ts");
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

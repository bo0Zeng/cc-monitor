// 根元素上标出界面跑在哪个系统上（`data-host-os`），给只能按平台分的那几条样式用 —— 今天一条：Linux 上等宽字体直接用通用名。
// 来由（L0 · 10-08 真窗口）：WebKitGTK 里字体栈的第一个具名字体没装时，fontconfig 把它顶成一款非等宽中文字体、WebKit 照收，
// 后面的 Consolas / monospace 不再看 ⇒ 终端画面那几行不是等宽字。
import { describe, it, expect, afterEach } from "vitest";
import { readFileSync } from "node:fs";
import { markHostOs, __setHostOsForTests } from "../../../../src/frontend/ui/settings/host-os";

afterEach(() => __setHostOsForTests(null));

describe("data-host-os", () => {
  it("按测出来的系统标在根元素上", () => {
    for (const os of ["linux", "windows", "macos", "unknown"] as const) {
      __setHostOsForTests(os);
      const root = document.createElement("html");
      markHostOs(root);
      expect(root.dataset.hostOs).toBe(os);
    }
  });

  it("三个窗口的入口都标（entry-common 一处）", () => {
    expect(readFileSync("src/frontend/ui/entry-common.ts", "utf8")).toMatch(/^markHostOs\(document\.documentElement\);$/m);
  });

  it("Linux 上等宽字体的缺省是通用名 monospace（别处照旧那一串）", () => {
    const css = readFileSync("src/frontend/ui/styles/tokens.css", "utf8");
    const rule = /:root\[data-host-os="linux"\]\s*\{\s*--font-mono:\s*monospace;\s*\}/;
    expect(css).toMatch(rule);
    expect(css).toMatch(/^\s*--font-mono: "JetBrains Mono", Consolas, monospace;$/m);
  });
});

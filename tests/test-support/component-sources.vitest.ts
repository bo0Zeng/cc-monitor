/**
 * S4b-3b-3：`componentSources` 自身的测试。
 *
 * 它是那几条源码级守卫的地基 —— 地基空转的话，守卫会**全绿地失效**，
 * 而这正是主计划 §5-4 提醒的那种病。
 */
import { describe, it, expect } from "vitest";
import { componentSources, stripLineComments } from "./component-sources";

describe("componentSources", () => {
  it("★ 一个前缀能收到**多个**文件（这就是「扩到组件级」的全部意义）", () => {
    // `cc-bus` 前缀今天真实覆盖两个组件文件。写死单文件名的老守卫只看得见其中一个。
    const files = componentSources("cc-bus").map((f) => f.file);
    expect(files.length).toBeGreaterThan(1);
    expect(files).toContain("cc-bus-section.ts");
    expect(files).toContain("cc-bus-hooks-section.ts");
  });

  it("排除测试文件（否则守卫会扫到自己的断言字符串，恒红或恒绿都可能）", () => {
    for (const f of componentSources("cc-bus")) {
      expect(f.file).not.toMatch(/\.vitest\.|\.test\./);
    }
  });

  it("前缀没匹配到任何文件时返回空数组（调用方须自检，见各守卫）", () => {
    expect(componentSources("根本没有这个组件")).toEqual([]);
  });

  it("剥掉整行注释——守卫扫的是代码，文件头注里写了禁用模式不该打红自己", () => {
    const src = ["// setInterval 在注释里", "const a = 1;", " * setInterval 也在注释里"].join(
      "\n",
    );
    const out = stripLineComments(src);
    expect(out).not.toContain("setInterval");
    expect(out).toContain("const a = 1;");
  });

  it("返回的 code 已剥注释（守卫不必再剥一遍）", () => {
    const hooks = componentSources("cc-bus-hooks");
    expect(hooks.length).toBeGreaterThan(0);
    // 文件头注是 `//` 开头的整行，剥完不该还在
    expect(hooks[0]!.code.split("\n").some((l) => l.trimStart().startsWith("//"))).toBe(
      false,
    );
  });
});

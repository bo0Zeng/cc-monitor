/**
 * `launch-render-fallback.ts` 的**前提触发器**〔audit-0805 08-08，Phase G 第 91 件〕。
 *
 * # 它守的是「那道编译器保护还在不在」
 *
 * 该模块头注写着「**sanitize 必须先于 wrap**」。08-08 先核发现：那句话当时说的
 * 「函数组合上的**结构保证**」**不成立** —— `renderArgv` 与 `applyWraps` 收发的都是
 * `string`，写 `applyWraps(plan.launcher, …)` 照样编得过，而且**全仓零判据**
 *（今天 `plan.wrap` 恒空 ⇒ 行为测试也测不出差别）。所谓结构保证，只是两处调用点
 * 碰巧写成了嵌套。
 *
 * 已改成真的：`renderArgv` 返回带标记的 `SanitizedArgv`，`applyWraps` 只收它。
 * 实测把调用改成 `applyWraps(plan.launcher, …)` ⇒ **TS2345**，编译器当场拦下。
 *
 * ⇒ **但编译器保护本身没人守**：谁把 `SanitizedArgv` 换回 `string`（比如觉得
 * 那个 `as` 碍眼），保护就悄悄消失，而 tsc / vitest 全绿 —— 与本会话反复逮到的
 * 「散文说有、实际没有」是同一形状，只是这次散文说的是类型。
 *
 * ⚠ **本条是源码层**，如实说：它挡得住「标记类型被撤掉/被改成 `string`」，
 * **挡不住**「有人用 `as SanitizedArgv` 强转一个没净化的串」——那是显式的、
 * 会在 review 里看见的动作，而本条要挡的是**悄悄的**那种。
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { describe, it, expect } from "vitest";

import { REPO_ROOT } from "./test-support/repo-root.ts";

describe("sanitize 先于 wrap 的类型保证", () => {
  const src = readFileSync(resolve(REPO_ROOT, "src/launch-render-fallback.ts"), "utf8");
  const code = src
    .split("\n")
    .filter((l) => {
      const s = l.trim();
      return !s.startsWith("//") && !s.startsWith("*") && !s.startsWith("/*");
    })
    .join("\n");

  it("`renderArgv` 仍然返回带标记的类型（不是裸 string）", () => {
    expect(
      code,
      "`renderArgv` 的返回类型不再是 `SanitizedArgv` —— 那道**编译器保护**没了：\n" +
        "  `applyWraps(plan.launcher, …)` 会重新变成合法代码，而 tsc / vitest 全绿。\n" +
        "  该模块头注逐字说着「sanitize 必须先于 wrap」，08-08 之前那句话是**空的**，\n" +
        "  现在靠这个标记类型撑着。",
    ).toContain("function renderArgv(plan: LaunchPlan): SanitizedArgv");
  });

  it("`applyWraps` 只收带标记的输入", () => {
    expect(
      code,
      "`applyWraps` 的第一个参数不再是 `SanitizedArgv` —— 任何裸串都能被包进\n" +
        "  `( prelude; exec <inner> )` 里送去远端执行。今天 `plan.wrap` 恒空所以看不出，\n" +
        "  而 F04 那批 wrap 落地时这就是一条真实的注入面。",
    ).toContain("function applyWraps(inner: SanitizedArgv");
  });

  it("标记类型本身还在（不是被 `type SanitizedArgv = string` 掏空的）", () => {
    const decl = /type SanitizedArgv = string & \{[^}]*unique symbol[^}]*\};/.exec(code);
    expect(
      decl,
      "`SanitizedArgv` 不再是**带 `unique symbol` 的品牌类型** —— 若它被改成\n" +
        "  `type SanitizedArgv = string`，上面两条会照旧绿，而裸串又能直接传进去了。\n" +
        "  这条挡的正是「把保护掏空、名字留着」那种改法。",
    ).not.toBeNull();
  });
});

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
import { isValidRbindToken } from "../src/launch-dimensions.ts";

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

/**
 * `设计/80 §8` 步 1：**启动期令牌的形状在 TS 与 Rust 两侧是同一个形状。**
 *
 * # 为什么这条判据必须存在，而且必须从 Rust 源码里抽
 *
 * 令牌是一个**要跨三个进程比相等**的串：monitor 铸它 → 远端 agent 进程的 `environ` 里
 * 存它 → 后端读回来从 wire 报回 → 本地拿它去 join 一张 `token → HWND` 的表。
 * 两侧的形状只要差一点（大小写、长度、允不允许 `-`），失效形态就是
 * **`↗` 拉不到窗口、而两侧各自的单测全绿** —— 归因会指向别处（`§8.5 ②` 要治的就是这个）。
 *
 * ⇒ 断言**从 `payload.rs` 现取**（`RBIND_TOKEN_LEN` 那个常量 ＋ 放行字节区间），
 * 不手抄一个 32。改 Rust 不改 TS ⇒ 本条红。
 *
 * ⚠ 它**买不到**「两侧同时改错」：那时两边仍然一致，本条照绿。
 * 挡那一类的是各侧自己的语义判据（TS：`launch-dimensions.test.ts` 的形状闸逐格；
 * Rust：`payload_tests.rs` 里那条 fail-closed 用例）。**这是已登记的边界，不是漏。**
 */
describe("启动期令牌形状：TS ↔ Rust 同口径（断言从 Rust 源码抽，不手抄）", () => {
  const rust = readFileSync(
    resolve(REPO_ROOT, "src/bridge/src/backend/control/payload.rs"),
    "utf8",
  );

  const declaredLen = (() => {
    const m = /pub const RBIND_TOKEN_LEN: usize = (\d+);/.exec(rust);
    return m === null ? null : Number(m[1]);
  })();

  it("抽取器自检：真的从 Rust 抽到了那个长度常量（抽空就会零命中地绿）", () => {
    expect(
      declaredLen,
      "`payload.rs` 里找不到 `pub const RBIND_TOKEN_LEN: usize = …;` —— " +
        "抽取器够不着了（改名？改成局部字面量？）。**这不是零命中，是尺子没跑。** " +
        "先修锚点，别把本条当成绿的。",
    ).not.toBeNull();
    expect(declaredLen).toBeGreaterThan(0);
  });

  it("TS 侧接受的长度恰好是 Rust 声明的那个（长度差一 ⇒ 拒）", () => {
    const n = declaredLen!;
    const hex = "0123456789abcdef";
    const of = (k: number) =>
      Array.from({ length: k }, (_, i) => hex[i % hex.length]).join("");
    expect(isValidRbindToken(of(n)), `${n} 位小写 hex 必须被接受`).toBe(true);
    expect(isValidRbindToken(of(n - 1)), `${n - 1} 位必须被拒`).toBe(false);
    expect(isValidRbindToken(of(n + 1)), `${n + 1} 位必须被拒`).toBe(false);
  });

  it("两侧字符集都是**小写** hex —— Rust 的放行区间里不许出现大写那一段", () => {
    const fn = rust.slice(
      rust.indexOf("pub fn rbind_token_shape_ok("),
      rust.indexOf("\n}", rust.indexOf("pub fn rbind_token_shape_ok(")),
    );
    expect(fn.length, "切不出 `rbind_token_shape_ok` 的体 —— 锚点坏了").toBeGreaterThan(40);
    expect(fn, "Rust 侧不再按 `b'0'..=b'9'` 放行数字了？形状变了").toContain("b'0'..=b'9'");
    expect(fn, "Rust 侧不再按 `b'a'..=b'f'` 放行小写了？形状变了").toContain("b'a'..=b'f'");
    // ★ 要害：大写那一段**不许**被放行（收了大写就等于允许同一个令牌两种写法，
    //   而 join 的两侧没有归一化步骤）。
    // ⚠ 用 `not.toContain` 而不是 `fn.includes(…)`：`scanning-guard-registry` 那条**递减棘轮**
    //   数的是「磁盘语料变量上的裸 `.includes("…")`」（匹配单位比事实小 ⇒ 正向事实钉会
    //   从缝里溜过去）。本条第一版写的就是 `.includes`，那条棘轮当场把上限从 8 顶到 10 并红 ——
    //   **它说对了**，改写法而不是抬上限。
    const upperHexForms = ["b'A'", "is_ascii_hexdigit"] as const;
    for (const form of upperHexForms) {
      expect(
        fn,
        `Rust 侧开始收大写 hex 了（\`${form}\`）—— TS 侧的 \`/^[0-9a-f]{32}$/\` 不收，两侧当场分家`,
      ).not.toContain(form);
    }
    // TS 侧的对应事实（行为，不是源码）：
    expect(isValidRbindToken("0F1E2D3C4B5A69788796A5B4C3D2E1F0")).toBe(false);
  });
});

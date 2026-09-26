/**
 * `设计/80 §8` 步 1 / `§8.6 ③`：**启动期令牌的形状在 TS 与 Rust 两侧是同一个形状。**
 *
 * 住址：`设计/80 §8.6 ③`（令牌「只能是一个不可猜的关联 id」，`[0-9a-f]{32}`，两侧 fail-closed 校验）。
 *
 * 〔LR2〕这一组原住 `tests/launch-render-fallback.vitest.ts`（TS 兜底渲染器的前提触发器那份文件）。
 * 那份渲染器按 `设计/00 §2.5 ④` 删了，文件里另一组（「sanitize 先于 wrap」的类型保证）随被测对象一起走；
 * **这一组与渲染器无关** —— 它守的是生产的铸币口与维度闸（`launch-dimensions.ts::isValidRbindToken`，
 * `remote-launch-run.ts::mintRbindToken` 与 `RBIND_TOKEN_DIMENSION` 都过它）对 Rust `payload.rs` 的同口径，
 * 所以原样搬来，一个断言没改。
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { describe, it, expect } from "vitest";

import { REPO_ROOT } from "./test-support/repo-root.ts";
import { isValidRbindToken } from "../src/launch-dimensions.ts";


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

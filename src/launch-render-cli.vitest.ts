/**
 * `launch-render-cli.ts` 的**唯一入口**前提触发器〔audit-0805 08-08，Phase G 第 92 件〕。
 *
 * # 它守的是一句「结构保证」到底还成不成立
 *
 * 该模块头注逐字写着改造前后的差别：
 *
 * > 改造前是两个独立导出：调用方先问 `canRenderCli`，为真再调 `renderCli` ……
 * > 也就是说「诚实降级」此前只是**调用约定**，不是结构保证：任何直接调 `renderCli`
 * > 的新代码路径都会**静默丢修饰**，而丢的恰好是账号这类东西 —— 症状就是 R11/R08
 * > 那族「**看起来生效了，只是用了错的号**」。
 * > 合成之后 …… 想绕过这条闸门就必须绕过唯一的入口，而那是显式的、可被 review 看见的动作。
 *
 * 08-08 先核：**今天确实只导出 `tryRenderCli`（+ 一个结果类型）**，那句话成立。
 * ⚠ 但**没有任何判据读它** —— 谁加回一个 `export function renderCli(...)`（或导出
 * 任何别的渲染函数），「结构保证」当天就退回成「调用约定」，而 tsc / vitest 全绿、
 * 头注还理直气壮地写着「结构保证」。这正是本会话反复逮到的形状：**散文说有、实际没有**，
 * 只是这次是「说的时候有，后来悄悄没了」。
 *
 * # 边界（如实说）
 *
 * 本条是**源码层**：挡得住「新增导出一个渲染入口」，**挡不住**「有人在 `tryRenderCli`
 * 体内把 `{ ok: false }` 那支改成静默跳过」——那半由 `launch-render-cli.test.ts` 的
 * 行为断言（含 9 条黄金串）看着。两层失效模式不同，是真纵深。
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { describe, it, expect } from "vitest";

import { REPO_ROOT } from "./test-support/repo-root.ts";

describe("CLI 渲染只有一个入口", () => {
  const code = readFileSync(resolve(REPO_ROOT, "src/launch-render-cli.ts"), "utf8")
    .split("\n")
    .filter((l) => {
      const s = l.trim();
      return !s.startsWith("//") && !s.startsWith("*") && !s.startsWith("/*");
    })
    .join("\n");

  /** 该模块导出的**函数**名（`export function X(`）。 */
  const exportedFns = [...code.matchAll(/^export function (\w+)\s*\(/gm)].map((m) => m[1]);

  it("导出的渲染函数恰好是 `tryRenderCli` 一个", () => {
    expect(
      exportedFns.length,
      "一个 `export function` 都没抠到 —— 抽取器坏了（本条会零命中地绿），先修它再谈结论",
    ).toBeGreaterThanOrEqual(1);
    expect(
      exportedFns,
      "该模块导出的函数变了。\n" +
        "★ 头注逐字写着：改造前 `canRenderCli` / `renderCli` 是两个独立导出，于是「诚实降级」\n" +
        "  只是**调用约定** —— 任何直接调 `renderCli` 的新路径都会**静默丢修饰**，\n" +
        "  而丢的恰好是账号这类东西，症状是 R11/R08 那族「看起来生效了，只是用了错的号」。\n" +
        "  合成成单一入口之后那句话才成立。\n" +
        "⇒ 新增一个导出的渲染函数 = 把「结构保证」退回成「调用约定」，而 tsc / vitest 全绿。\n" +
        "  真要加：先回答「它凭什么不会绕过能力闸门」，并把头注那段一起改掉。",
    ).toEqual(["tryRenderCli"]);
  });

  it("能力不足时走的是「拿不到命令」，不是「静默跳过」", () => {
    // 结构那一半：闸门必须以**返回失败**的形式存在于同一次遍历里。
    expect(
      code,
      "`tryRenderCli` 里找不到「能力不足 ⇒ 返回 ok:false」那一支 —— \n" +
        "  头注说的「`null` 在同一次遍历里直接变成 `{ ok: false }`：**拿不到命令**」没了，\n" +
        "  渲染器会退回「静默跳过这个维度」，即改造前那个丢修饰的形状。",
    ).toMatch(/return \{ ok: false, reason: `维度/);
  });
});

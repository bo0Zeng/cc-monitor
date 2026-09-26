/**
 * `设计/80 §8` 步 1 / `§8.6 ③`：**启动期令牌：前端铸出来的，恰好落在 Rust 那一条形状里。**
 *
 * 住址：`设计/80 §8.6 ③`（令牌「只能是一个不可猜的关联 id」，`[0-9a-f]{32}`，fail-closed 校验）·
 * `设计/90 §3` 判据 2（凡是有对应 Rust 判定的，TS 侧零实现）。
 *
 * 〔DUP2 · J8〕这一组原来拿 TS 手写的形状副本（`launch-dimensions.ts` 里那一份）对 Rust 源码。今天那份副本删了：
 * 铸币口 `remote-launch-run.ts::mintRbindToken` 按生成物（`RBIND_TOKEN_ALPHABET` × `RBIND_TOKEN_LEN`，monitor 从 `payload.rs` 那两个常量
 * 现生成）**造**令牌。本条守的是「铸币输出 ⊂ Rust 规则」—— 断言的规则**从 `payload.rs` 源码现抠**（异源：源码文本，
 * 不是生成物；生成物漂了、或铸币口造错了，这里都红）。
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { describe, it, expect } from "vitest";

import { REPO_ROOT } from "./test-support/repo-root.ts";
import { mintRbindToken } from "../src/remote-launch-run.ts";

/**
 * # 为什么规则必须从 Rust 源码里抠
 *
 * 令牌是一个**要跨三个进程比相等**的串：monitor 铸它 → 远端 agent 进程的 `environ` 里
 * 存它 → 后端读回来从 wire 报回 → 本地拿它去 join 一张 `token → HWND` 的表。
 * 铸出来的只要有一位落在形状外（大写、长度差一），失效形态就是
 * **`↗` 拉不到窗口、而两侧各自的单测全绿** —— 归因会指向别处（`§8.5 ②` 要治的就是这个）。
 *
 * ⇒ 长度与字母表**从 `payload.rs` 现取**，并核 `rbind_token_shape_ok` 的体真的是按这两个常量判的
 * （否则抠到的只是两个没人用的数）。
 *
 * ⚠ 它**买不到**「Rust 那条形状本身写错」：那时铸币口与渲染闸一起错、本条照绿 ——
 * 挡那一类的是 Rust 侧 `payload_tests.rs` 里那条 fail-closed 用例（手写的坏样本）。
 */
describe("启动期令牌：铸币输出 ⊂ Rust 那一条形状（规则从 payload.rs 源码抠，不共用生成物）", () => {
  const rust = readFileSync(resolve(REPO_ROOT, "src/bridge/src/backend/control/payload.rs"), "utf8");
  const declaredLen = (() => {
    const m = /pub const RBIND_TOKEN_LEN: usize = (\d+);/.exec(rust);
    return m === null ? null : Number(m[1]);
  })();
  const declaredAlphabet = (() => {
    const m = /pub const RBIND_TOKEN_ALPHABET: &str = "([^"]*)";/.exec(rust);
    return m === null ? null : m[1];
  })();

  it("抽取器自检：真的从 Rust 抽到了长度与字母表（抽空就会零命中地绿）", () => {
    expect(declaredLen, "`payload.rs` 里找不到 `pub const RBIND_TOKEN_LEN: usize = …;` —— 尺子没跑").not.toBeNull();
    expect(declaredAlphabet, "`payload.rs` 里找不到 `pub const RBIND_TOKEN_ALPHABET: &str = \"…\";` —— 尺子没跑").not.toBeNull();
    expect(declaredLen).toBeGreaterThan(0);
    expect(declaredAlphabet!.length).toBeGreaterThan(1);
  });

  it("抠到的两个常量就是 `rbind_token_shape_ok` 真在用的那两个（不是两个没人读的数）", () => {
    const at = rust.indexOf("pub fn rbind_token_shape_ok(");
    const body = rust.slice(at, rust.indexOf("\n}", at));
    expect(body.length, "切不出 `rbind_token_shape_ok` 的体 —— 锚点坏了").toBeGreaterThan(40);
    expect(body).toContain("RBIND_TOKEN_LEN");
    expect(body).toContain("RBIND_TOKEN_ALPHABET");
  });

  it("★ 铸 256 个：每个长度 == Rust 的长度、每一位都在 Rust 的字母表里", () => {
    const bad: string[] = [];
    for (let i = 0; i < 256; i += 1) {
      const t = mintRbindToken();
      if (t.length !== declaredLen || [...t].some((ch) => !declaredAlphabet!.includes(ch))) bad.push(t);
    }
    expect(bad).toEqual([]);
  });

  it("Rust 的字母表只有小写 hex（收了大写 ⇒ 同一个令牌两种写法，而 join 的两侧没有归一化步骤）", () => {
    expect(declaredAlphabet).toBe(declaredAlphabet!.toLowerCase());
    expect([...declaredAlphabet!].every((ch) => /[0-9a-f]/.test(ch))).toBe(true);
  });
});

/**
 * `设计/90 §4 E`：外层 tmux 那三格的夹具 **入库版 == 现场渲染版**。
 *
 * 这是跨语言对拍的 TS 那一半（另一半是
 * `src/frontend/shell/src/backend/control/launch_tmux_outer_parity.rs`）。
 * 它挡的是唯一一种能让对拍静默失效的改法：**改了 TS 渲染器但没重生成夹具** ——
 * 那时 Rust 侧仍与旧夹具一致、全绿，而两种语言其实已经分家了。
 *
 * ⚠ 本文件被 Rust 侧 `include_str!` 着（`TS_HALF`）：改名/删除 ⇒ 那边**编译失败**，
 * 掏空断言 ⇒ 那边 `the_typescript_half_still_asserts_the_fixture_is_current` 红。
 */
import { describe, expect, test } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { TMUX_OUTER_CASES, renderTmuxOuterFixture } from "../../test-support/launch-tmux-outer-golden.ts";

// `import.meta.url` 在 vitest 里不是 `file:` scheme —— 照本仓既有做法用 `resolve(__dirname, "../../..")`。
const FIXTURE_PATH = resolve(
  __dirname,
  "../../..",
  "src/backend/control/launch_render/fixtures/tmux-outer-golden.json",
);

describe("外层 tmux 命令黄金串夹具（`设计/90 §4 E` 跨语言对拍的 TS 半边）", () => {
  test("入库的夹具与现场渲染逐字节相同（改了渲染器/座就得重生成）", () => {
    expect(readFileSync(FIXTURE_PATH, "utf8")).toBe(renderTmuxOuterFixture());
  });

  // 用例数那条**刻意只留 Rust 侧**（那边是 `assert_eq!`，强制触碰）。
  // 这边不需要：上面那条是**全文件字节比较**，夹具被清空/截断时它必红。

  test("三格每一格都真的有用例（只数总数挡不住「全写成 create」）", () => {
    const modes = new Set(TMUX_OUTER_CASES.map((c) => c.mode ?? "attach"));
    expect([...modes].sort()).toEqual(["attach", "create", "send-into"]);
  });

  test("每条用例都真的渲染出了一条 tmux 命令（反空真）", () => {
    const parsed = JSON.parse(readFileSync(FIXTURE_PATH, "utf8")) as {
      cases: { name: string; cmd: string }[];
    };
    const bad = parsed.cases.filter((c) => !c.cmd.startsWith("tmux ")).map((c) => c.name);
    expect(bad).toEqual([]);
  });

  test("`-t` 一律是精确匹配形态（`=名:`）—— 裸 `-t 名` 会命中前缀/glob", () => {
    const parsed = JSON.parse(readFileSync(FIXTURE_PATH, "utf8")) as {
      cases: { name: string; cmd: string }[];
    };
    // 每一处 `-t <token>`：token 要么是裸的 `=名:`，要么是 `'=名:'`。
    const offenders: string[] = [];
    for (const c of parsed.cases) {
      for (const m of c.cmd.matchAll(/-t ('[^']*'|\S+)/g)) {
        const tok = m[1];
        const inner = tok.startsWith("'") ? tok.slice(1, -1) : tok;
        if (!inner.startsWith("=") || !inner.endsWith(":")) offenders.push(`${c.name}: ${tok}`);
      }
    }
    expect(offenders).toEqual([]);
  });
});

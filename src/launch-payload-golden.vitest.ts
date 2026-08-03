/**
 * U8c-1：夹具**入库版 == 现场渲染版**。
 *
 * 这是跨语言对拍的 TS 那一半（另一半是 `src-tauri/src/launch_payload_parity.rs`）。
 * 它挡的是唯一一种能让对拍静默失效的改法：**改了 TS 渲染器但没重生成夹具** ——
 * 那时 Rust 侧仍与旧夹具一致、全绿，而两种语言其实已经分家了。
 */
import { describe, expect, test } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { GOLDEN_CASES, renderGoldenFixture } from "./launch-payload-golden.ts";

// `import.meta.url` 在 vitest 里不是 `file:` scheme（实测 `The URL must be of scheme file`）——
// 照本仓既有做法用 `resolve(__dirname, "..")`（同 `fork-flow.vitest.ts:114`）。
const FIXTURE_PATH = resolve(
  __dirname,
  "..",
  "src-tauri/crates/launch-core/fixtures/payload-golden.json",
);

describe("载荷黄金串夹具（U8c-1 跨语言对拍的 TS 半边）", () => {
  test("入库的夹具与现场渲染逐字节相同（改了渲染器就得重生成）", () => {
    const onDisk = readFileSync(FIXTURE_PATH, "utf8");
    expect(onDisk).toBe(renderGoldenFixture());
  });

  // 用例数那条地板**刻意只留 Rust 侧**（那边是 `assert_eq!` 强制触碰）。
  // TS 这边不需要：上面那条「入库的 == 现场渲染的」是**全文件字节比较**，
  // 夹具被清空/截断时它必红 —— 再加一条数数的是重复，且是个不会被棘的棘轮。

  test("每条用例都真的渲染出了非空载荷", () => {
    const parsed = JSON.parse(readFileSync(FIXTURE_PATH, "utf8")) as {
      cases: { name: string; payload: string }[];
    };
    const empty = parsed.cases.filter((c) => c.payload.trim() === "").map((c) => c.name);
    expect(empty).toEqual([]);
  });

  test("四种 EnvOp 每一种都至少被一条用例覆盖", () => {
    const seen = new Set(GOLDEN_CASES.flatMap((c) => c.env.map((op) => op.kind)));
    expect([...seen].sort()).toEqual(
      ["export-config-dir", "export-model", "unset-config-dir", "unset-nested-env"].sort(),
    );
  });
});

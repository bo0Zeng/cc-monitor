/**
 * U8c-2c-2：`render_ccm_launch` 的 **wire 形状两侧一致** + **生产真的切过去了**。
 *
 * `src/launch-cli-wire.ts` 是一份**手写镜像**（不是 ts-rs 生成的）。Rust 那边带
 * `deny_unknown_fields` ⇒ 前端多送/少送一个字段会被**拒**，而那在生产里表现为
 * 「拉起时静默走兜底」—— 功能不变砖，但**真正在跑的那条路悄悄换了**，没人会发现。
 * 所以这条对拍是必需的。
 */
import { describe, expect, test } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const read = (p: string) => readFileSync(resolve(__dirname, "..", p), "utf8");
const RUST = read("src-tauri/src/launch_cli_cmd.rs");
const WIRE = read("src/launch-cli-wire.ts");
const RUN = read("src/remote-launch-run.ts");

describe("ccm 调用行的 wire 形状（U8c-2c-2）", () => {
  // Rust 侧 `CliRenderRequest` 用 `rename_all = "camelCase"`，所以字段名要转过来比。
  const rustReqFields = (() => {
    const body = RUST.slice(
      RUST.indexOf("pub struct CliRenderRequest {"),
      RUST.indexOf("}", RUST.indexOf("pub struct CliRenderRequest {")),
    );
    return body
      .split("\n")
      .map((l) => /^\s{4}pub ([a-z_0-9]+):/.exec(l)?.[1])
      .filter((x): x is string => !!x)
      .map((snake) => snake.replace(/_([a-z])/g, (_, c: string) => c.toUpperCase()))
      .sort();
  })();

  test("抽取器自检：真的从 Rust 抽到了字段（抽空就会零命中零失败地绿）", () => {
    expect(rustReqFields.length).toBeGreaterThanOrEqual(8);
  });

  test("TS 手写镜像的字段集 == Rust `CliRenderRequest` 的字段集", () => {
    const body = WIRE.slice(
      WIRE.indexOf("export interface CliRenderRequest {"),
      WIRE.indexOf("}", WIRE.indexOf("export interface CliRenderRequest {")),
    );
    const tsFields = body
      .split("\n")
      .map((l) => /^\s{2}([a-zA-Z0-9]+)[?]?:/.exec(l)?.[1])
      .filter((x): x is string => !!x)
      .sort();
    expect(tsFields).toEqual(rustReqFields);
  });

  test("Rust 侧三个 wire 枚举都带 deny_unknown_fields（多送字段必须被拒，不静默吞）", () => {
    for (const t of ["CliRenderRequest", "WireAction", "WireContainer", "WireAccount"]) {
      const at = RUST.indexOf(`enum ${t} {`) >= 0 ? RUST.indexOf(`enum ${t} {`) : RUST.indexOf(`struct ${t} {`);
      expect(at, `${t} 找不到`).toBeGreaterThan(0);
      // ⚠ **只看紧邻的 `#[...]` 属性行** —— 初版是「往前扫 220 字符找子串」，
      // 而这些类型的**文档注释里就写着** `deny_unknown_fields` 这个词 ⇒ 摘掉真属性照样绿
      // （自己的变异检查 M3 抓到的）。散文不是属性。
      const before = RUST.slice(0, at).split("\n");
      const attrLines: string[] = [];
      for (let i = before.length - 2; i >= 0; i--) {
        const line = before[i].trim();
        if (line.startsWith("#[")) attrLines.push(line);
        else if (line !== "") break; // 撞到 doc 注释/空行以外的东西就停
      }
      expect(attrLines.join("\n"), `${t} 的属性里缺 deny_unknown_fields`).toContain(
        "deny_unknown_fields",
      );
    }
  });

  // ★ 接缝判据：**生产真的切过去了**。没有它，「把 renderCliViaBackend 换回 tryRenderCli」
  // 会让所有夹具/单测照常全绿 —— 那正是这一轮唯一实质的改动，也是最容易被悄悄回退的一处。
  test("生产渲染路径调的是后端，不是 TS 的 tryRenderCli", () => {
    expect(RUN).toContain("await renderCliViaBackend(ctx, plan, probe)");
    expect(RUN).toContain("commands.render_ccm_launch");
    // TS 的 `tryRenderCli` 只许作为**类型**出现（`CliRenderResult`），不许再被调用。
    expect(/[^a-zA-Z]tryRenderCli\s*\(/.test(RUN)).toBe(false);
  });
});

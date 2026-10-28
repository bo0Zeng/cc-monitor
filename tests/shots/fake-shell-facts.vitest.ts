/**
 * 截图台架假壳那一口 `footprint_client_facts` 照壳的形状回：界面把它原样带进本机后端的 `footprint-report` / `data-report`
 * （`{client: …}`），本机后端照 `FootprintClient` 收；少一格必填的 ⇒ 后端拒（`contract: malformed request: missing field 'home'`），
 * 那一页的足迹 · 数据报告截出来是一句「请求格式不对」。
 *
 * 人群：`FootprintClient` 生成类型（`src/frontend/ui/generated/FootprintClient.ts`，后端 `footprint::FootprintClient` 导出）里的必填格。
 * 买不到：只查必填格在不在、是不是串；可选格（`path`）给什么不查。家目录要是沙箱里后端那一份（`real/pool.mjs` 里挂成 `/home/user`）。
 */
import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";
import { machineCommands } from "./fake/machine";
import { defaultWorld } from "./fake/world";
import { nonEmpty } from "../test-support/non-empty";

const generated = path.resolve(__dirname, "../../src/frontend/ui/generated/FootprintClient.ts");

/** 生成类型 `{ a: string, b?: … }` 里不带 `?` 的那几格。 */
function requiredKeys(): string[] {
  const body = /export type FootprintClient = \{([^}]*)\}/.exec(readFileSync(generated, "utf8"))?.[1] ?? "";
  return [...body.matchAll(/(\w+)(\??):/g)].filter((m) => m[2] === "").map((m) => m[1]);
}

describe("假壳的 footprint_client_facts", () => {
  it("必填格一格不少，家目录是沙箱里后端的家", async () => {
    const keys = nonEmpty(requiredKeys());
    const got = (await machineCommands().footprint_client_facts({}, defaultWorld())) as Record<string, unknown>;
    for (const k of keys) expect(typeof got[k], `缺 ${k}`).toBe("string");
    expect(got.home).toBe("/home/user");
  });
});

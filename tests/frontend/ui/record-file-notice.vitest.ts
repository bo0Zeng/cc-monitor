/**
 * 记录文件那一句话的**线上字面量**两端对拍。
 *
 * 要求：删了 / 改名 ⇒ 出声；被截短 ⇒ 按截断重读
 * 「原地整份改写 …… 从 0 重读并出声（与截短同一句话族）」。
 *
 * 界面不另抄一份取值表：格里的 `change` 就是生成类型 `SessionFileNoticePayload.change`，界面只在两处拿它比字面量
 * （`record-file-notice.ts` 定「不见了」那句什么时候收 · `tabs.ts::onRecordFileReread` 定要不要整份重来）。
 * 那两处比的每个字面量都得是 monitor 交得出来的（`src/frontend/shell/src/stream_source/local.rs::FileChange::as_wire` 源码里现抠，异源），
 * 且三臂都有人比 —— 两向。那一句不在前端：后端写好随格带来（`SessionFileNoticePayload.said`），这里不照取值映文案。
 * 正控：两个抠取器在合成语料上各认得出。
 */
import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { nonEmpty } from "../../test-support/non-empty";

/** `fn as_wire(self) -> &'static str { match self { FileChange::X => "x", … } }` 里的字面量。 */
function asWireLiterals(src: string): string[] {
  const at = src.indexOf("pub fn as_wire(self) -> &'static str {");
  if (at < 0) return [];
  const body = src.slice(at, src.indexOf("\n    }\n", at));
  return [...body.matchAll(/FileChange::\w+ => "([a-z_]+)"/g)].map((m) => m[1]).sort();
}

/** 源码里 `change === "x"` / `change !== "x"` 比的字面量。 */
function comparedLiterals(src: string): string[] {
  return [...src.matchAll(/\bchange [!=]== "([a-z_]+)"/g)].map((m) => m[1]);
}

const ui = (f: string): string => readFileSync(join(__dirname, "../../../src/frontend/ui", f), "utf8");

describe("〔FW1〕记录文件出声的线上字面量", () => {
  it("抠取器认得出合成语料（正控）", () => {
    const fake = `impl FileChange {\n    pub fn as_wire(self) -> &'static str {\n        match self {\n            FileChange::A => "a",\n            FileChange::B => "b_c",\n            FileChange::C => "c",\n        }\n    }\n}\n`;
    expect(asWireLiterals(fake)).toEqual(["a", "b_c", "c"]);
    expect(comparedLiterals(`if (change === "a") x(); if (change !== "b_c" && change !== "c") return;`)).toEqual(["a", "b_c", "c"]);
  });

  it("界面比的取值 == monitor 交的（两向）", () => {
    const wire = asWireLiterals(readFileSync(join(__dirname, "../../../src/frontend/shell/src/stream_source/local.rs"), "utf8"));
    nonEmpty(wire);
    const notice = comparedLiterals(ui("record-file-notice.ts"));
    const reread = comparedLiterals(ui("tabs.ts"));
    expect(notice, "record-file-notice.ts 只认「不见了」那一种").toEqual(["gone"]);
    expect([...new Set([...notice, ...reread])].sort()).toEqual(wire);
  });

  it("record-file-notice.ts 不另抄取值表（格的取值是生成类型那一格）", () => {
    expect(ui("record-file-notice.ts")).not.toMatch(/\[\s*"gone",\s*"truncated",\s*"rewritten"\s*\]/);
  });
});

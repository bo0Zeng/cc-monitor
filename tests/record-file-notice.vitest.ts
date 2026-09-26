/**
 * 〔FW1 · 第四波 4D · 主会话裁 D-d〕记录文件那一句话的**线上字面量**两端对拍。
 *
 * 要求住址：题面 `4d-lanes.md`「主会话本批裁的」D-d（删了 / 改名 ⇒ 出声；被截短 ⇒ 按截断重读）＋ 09-25 补
 * 「原地整份改写 …… 从 0 重读并出声（与截短同一句话族）」。
 *
 * 前端认的三个取值（`record-file-notice.ts::RECORD_FILE_CHANGES`）== monitor 交出来的那三个
 * （`src/bridge/src/ssh_source.rs::FileChange::as_wire` 源码里现抠，异源）—— 两向相等；再各有一句文案（表里现取）。
 * 正控：抠取器在合成语料上认得出三臂。
 */
import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { RECORD_FILE_CHANGES } from "../src/record-file-notice";
import { copyText } from "../src/copy-table";

/** `fn as_wire(self) -> &'static str { match self { FileChange::X => "x", … } }` 里的字面量。 */
function asWireLiterals(src: string): string[] {
  const at = src.indexOf("pub fn as_wire(self) -> &'static str {");
  if (at < 0) return [];
  const body = src.slice(at, src.indexOf("\n    }\n", at));
  return [...body.matchAll(/FileChange::\w+ => "([a-z_]+)"/g)].map((m) => m[1]).sort();
}

describe("〔FW1〕记录文件出声的线上字面量", () => {
  it("抠取器认得出合成语料里的三臂（正控）", () => {
    const fake = `impl FileChange {\n    pub fn as_wire(self) -> &'static str {\n        match self {\n            FileChange::A => "a",\n            FileChange::B => "b_c",\n            FileChange::C => "c",\n        }\n    }\n}\n`;
    expect(asWireLiterals(fake)).toEqual(["a", "b_c", "c"]);
  });

  it("前端认的 == monitor 交的（两向），每一个都有一句文案", () => {
    const src = readFileSync(join(__dirname, "../src/bridge/src/ssh_source.rs"), "utf8");
    const wire = asWireLiterals(src);
    expect(wire.length, "monitor 那一侧一个都没抠到 —— 抠取器坏了").toBeGreaterThan(0);
    expect([...RECORD_FILE_CHANGES].sort()).toEqual(wire);
    for (const c of RECORD_FILE_CHANGES) {
      expect(copyText(`sessionState.recordFile.${c}` as never).length).toBeGreaterThan(0);
    }
  });
});

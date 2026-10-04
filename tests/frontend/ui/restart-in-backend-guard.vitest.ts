/**
 * 换号重启整条在会话所在那台的后端做完：界面不送压缩那一句、不按记录判「压缩完了没」。
 *
 * - 「请求压缩用哪一句」是适配层的一格（后端 `agents/<家>/`）；界面生产段零处 `"/compact"` 字面量。
 * - 「压缩完了没」由那台盯记录判；界面生产段零处读流上那一行去等它（旧的等待者 / 判定名一个都不在）。
 * 正控：同一套扫描在后端适配层恰好找到那一句（扫描没瞎）；合成串认得出那几个名字。
 */
import { describe, it, expect } from "vitest";
import { productionRsFiles, productionTsFiles, SCAN_TIMEOUT_MS } from "../../test-support/production-sources.ts";
import { stripComments } from "../../test-support/strip-comments.ts";

/** 一个带引号的 `/compact` 字面量（单 / 双 / 反引号）。 */
const COMPACT_LITERAL = /(["'`])\/compact\1/;
/** 界面那一侧等压缩摘要的那几个名字（旧编排里的等待者与判定）。 */
const WAITERS = /\b(isCompactRecord|compactWaiters|settleCompact|awaitCompactFor|hasCompactWaiters)\b/;

describe("换号重启不在界面编排", () => {
  it(
    "界面零处 \"/compact\" 字面量；后端适配层恰好一处（正控）",
    () => {
      const ui = productionTsFiles("src/frontend")
        .filter((f) => COMPACT_LITERAL.test(stripComments(f.text, "ts")))
        .map((f) => f.file);
      expect(ui).toEqual([]);
      const backend = productionRsFiles("src/backend")
        .filter((f) => COMPACT_LITERAL.test(f.text))
        .map((f) => f.file);
      expect(backend).toEqual(["src/backend/agents/claudecode/mod.rs"]);
    },
    SCAN_TIMEOUT_MS,
  );

  it(
    "界面零处按记录判「压缩完了没」（正控：合成串认得出）",
    () => {
      const hits = productionTsFiles("src")
        .filter((f) => WAITERS.test(stripComments(f.text, "ts")))
        .map((f) => f.file);
      expect(hits).toEqual([]);
      expect(WAITERS.test("this.actions.settleCompact(sid, () => isCompactRecord(m))")).toBe(true);
      expect(COMPACT_LITERAL.test('sendKeys(o, n, "/compact")')).toBe(true);
    },
    SCAN_TIMEOUT_MS,
  );
});

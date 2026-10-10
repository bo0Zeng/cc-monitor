/**
 * 界面里认着某一家记录格式的地方 —— **欠账表**（只许缩短；今天已清零）。
 *
 * 规矩：会话是哪一家、那一家的记录长什么样，都是后端的事（加一家 agent 只改后端 `agents/<名>/`）；界面只读后端给的成品
 * （通用记录 `LineRecord`：`t` 分类、`autoReply` · `endsTurn` · `error` 都是后端判好的；主线外清单是后端给的）。
 * 记录帧换形（10-09）之后界面不再认 Claude 盘上的格：自动应答的 `<synthetic>` · 链上的 `parentUuid` · 一轮结束的 `stop_reason`
 * · 报错标记 `isApiErrorMessage` · 排队记录 `queue-operation` · 标题记录 `ai-title` —— 全进 ZERO，一处都不许回来。
 * 盘上命中 == 登记（两向）；多一处 ⇒ 红（不许新长）；少了 ⇒ 红（修掉了就把登记减掉）。
 * 「当前那一家」（`ACTIVE_AGENT` / 单家 `AGENT_PROFILE`）与按会话文件名猜家（`rollout-`）同样清零。
 */
import { describe, it, expect } from "vitest";
import { stripComments } from "../../test-support/strip-comments";
import { productionTsFiles, SCAN_TIMEOUT_MS } from "../../test-support/production-sources";

/** `[针, 文件, 处数, 解锁条件]`。今天一行都没有。 */
const DEBT: ReadonlyArray<readonly [needle: string, file: string, n: number, unlock: string]> = [];

/** 清零了的：一处都不许回来。 */
const ZERO: readonly RegExp[] = [
  /\bACTIVE_AGENT\b/,
  /\bAGENT_PROFILE\b(?!_)/,
  /rollout-/,
  /<synthetic>/,
  /\bparentUuid\b/,
  /\bstop_reason\b/,
  /\bisApiErrorMessage\b/,
  /["'`]queue-operation["'`]/,
  /["'`]ai-title["'`]/,
];

const NEEDLES: ReadonlyArray<readonly [string, RegExp]> = [
  ["<synthetic>", /<synthetic>/g],
  ["parentUuid", /\bparentUuid\b/g],
];

describe("界面里认着某一家记录格式的地方：只许缩短", () => {
  it(
    "★ 盘上命中 == 欠账表（两向）；清零了的零命中",
    () => {
      const files = productionTsFiles("src/frontend/ui").filter((f) => !f.file.includes("/generated/"));
      expect(files.length, "扫描面塌了").toBeGreaterThan(100);
      const got: string[] = [];
      const zero: string[] = [];
      for (const { file, text } of files) {
        const code = stripComments(text, "ts");
        for (const [name, re] of NEEDLES) {
          const n = code.match(re)?.length ?? 0;
          if (n > 0) got.push(`${name} ${file} ${n}`);
        }
        for (const re of ZERO) if (re.test(code)) zero.push(`${re.source} ${file}`);
      }
      expect(got.sort(), "欠账变了：多了 ⇒ 不许新长（搬进后端）；少了 ⇒ 好事，把这一行减掉").toEqual(
        DEBT.map(([needle, file, n]) => `${needle} ${file} ${n}`).sort(),
      );
      expect(zero, "清零了的又回来了").toEqual([]);
      for (const [, , , unlock] of DEBT) expect(unlock, "每一行都要写清什么时候解").not.toBe("");
    },
    SCAN_TIMEOUT_MS,
  );

  it("正控：针认得出、剥注释之后注释里的不算", () => {
    const code = stripComments('// <synthetic> 说明\nconst x = "<synthetic>"; const p = r.parentUuid;', "ts");
    expect(code.match(/<synthetic>/g)?.length).toBe(1);
    expect(code.match(/\bparentUuid\b/g)?.length).toBe(1);
  });
});

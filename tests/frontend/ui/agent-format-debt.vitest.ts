/**
 * 界面里认着某一家记录格式的地方：零处。
 *
 * 规矩：会话是哪一家、那一家的记录长什么样，都是后端的事（加一家 agent 只改后端 `agents/<名>/`）；界面只读后端给的成品
 * （通用记录 `LineRecord`：`t` 分类、`autoReply` · `error` 都是后端判好的；主线外清单是后端给的）。
 * 记录帧换形（10-09）之后界面不再认 Claude 盘上的格：自动应答的 `<synthetic>` · 链上的 `parentUuid` · 一轮结束的 `stop_reason`
 * · 报错标记 `isApiErrorMessage` · 排队记录 `queue-operation` · 标题记录 `ai-title` —— 全进 ZERO，一处都不许回来。
 * 「当前那一家」（`ACTIVE_AGENT` / 单家 `AGENT_PROFILE`）与按会话文件名猜家（`rollout-`）同样清零。
 */
import { describe, it, expect } from "vitest";
import { stripComments } from "../../test-support/strip-comments";
import { productionTsFiles } from "../../test-support/production-sources";

/** 一处都不许有。 */
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

describe("界面里认着某一家记录格式的地方：零处", () => {
  it(
    "★ 那几根针在界面生产代码里零命中",
    () => {
      const files = productionTsFiles("src/frontend/ui").filter((f) => !f.file.includes("/generated/"));
      expect(files.length, "扫描面塌了").toBeGreaterThan(100);
      const zero: string[] = [];
      for (const { file, text } of files) {
        const code = stripComments(text, "ts");
        for (const re of ZERO) if (re.test(code)) zero.push(`${re.source} ${file}`);
      }
      expect(zero, "界面又认起某一家的记录格式了 ⇒ 搬进后端").toEqual([]);
    },
  );

  it("正控：针认得出、剥注释之后注释里的不算", () => {
    const code = stripComments('// <synthetic> 说明\nconst x = "<synthetic>"; const p = r.parentUuid;', "ts");
    expect(code.match(/<synthetic>/g)?.length).toBe(1);
    expect(code.match(/\bparentUuid\b/g)?.length).toBe(1);
  });
});

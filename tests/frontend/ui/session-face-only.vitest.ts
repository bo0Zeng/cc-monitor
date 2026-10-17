// 会话此刻是什么样、能做什么只由那台后端判（整份脸 `SessionFace`）：界面生产 TS 里没有会话状态转移（转移表 · 落值口 · 两轴的格）。
import { describe, it, expect } from "vitest";
import { productionTsFiles } from "../../test-support/production-sources.ts";
import { stripComments } from "../../test-support/strip-comments";

/** 会话状态转移的那几个名字（整词）：转移表 · 落值口 · 事件 · 两轴。 */
const TRANSITION = /\b(?:nextState|applyState|StateEvent|liveness|recoverability)\b/g;

const hits = (code: string): string[] => code.match(TRANSITION) ?? [];

describe("界面不攒会话状态", () => {
  it("★ 生产 TS 里转移表 · 落值口 · 两轴的名字零命中", () => {
    const found = productionTsFiles("src")
      .filter((s) => !s.file.includes("/generated/"))
      .flatMap((s) => hits(stripComments(s.text, "ts")).map((h) => `${s.file}: ${h}`));
    expect(found).toEqual([]);
  });

  it("正控：同一读法认得出一处转移（注释里的不算）", () => {
    expect(hits(stripComments("const next = nextState(tab.state, ev); // liveness", "ts"))).toEqual(["nextState"]);
  });
});

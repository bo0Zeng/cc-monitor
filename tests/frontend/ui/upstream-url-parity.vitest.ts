/**
 * 上游 base URL 能不能用：界面读的生成物（`baseUrlIssue`）与共用金样逐条对。
 *
 * 要求：「凡是有对应 `*-core` crate 的判定，TS 侧零实现」—— 规则住 `upstream_url_core::usable`，
 * monitor 现生成式子进 `src/frontend/ui/generated/judgment-rules.ts`；Rust 那一侧 `payload_judgment_rules.rs::the_upstream_url_golden_agrees_with_the_one_rule`
 * 读同一份金样 ⇒ 两侧各对金样，不是彼此对拍。
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { describe, expect, it } from "vitest";

import { baseUrlIssue } from "../../../src/frontend/ui/generated/judgment-rules";
import { checkBaseUrl } from "../../../src/frontend/ui/settings/account-new-form";
import { REPO_ROOT } from "../../test-support/repo-root.ts";

const golden = JSON.parse(readFileSync(resolve(REPO_ROOT, "tests/__fixtures__/upstream-url.golden.json"), "utf8")) as {
  cases: { url: string; want: string | null }[];
};

describe("J9 上游 base URL：生成物 == 共用金样", () => {
  it("每条：baseUrlIssue 给的理由 == 手写的 want（null = 能用）", () => {
    expect(golden.cases.length, "金样读空了").toBeGreaterThan(40);
    const wrong = golden.cases
      .map((c) => ({ c, got: baseUrlIssue(c.url) }))
      .filter(({ c, got }) => got !== c.want)
      .map(({ c, got }) => `${JSON.stringify(c.url)}：得 ${got}，金样 ${c.want}`);
    expect(wrong).toEqual([]);
  });

  it("表单薄壳：空 = 默认上游；能用的原样收（trim 过）；用不了的给一句", () => {
    expect(checkBaseUrl("  ")).toEqual({ ok: true, value: undefined });
    expect(checkBaseUrl(" http://[::1]:9 ")).toEqual({ ok: true, value: "http://[::1]:9" });
    expect(checkBaseUrl("http://api.example.com").ok).toBe(false);
  });
});

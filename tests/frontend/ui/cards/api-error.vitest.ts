/**
 * 报错卡与重试细条不自己映原因、不自己拼字：模块里不留原因一词的映射、不留并条时拼字的那一处。DOM 那几条在 `retry-bar.vitest.ts`。
 * 报错卡「下一步」那一格只有一条路在写：会话事实里没接上的那一串的整句（`applyRetries`）；卡落进消息流那一下照手上那一份事实摆。
 */
import { test, expect } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import * as cards from "../../../../src/frontend/ui/cards/api-error.ts";
import { REPO_ROOT } from "../../../test-support/repo-root.ts";

test("界面不留原因一词的映射、不留并条时自己拼字的那一处", () => {
  expect(Object.keys(cards)).not.toContain("mergeRetry");
  expect(Object.keys(cards)).not.toContain("reasonWord");
});

test("★ 事实先到、卡后画：卡落进消息流那一下照手上那一份事实摆（批量建卡 · 实时一条都摆）", () => {
  const src = readFileSync(resolve(REPO_ROOT, "src/frontend/ui/tab-stream-view.ts"), "utf8");
  const batch = src.slice(src.indexOf("tab.stream.batchInsert("));
  expect(batch.slice(0, batch.indexOf("\n  }\n")), "批量建卡之后").toContain("applyRetries(tab.streamEl, tab.retries)");
  const live = src.slice(src.indexOf("renderContentRecord(payload, ctx, sink);"));
  expect(live.slice(0, live.indexOf("\n  }\n")), "实时那一条落下之后").toContain("applyRetries(tab.streamEl, tab.retries)");
});

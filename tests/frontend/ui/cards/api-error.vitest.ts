/**
 * 报错卡与重试细条不自己映原因、不自己拼字：模块里不留原因一词的映射、不留并条时拼字的那一处。DOM 那几条在 `retry-bar.vitest.ts`。
 */
import { test, expect } from "vitest";
import * as cards from "../../../../src/frontend/ui/cards/api-error.ts";

test("界面不留原因一词的映射、不留并条时自己拼字的那一处", () => {
  expect(Object.keys(cards)).not.toContain("mergeRetry");
  expect(Object.keys(cards)).not.toContain("reasonWord");
});

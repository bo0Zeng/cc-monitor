/**
 * 报错卡的原因一词按后端给的种类出（界面不读报错对象）。DOM 那几条（并条 · 结局）在 `main-window-behavior.vitest.ts`。
 */
import { test, expect } from "vitest";
import { reasonWord } from "../../../../src/frontend/ui/cards/api-reason.ts";
import { copyText } from "../../../../src/frontend/ui/copy-table.ts";

test("六种原因各出一词、互不相同（后端给的种类，界面不读报错对象）", () => {
  const words = (["overloaded", "quota", "network", "auth", "context", "unknown"] as const).map((r) => reasonWord(r));
  expect(new Set(words).size, "六个词互不相同").toBe(6);
  expect(words.join(" / ")).toBe("服务器过载 / 额度满 / 网络中断 / 需登录 / 上下文超长 / 原因不明");
});

test("后端没给原因（老后端）⇒ 原因不明，不猜", () => {
  expect(reasonWord(undefined)).toBe(copyText("reason.io.unknown"));
});

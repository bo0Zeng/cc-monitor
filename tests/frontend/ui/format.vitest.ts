/**
 * `sizeText`（大小的那一个读口）的确定性边界；整份对拍在 `tests/copy/size-text.vitest.ts`，读金样 `size-text.golden.json`。
 *
 * 为什么值得锁：大小的写法只有这一个读口与核心一份（界面 · 文件窗口 · 卡片各写一份时，GB 位数与带不带 TB 各不相同）。
 */
import { test, expect } from "vitest";
import { sizeText } from "../../../src/frontend/ui/format.ts";

test("sizeText: B 段（< 1024）", () => {
  expect(sizeText(0)).toBe("0 B");
  expect(sizeText(1)).toBe("1 B");
  expect(sizeText(1023)).toBe("1023 B");
});

test("sizeText: KB 段（1 位小数）", () => {
  expect(sizeText(1024)).toBe("1.0 KB");
  expect(sizeText(1536)).toBe("1.5 KB");
  expect(sizeText(1024 * 1024 - 1), "舍入到 1024.0 的进一档").toBe("1.0 MB");
});

test("sizeText: MB · GB 段（都是 1 位小数）", () => {
  expect(sizeText(1024 * 1024)).toBe("1.0 MB");
  expect(sizeText(5 * 1024 * 1024)).toBe("5.0 MB");
  expect(sizeText(1024 * 1024 * 1024)).toBe("1.0 GB");
  expect(sizeText(Math.round(2.5 * 1024 * 1024 * 1024))).toBe("2.5 GB");
});

test("sizeText: 负数当 0", () => {
  expect(sizeText(-5)).toBe("0 B");
});

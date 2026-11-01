/**
 * **「在跑」只读一格**：会话在不在跑只看后端写好的那一格 —— 会话脸的 `life`（`cmd-wire` 的 `Life`）。
 *
 * 界面那一半由 eslint 守（`eslint.config.js` 的 `singleDoors`：`life` 只在 `face-can.ts` 读 · 不拿状态点推说不清；
 * `single-home-lint.vitest.ts` 证会抓）。本文件只管手机那一半：Kotlin 生产码不读 `life`（照成品画）。
 *
 * 买不到：换个写法自己判（比如照 `dot` 列一串在跑的点）这里看不见；只抓 `.life` 那一格被读的字面。
 */
import { describe, expect, it } from "vitest";

import { productionKtFiles } from "../../test-support/production-sources.ts";
import { stripComments } from "../../test-support/strip-comments.ts";
import { nonEmpty } from "../../test-support/non-empty.ts";

const kt = nonEmpty(
  productionKtFiles()
    .filter((f) => !f.file.includes("/gen/"))
    .map((f) => ({ file: f.file, code: stripComments(f.text, "ts") })),
);

const hits = (files: { file: string; code: string }[], re: RegExp): string[] =>
  files.flatMap((f) => f.code.split("\n").flatMap((l, i) => (re.test(l) ? [`${f.file}:${i + 1}: ${l.trim()}`] : [])));

describe("「在跑」只读一格（手机）", () => {
  it("Kotlin 生产码不读会话脸的 `life`", () => {
    expect(hits(kt, /\.life\b/)).toEqual([]);
  });

  it("探测器认得出 `.life` 被读、注释里写的不算", () => {
    expect(hits([{ file: "x.kt", code: stripComments("val a = face.life.isLive\n// face.life", "ts") }], /\.life\b/)).toEqual([
      "x.kt:1: val a = face.life.isLive",
    ]);
  });
});

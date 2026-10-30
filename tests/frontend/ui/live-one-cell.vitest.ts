/**
 * **「在跑」只读一格**：会话在不在跑只看后端写好的那一格 —— 会话脸的 `life`（`cmd-wire` 的 `Life`）。
 *
 * 界面里读 `life` 的只有 `face-can.ts` 那几个短名（`isLive` · `isUnclear` · `activityOf`），别处只调它们；
 * 不拿状态点（`dot === "unknown"`）去推说不清；手机的 Kotlin 生产码不读 `life`（照成品画）。
 *
 * 买不到：换个写法自己判（比如照 `dot` 列一串在跑的点）这里看不见；只抓 `life` 那一格被读的几种字面。
 */
import { describe, expect, it } from "vitest";

import { productionKtFiles, productionTsFiles } from "../../test-support/production-sources.ts";
import { stripComments } from "../../test-support/strip-comments.ts";
import { nonEmpty } from "../../test-support/non-empty.ts";

const HOME = "src/frontend/ui/face-can.ts";

const ts = nonEmpty(productionTsFiles("src/frontend").map((f) => ({ file: f.file, code: stripComments(f.text, "ts") })));
const kt = nonEmpty(
  productionKtFiles()
    .filter((f) => !f.file.includes("/gen/"))
    .map((f) => ({ file: f.file, code: stripComments(f.text, "ts") })),
);

const hits = (files: { file: string; code: string }[], re: RegExp): string[] =>
  files.flatMap((f) => f.code.split("\n").flatMap((l, i) => (re.test(l) ? [`${f.file}:${i + 1}: ${l.trim()}`] : [])));

describe("「在跑」只读一格", () => {
  it("界面里读会话一生（`life.is`）的只有 face-can.ts", () => {
    expect(hits(ts.filter((f) => f.file !== HOME), /\blife\??\.is\b/)).toEqual([]);
  });

  it("不拿状态点推在不在跑 / 说不清", () => {
    expect(hits(ts, /\bdot\s*===\s*"unknown"\s*\?/)).toEqual([]);
  });

  it("face-can.ts 恰有那三个读口，各读 `life` 一次（探测器的阳性对照）", () => {
    const home = ts.find((f) => f.file === HOME);
    expect(home, HOME).toBeDefined();
    for (const name of ["isLive", "isUnclear", "activityOf"]) expect(home!.code, name).toMatch(new RegExp(`export const ${name}\\b`));
    expect(hits([home!], /\blife\??\.is\b/).length).toBe(3);
  });

  it("手机：Kotlin 生产码不读会话脸的 `life`", () => {
    expect(hits(kt, /\.life\b/)).toEqual([]);
  });
});

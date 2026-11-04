/**
 * 设置那一族场景只按条件等：等它要的那个元素出现、那个状态到，找不着就 ✗（这一张算没截成）。
 *
 * 为什么有这一格：按钟点等（`sleep`）的那一张在机器忙时截到的是别页、是「读取中」；`querySelector(…)?.click()` 找不着就放过，
 * 截下来的是没点开的那一页，台架照样 ✓。折叠头按正文找（`byText(button, 标题)`）：头里一带摘要，就点到别的按钮或找不着。
 * 三样都让「要等的元素改了名」这件事截成一张错图还报 ✓。
 *
 * 人群：`scenes/settings.ts`（设置窗那一族，`SETTINGS_SCENES`）。
 * 判法（剥了注释的源码）：没有 `sleep(` · 没有挂在 DOM 查找上的 `?.` · 折叠头（「这台上的 cc-monitor」·「连接设置」）不按正文找。
 * 买不到：场景等的是不是「对的」那个元素（那要真开页：`run.mjs --only <场景>`）；`go()` 自己等页露出来、页里没有在读的骨架，
 *   它怎么等住在 `scenes/settings.ts::shown`。
 */
import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";
import { stripComments } from "../test-support/strip-comments";
import { copyText } from "../../src/frontend/ui/copy-table";

const FILE = path.resolve(__dirname, "scenes/settings.ts");

function lines(): { n: number; text: string }[] {
  return stripComments(readFileSync(FILE, "utf8"), "ts")
    .split("\n")
    .map((text, i) => ({ n: i + 1, text }));
}

const hits = (re: RegExp): string[] => lines().filter((l) => re.test(l.text)).map((l) => `settings.ts:${l.n}  ${l.text.trim()}`);

describe("设置那一族场景只按条件等", () => {
  it("不按钟点等（没有 sleep）", () => {
    expect(hits(/\bsleep\s*\(/)).toEqual([]);
  });

  it("DOM 查找找不着不放过（查找上不挂 ?.）", () => {
    // `querySelector(…)?.x` · `querySelectorAll(…)[0]?.x` · `[…].find(…)?.x` · `.filter(…)[0]?.x`
    expect(hits(/querySelector(All)?\s*(<[^>]*>)?\s*\(.*\)\s*(\[[^\]]*\])?\s*\?\./)).toEqual([]);
    expect(hits(/\.(find|filter)\s*\(.*\)\s*(\[[^\]]*\])?\s*\?\./)).toEqual([]);
  });

  it("折叠头按结构找，不按正文", () => {
    // 按文案键写的，与把那一句原文直接写进去的，两样都算。
    const said = [copyText("machinePage.cc.title"), copyText("machinePage.conn.toggle")].map((t) => `"${t.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}"`);
    expect(hits(new RegExp(`byText\\s*\\(.*(machinePage\\.cc\\.title|machinePage\\.conn\\.toggle|${said.join("|")})`))).toEqual([]);
  });
});

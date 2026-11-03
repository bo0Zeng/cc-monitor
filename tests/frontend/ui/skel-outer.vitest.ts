/**
 * 骨架外框：第一级粗估的外框常数（`height-estimate.ts` 的 `SKEL_*`）对拍真浏览器。
 *
 * 金样由真 Chromium 打（`tests/evidence/skel-run.ts`；读的是卡在流里占的位置 − 头 − 体），语料是 `scale2-height-corpus.ts` 那一份。
 * ⚠ 买不到：WebKitGTK 那一个引擎没打；卡间距按语料里的邻居折叠，真会话里邻居不同会差几 px。
 */
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { SKEL_OUTER } from "../../../src/frontend/ui/height-estimate";
import { nonEmpty } from "../../test-support/non-empty";

describe("骨架外框", () => {
  it("★ 四格 `SKEL_*` == 真浏览器里「位置 − 体」的中位数向上取整（user · assistant 含头 · 几步卡的外距 · 系统细条）", () => {
    const g = JSON.parse(readFileSync(resolve(__dirname, "../../evidence/skel-golden.json"), "utf8")) as {
      rows: Array<{ cls: string; slot: number; body: number }>;
    };
    const median = (xs: number[]): number => {
      const s = [...xs].sort((a, b) => a - b);
      const m = s.length >> 1;
      return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2;
    };
    const of = (cls: string): number => {
      const v = g.rows.filter((r) => r.cls === cls).map((r) => r.slot - r.body);
      nonEmpty(v);
      return Math.ceil(median(v));
    };
    expect({ ...SKEL_OUTER }).toEqual({
      user: of("card-user"),
      assistant: of("card-assistant"),
      steps: of("card-steps"),
      system: of("card-api-retry"),
    });
  });
});

// @vitest-environment node
/**
 * 〔GAP1 · `设计/01 §1.5`「一个 store，一个 router」〕overlay 视图（历史 / 全景 / 网格 / 收件箱 / 驾驶舱）的路由。
 * 守的要求（`设计/01 §1.5` 原文）：「overlay 视图的路由（历史 / 全景 / 网格 / 收件箱）还在 `main.ts`」⇒ 收进 router。
 *
 * ① 三个动作的语义（期望手写）：toggle 开着关 / 关着开 · open 只开不关 · show 照开一次；
 * ② `main.ts` 生产段零处自己判 `isVisible(`（带正控），登记的名字 == 五个（两向）。
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { OverlayRouter, type OverlayView } from "../src/overlay-router";
import { REPO_ROOT } from "./test-support/repo-root.ts";
import { stripComments } from "./test-support/strip-comments.ts";

function fake(): OverlayView & { log: string[]; shown: boolean } {
  const v = {
    log: [] as string[],
    shown: false,
    isVisible: () => v.shown,
    open: () => {
      v.log.push("open");
      v.shown = true;
    },
    close: () => {
      v.log.push("close");
      v.shown = false;
    },
  };
  return v;
}

describe("〔GAP1〕overlay 路由", () => {
  it("★ toggle 开着关、关着开；open 只开不关；show 不看开没开照开一次", async () => {
    const r = new OverlayRouter();
    const h = fake();
    r.register("history", h);
    r.toggle("history");
    r.toggle("history");
    r.open("history");
    r.open("history");
    await r.show("history");
    expect(h.log).toEqual(["open", "close", "open", "open"]);
    expect(() => r.register("history", fake())).toThrow();
    expect(() => r.toggle("grid")).toThrow();
  });

  it("★ main.ts 零处自己判 overlay 开没开；登记的恰是那五个", () => {
    const code = stripComments(readFileSync(resolve(REPO_ROOT, "src/main.ts"), "utf8"), "ts");
    const judged = (src: string): number => (src.match(/\.isVisible\(/g) ?? []).length;
    expect(judged(code), "main.ts 又自己判 overlay 开没开了 —— 走 overlays.toggle / open").toBe(0);
    expect(judged(`${code}\nif (historyView.isVisible()) x();`), "正控：量具认得出").toBe(1);
    const names = [...code.matchAll(/overlays\.register\("([^"]+)"/g)].map((m) => m[1]).sort();
    expect(names).toEqual(["cc-bus", "grid", "history", "inbox", "panorama"]);
  });
});

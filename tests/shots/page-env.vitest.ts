/**
 * 截图台架与性能台架开的每一页，时区与「现在」都给死；截图台架另按减少动画截。
 *
 * - 「现在」：每一页开头拨钟（`real/pool.mjs::pinDateSource`，经调试口在页里任何脚本之前装上），页里起世界时把那一刻交给各台后端
 *   （`CCM_SHOTS_NOW_MS`，后端那一侧判据 `tests/backend/one_clock_guard.rs`）。不拨 ⇒ 钟面 · 距今随跑的那一刻变，两趟截图逐像素对不上。
 * - 时区：浏览器的环境里 `TZ` 是 `SHOTS_TZ`（同各台后端）。
 * - 减少动画（只截图台架）：`prefers-reduced-motion: reduce`，界面那一侧什么都不动（判据 `tests/frontend/ui/reduced-motion.vitest.ts`），
 *   截下来的那一帧不落在动画半途。性能台架不开：它量的是用户默认看到的那一份。
 * - 家目录：假壳 `footprint_client_facts` 交的家就是各台后端沙箱里的家（`real/pool.mjs` 挂成的那一个）；界面把它原样带进本机后端的
 *   足迹 · 数据报告，对不上 ⇒ 报告里的路径与沙箱里的盘对不上。那一口的形状（必填格）由 tsc 守：返回值注解为 `FootprintClient`。
 *
 * 人群：`tests/shots/` 下每一份起无头浏览器的脚本（认 `"--headless=new"`）—— 新加一份台架也自动进来。
 * 买不到：读的是源码文本（脚本有顶层副作用，不能 import 起来真跑）；参数拼得更绕时认不出。
 */
import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, it, expect } from "vitest";
import { filePaths } from "../test-support/production-sources";
import { REPO_ROOT } from "../test-support/repo-root";
import { nonEmpty } from "../test-support/non-empty";
import { machineCommands } from "./fake/machine";
import { defaultWorld } from "./fake/world";

const root = path.resolve(__dirname);


const launchers = filePaths("tests/shots", { ext: ".mjs", tests: true, generated: true })
  .map((f) => path.resolve(REPO_ROOT, f))
  .filter((p) => readFileSync(p, "utf8").includes('"--headless=new"'));

/** 每一处 `Page.open(…)` 的整段实参（到配对的右括号）。 */
function opens(src: string): string[] {
  const out: string[] = [];
  for (let at = src.indexOf("Page.open("); at >= 0; at = src.indexOf("Page.open(", at + 1)) {
    let depth = 0;
    let i = at + "Page.open".length;
    for (; i < src.length; i++) {
      if (src[i] === "(") depth++;
      else if (src[i] === ")" && --depth === 0) break;
    }
    out.push(src.slice(at, i + 1));
  }
  return out;
}

describe("台架开的每一页：时区 · 现在给死，截图另按减少动画", () => {
  it("人群不空（截图台架 ＋ 性能台架）", () => {
    const rels = launchers.map((p) => path.relative(root, p).split(path.sep).join("/"));
    expect(rels).toContain("run.mjs");
    expect(rels).toContain("perf/bench.mjs");
  });

  it("页的那一侧真在拨：开页时装拨钟那一段、按要求开减少动画", () => {
    const cdp = readFileSync(path.join(root, "cdp.mjs"), "utf8");
    expect(cdp).toContain('"Page.addScriptToEvaluateOnNewDocument", { source: opts.boot }');
    expect(cdp).toMatch(/opts\.reducedMotion[\s\S]{0,200}"Emulation\.setEmulatedMedia"[\s\S]{0,120}name: "prefers-reduced-motion", value: "reduce"/);
    const pool = readFileSync(path.join(root, "real/pool.mjs"), "utf8");
    expect(pool, "后端从页交来的那一刻接着走").toContain("CCM_SHOTS_NOW_MS: String(Math.round(now))");
    expect(readFileSync(path.join(root, "fake/backend.ts"), "utf8"), "页里起世界时交那一刻").toContain("now: Date.now()");
  });

  it("假壳交的家目录就是各台后端沙箱里的家", async () => {
    const pool = readFileSync(path.join(root, "real/pool.mjs"), "utf8");
    const box = /const H = "([^"]+)";[\s\S]*?\bHOME: H,/.exec(pool)?.[1];
    expect(box, "real/pool.mjs 里认不出沙箱的家（`const H = \"…\"` 交给 `HOME: H`）").toBeTruthy();
    const facts = (await machineCommands().footprint_client_facts({}, defaultWorld())) as { home?: unknown };
    expect(facts.home).toBe(box);
  });

  for (const file of launchers) {
    const rel = path.relative(root, file).split(path.sep).join("/");
    const src = readFileSync(file, "utf8");
    const calls = opens(src);
    it(`${rel}：每一页开头拨钟`, () => {
      nonEmpty(calls);
      expect(calls.filter((c) => !c.includes("boot: pinDateSource(")), "Page.open 第五个实参带 { boot: pinDateSource() }").toEqual([]);
    });
    it(`${rel}：浏览器的时区是 SHOTS_TZ`, () => {
      expect(/TZ: SHOTS_TZ/.test(src), "起浏览器的环境里 TZ: SHOTS_TZ").toBe(true);
    });
    if (rel === "run.mjs") {
      it(`${rel}：每一页按减少动画截`, () => {
        expect(calls.filter((c) => !c.includes("reducedMotion: true"))).toEqual([]);
      });
    }
  }
});

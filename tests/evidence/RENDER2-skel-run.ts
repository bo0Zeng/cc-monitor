/**
 * 骨架外框秤的一键复算：打包探针 → Chromium（Playwright 的 headless shell）跑一遍 → 写金样。
 * 不进门禁（门禁读金样，`tests/frontend/ui/scale2-height-truth.vitest.ts` 末尾「骨架外框」那一组）。
 *
 * 用法（仓根）：`npx tsx tests/evidence/RENDER2-skel-run.ts`
 * 要一份 playwright：`npm i --prefix .scratch/pw playwright`（浏览器本体在 `~/.cache/ms-playwright`，秤 2 那份同用）。
 * 产物落 `.scratch/skel-probe/`（不进仓）；时区 / locale 与秤 2 同钉（语料带时间戳，字数跟着变）。
 */
import { build } from "vite";
import { createRequire } from "node:module";
import { writeFileSync, mkdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "../..");
const work = resolve(root, ".scratch/skel-probe");
process.env.TZ = "America/Los_Angeles";

async function run(): Promise<void> {
  mkdirSync(work, { recursive: true });
  await build({
    root,
    logLevel: "warn",
    configFile: false,
    build: {
      outDir: resolve(work, "dist"),
      emptyOutDir: true,
      cssCodeSplit: false,
      minify: false,
      lib: { entry: resolve(here, "RENDER2-skel-probe-entry.ts"), formats: ["iife"], name: "SkelProbe", fileName: () => "probe.js" },
    },
  });
  writeFileSync(
    resolve(work, "probe.html"),
    '<!doctype html><html lang="zh"><head><meta charset="utf-8"><link rel="stylesheet" href="./dist/cc-monitor.css">' +
      '<link rel="stylesheet" href="./dist/style.css"></head><body><script src="./dist/probe.js"></script></body></html>',
  );
  const req = createRequire(resolve(root, ".scratch/pw/package.json"));
  // playwright 装在 `.scratch/pw`（不进仓的依赖）⇒ 类型只写这一趟用到的那几个口，不 `import("playwright")`。
  type Page = {
    goto(url: string): Promise<unknown>;
    waitForFunction(fn: () => boolean, arg: null, opts: { timeout: number }): Promise<unknown>;
    evaluate<T>(fn: () => T): Promise<T>;
  };
  type Browser = { newPage(opts: Record<string, unknown>): Promise<Page>; close(): Promise<void> };
  const { chromium } = req("playwright") as { chromium: { launch(): Promise<Browser> } };
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: 1280, height: 900 }, locale: "zh-CN", timezoneId: "America/Los_Angeles" });
  await page.goto(pathToFileURL(resolve(work, "probe.html")).href); // 仓可能住非 ASCII 路径（`~/文档/`）⇒ 走编码过的 URL
  await page.waitForFunction(() => (window as unknown as { __DONE?: boolean }).__DONE === true, null, { timeout: 60_000 });
  const raw = await page.evaluate(() => (window as unknown as { __RESULT: string }).__RESULT);
  await browser.close();
  const out = { note: "RENDER2 骨架外框秤（RENDER2-skel-run.ts 生成，勿手改）", generatedAt: new Date().toISOString(), ...JSON.parse(raw) };
  writeFileSync(resolve(here, "RENDER2-skel-golden.json"), JSON.stringify(out, null, 2) + "\n");
  console.log(`写了 ${out.rows.length} 行 → tests/evidence/RENDER2-skel-golden.json`);
}

void run();

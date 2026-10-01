/**
 * 用户 09-29「readme全面搞一下 … 看看有没有什么截图 … 突出这个app是干嘛的」—— README 的产品截图，一键重拍。
 *
 * 真前端（vite 照仓里的 `vite.config.ts` 构建 index.html / settings.html）＋ 页里的替身后端（`README-shots-ipc.ts`，
 * 数据全是 `README-shots-fixture.ts` 里合成的）＋ Chromium（Playwright 的 headless shell）⇒ `docs/screenshots/*.png`。
 * 不起后端、不碰 `~/.claude` / `~/.cc-monitor` / tmux；1280×800、界面默认主题、钟停在夹具那一刻（时间戳每次拍都一样）。
 *
 * 用法（仓根）：`npx tsx tests/evidence/README-shots.ts [main history machines footprint]`（不给 = 全拍）。
 * 要一份 playwright：与 RENDER2 同住 `.scratch/pw`（本机 npx 缓存里已有的那份软链进 `.scratch/pw/node_modules/` 也行，不必再装）。
 * 构建产物落 `.scratch/readme-shots/`（不进仓）。页里抛错 / 状态栏报错 / 超过 300 KB ⇒ 这一张作废、非零退出。
 */
import { build } from "vite";
import { createRequire } from "node:module";
import { createServer } from "node:http";
import { existsSync, mkdirSync, readFileSync, statSync } from "node:fs";
import { dirname, extname, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { copyText } from "../../src/frontend/ui/copy-table";
import { ACTIVE_SID, MACHINES, NOW_ISO, REMOTE, SEARCH_WORD, SESSIONS } from "./README-shots-fixture";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "../..");
const work = resolve(root, ".scratch/readme-shots");
const outDir = resolve(root, "docs/screenshots");
const IPC = "/tests/evidence/README-shots-ipc.ts";
const MAX_BYTES = 300 * 1024;

// playwright 住 `.scratch/pw`（不进仓的依赖）⇒ 只写这一趟用到的那几个口，不 `import("playwright")`。
interface Locator {
  first(): Locator;
  click(): Promise<void>;
}
interface Page {
  goto(url: string): Promise<unknown>;
  click(selector: string): Promise<void>;
  fill(selector: string, value: string): Promise<void>;
  press(selector: string, key: string): Promise<void>;
  selectOption(selector: string, value: string): Promise<unknown>;
  locator(selector: string, opts?: { hasText?: RegExp }): Locator;
  waitForSelector(selector: string, opts?: { state?: "attached" | "visible" }): Promise<unknown>;
  waitForFunction<A>(fn: (arg: A) => boolean, arg: A, opts?: { timeout?: number }): Promise<unknown>;
  waitForTimeout(ms: number): Promise<void>;
  evaluate<T, A>(fn: (arg: A) => T, arg: A): Promise<T>;
  screenshot(opts: { path: string }): Promise<unknown>;
  on(event: "pageerror", fn: (e: Error) => void): void;
  clock: { setFixedTime(t: Date): Promise<void> };
}
interface Browser {
  newContext(opts: Record<string, unknown>): Promise<{ newPage(): Promise<Page>; close(): Promise<void> }>;
  close(): Promise<void>;
}

interface Scene {
  html: "index.html" | "settings.html";
  drive(page: Page): Promise<void>;
}

const exact = (text: string): RegExp => new RegExp(`^${text}$`);

/** 主角那一轮里有几张工具卡（全画出来了才动手）。 */
const toolCards = (SESSIONS.find((s) => s.sid === ACTIVE_SID)?.records ?? []).flatMap((r) =>
  r.type === "assistant" && Array.isArray(r.message.content) ? r.message.content.filter((b: { type?: string }) => b.type === "tool_use") : [],
).length;
/** 主窗口把主角那一轮画完了（四个 tab 的行都到了、工具卡齐了）。 */
const sessionShown = (page: Page): Promise<unknown> =>
  page.waitForFunction((n) => document.querySelectorAll(".stream.active details.block-tool-use").length === n, toolCards);

const SCENES: Record<string, Scene> = {
  /** 主界面：左边四个 tab（本机两个、devbox 两个），右边停在 api-server 那一轮，展开「改文件」那张卡看 diff。 */
  main: {
    html: "index.html",
    async drive(page) {
      await sessionShown(page);
      await page.evaluate(() => {
        for (const d of document.querySelectorAll<HTMLDetailsElement>(".stream.active details.block-tool-use"))
          if (d.querySelector("summary")?.textContent?.includes("Edit")) d.open = true;
      }, null);
      await page.waitForSelector(".stream.active .block-diff");
      // 从第一张卡看起（顶上留出右上角那排按钮的高度）；这一轮正好一屏
      await page.evaluate(() => {
        const st = document.querySelector<HTMLElement>(".stream.active")!;
        const first = st.querySelector<HTMLElement>(".stream-content > *")!;
        st.scrollTop += first.getBoundingClientRect().top - st.getBoundingClientRect().top - 44;
      }, null);
    },
  },
  /** 历史：全文搜一个词，两台机器的会话一起列出来，命中处高亮。 */
  history: {
    html: "index.html",
    async drive(page) {
      await sessionShown(page);
      await page.click(".history-trigger");
      await page.locator(".history-mode-btn", { hasText: exact(copyText("history.build.modeFulltext")) }).first().click();
      await page.fill(".history-search", SEARCH_WORD);
      await page.press(".history-search", "Enter");
      await page.waitForSelector(".search-hit mark");
    },
  },
  /** 设置 → 机器：本机与 devbox 都连上、五项都测过通过。 */
  machines: {
    html: "settings.html",
    async drive(page) {
      await page.waitForFunction((n) => document.querySelectorAll('.backend-row-state[data-on="true"]').length === n, MACHINES.length);
    },
  },
  /** 设置 → devbox → 足迹：cc-monitor 在那台机器上放了什么、现在在不在。 */
  footprint: {
    html: "settings.html",
    async drive(page) {
      await page.locator(".settings-nav button", { hasText: exact(REMOTE.label) }).first().click();
      await page.locator("button:visible", { hasText: exact(copyText("settingsPanel.machineTab.footprint")) }).first().click();
      await page.waitForSelector(".config-surface-tool");
    },
  },
};

/** 构建真前端：仓里那份 vite 配置（带 `vendor` 层插件）＋ 把替身后端注进每个 html 的最前面。 */
async function buildFrontend(dist: string): Promise<void> {
  await build({
    root,
    configFile: resolve(root, "vite.config.ts"),
    logLevel: "warn",
    base: "./",
    build: { outDir: dist, emptyOutDir: true },
    plugins: [
      {
        name: "readme-shots-ipc",
        transformIndexHtml: {
          order: "pre",
          handler: (html: string) => html.replace("<head>", `<head>\n    <script type="module" src="${IPC}"></script>`),
        },
      },
    ],
  });
}

const MIME: Record<string, string> = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript",
  ".css": "text/css",
  ".json": "application/json",
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".woff": "font/woff",
  ".woff2": "font/woff2",
  ".ttf": "font/ttf",
};

/** 模块脚本不认 `file://` ⇒ 起一个只读的本地静态服务（只监听回环、只给 dist 里的文件）。 */
function serve(dist: string): Promise<{ url: string; close(): void }> {
  const server = createServer((req, res) => {
    const p = resolve(dist, "." + decodeURIComponent((req.url ?? "/").split("?")[0]));
    if (!(p === dist || p.startsWith(dist + sep)) || !existsSync(p) || statSync(p).isDirectory()) {
      res.writeHead(404).end();
      return;
    }
    res.writeHead(200, { "content-type": MIME[extname(p)] ?? "application/octet-stream" }).end(readFileSync(p));
  });
  return new Promise((ok) =>
    server.listen(0, "127.0.0.1", () => ok({ url: `http://127.0.0.1:${(server.address() as { port: number }).port}`, close: () => server.close() })),
  );
}

async function shoot(browser: Browser, base: string, name: string, scene: Scene): Promise<string[]> {
  const ctx = await browser.newContext({ viewport: { width: 1280, height: 800 }, locale: "zh-CN", timezoneId: "Asia/Shanghai" });
  const page = await ctx.newPage();
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(`页里抛错：${e.message}`));
  await page.clock.setFixedTime(new Date(NOW_ISO));
  await page.goto(`${base}/${scene.html}`);
  await scene.drive(page);
  await page.evaluate(() => document.fonts.ready.then(() => undefined), null);
  await page.waitForTimeout(400); // 滚动 / 展开之后的那一帧
  const status = await page.evaluate(() => document.getElementById("status-bar")?.textContent ?? "", null);
  if (/^(ERR|REJ):/.test(status)) errors.push(`状态栏报错：${status}`);
  const unanswered = await page.evaluate(() => (window as unknown as { __SHOTS_UNANSWERED: string[] }).__SHOTS_UNANSWERED, null);
  const path = resolve(outDir, `${name}.png`);
  if (errors.length === 0) {
    await page.screenshot({ path });
    const bytes = statSync(path).size;
    if (bytes > MAX_BYTES) errors.push(`${name}.png ${Math.round(bytes / 1024)} KB，超过 300 KB`);
    else console.log(`${name}.png  ${Math.round(bytes / 1024)} KB`);
  }
  if (unanswered.length > 0) console.log(`  （${name}：替身没答、界面按旧后端处理的帧命令：${[...new Set(unanswered)].join("，")}）`);
  await ctx.close();
  return errors.map((e) => `[${name}] ${e}`);
}

async function run(): Promise<void> {
  const want = process.argv.slice(2);
  const unknown = want.filter((n) => !(n in SCENES));
  if (unknown.length > 0) throw new Error(`不认得的场景：${unknown.join(" ")}（有：${Object.keys(SCENES).join(" ")}）`);
  const pw = resolve(root, ".scratch/pw/package.json");
  if (!existsSync(pw)) throw new Error("没有 .scratch/pw：照头注备一份 playwright（软链现成的那份即可）");
  mkdirSync(work, { recursive: true });
  mkdirSync(outDir, { recursive: true });
  const dist = resolve(work, "dist");
  await buildFrontend(dist);
  const { chromium } = createRequire(pw)("playwright") as { chromium: { launch(): Promise<Browser> } };
  const server = await serve(dist);
  const browser = await chromium.launch();
  const failed: string[] = [];
  try {
    for (const name of want.length > 0 ? want : Object.keys(SCENES)) failed.push(...(await shoot(browser, server.url, name, SCENES[name])));
  } finally {
    await browser.close();
    server.close();
  }
  if (failed.length > 0) {
    console.error(failed.join("\n"));
    process.exitCode = 1;
  }
}

void run().catch((e: unknown) => {
  console.error(e);
  process.exitCode = 1;
});

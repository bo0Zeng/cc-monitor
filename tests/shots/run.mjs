/**
 * 截图工具：把 cc-monitor 的每个界面、每种状态真画出来截图，出一页图集（`index.html`）。
 *
 *   npm run shots -- [--out <目录>] [--only <正则>] [--no-filewin] [--no-web]
 *
 * - 网页那三扇窗：仓里那份 vite ＋ 无头 Chromium ＋ 页里的假后端（`fake/`），场景见 `scenes/`。
 * - 文件窗口（egui）：私有 Xvfb 上起那一格截图测试（`tests/frontend/filewin/workspace_tests.rs` 末尾），
 *   一张图一个进程（winit 一个进程只许建一个事件循环）。
 * - 隔离：浏览器与 vite 的 HOME 都在 `.build/shots-sandbox/` 里；不起后端、不起 claude、不碰 tmux、不连任何机器。
 *
 * 浏览器：环境变量 `CCM_SHOTS_CHROME` 指定，不给就找 Playwright 缓存里的 Chrome for Testing。
 */
import { spawn } from "node:child_process";
import { existsSync, mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync, statSync } from "node:fs";
import net from "node:net";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { Cdp, Page, sleep } from "./cdp.mjs";
import { FILEWIN_SCENES, shootFilewin } from "./filewin.mjs";
import { writeIndex } from "./index-page.mjs";

const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const args = parseArgs(process.argv.slice(2));
const out = path.resolve(args.out ?? path.join(repo, ".build/shots"));
const sandbox = path.join(repo, ".build/shots-sandbox");
const only = args.only ? new RegExp(args.only) : null;

const children = [];
const cleanup = () => {
  for (const c of children.reverse()) {
    try {
      process.kill(c.pid, "SIGTERM");
    } catch {
      // 已经退了
    }
  }
};
process.on("exit", cleanup);
process.on("SIGINT", () => process.exit(130));
process.on("SIGTERM", () => process.exit(143));

rmSync(sandbox, { recursive: true, force: true });
mkdirSync(path.join(sandbox, "home"), { recursive: true });
mkdirSync(out, { recursive: true });

/** 子进程的环境：HOME 换成沙箱，摘掉会把东西引向真环境的那几样。 */
/**
 * 摘掉会把东西引向真环境的那几族：从 cc-monitor 起的 shell 里跑时，环境里带着常驻后端的监听口、钥匙、
 * 日志与几份真文件的位置（`CCM_*`），还有 claude / Anthropic / tmux 的那几样。工具自己的 `CCM_SHOTS_*` 留着。
 */
function scrubbedEnv(extra = {}) {
  const env = { ...process.env, ...extra };
  for (const k of Object.keys(env)) {
    if (k.startsWith("CCM_SHOTS_")) continue;
    if (/^(CCM_|CLAUDE_|ANTHROPIC_|TMUX)/.test(k)) delete env[k];
  }
  return env;
}

/** 起浏览器 / vite / 文件窗口那几个子进程用：再把 HOME 换成沙箱、XDG 那几格摘掉。 */
function isolatedEnv(extra = {}) {
  const env = scrubbedEnv({ HOME: path.join(sandbox, "home"), ...extra });
  for (const k of ["XDG_CONFIG_HOME", "XDG_CACHE_HOME", "XDG_DATA_HOME", "XDG_RUNTIME_DIR"]) delete env[k];
  return env;
}

const results = [];
const problems = [];

if (!args["no-web"]) await shootWeb();
if (!args["no-filewin"]) {
  const scenes = FILEWIN_SCENES.filter((s) => !only || only.test(s.id));
  if (scenes.length > 0) {
    const r = await shootFilewin({ repo, sandbox, out, scenes, env: isolatedEnv(), buildEnv: scrubbedEnv(), children });
    results.push(...r.results);
    problems.push(...r.problems);
  }
}

// 图集：本次没重截的、上一次留下的图也列进去（按场景清单的顺序）。
writeIndex(out, results);
console.log(`\n截了 ${results.filter((r) => r.ok).length} 张；图集：${path.join(out, "index.html")}`);
if (problems.length > 0) {
  console.log(`\n⚠ ${problems.length} 处要看：`);
  for (const p of problems) console.log(`  - ${p}`);
}
writeFileSync(path.join(out, "problems.txt"), problems.join("\n") + "\n");
cleanup();
process.exit(results.some((r) => !r.ok) ? 1 : 0);

async function shootWeb() {
  const port = await freePort();
  const vite = spawn(process.execPath, [path.join(repo, "tests/shots/serve.mjs"), String(port)], {
    cwd: repo,
    env: isolatedEnv({ CCM_SHOTS_SANDBOX: sandbox }),
    stdio: ["ignore", "pipe", "inherit"],
  });
  children.push(vite);
  await new Promise((resolve, reject) => {
    let buf = "";
    vite.stdout.on("data", (d) => {
      buf += String(d);
      if (buf.includes(`READY ${port}`)) resolve();
    });
    vite.on("exit", (code) => reject(new Error(`vite 起不来（退出码 ${code}）`)));
  });
  const base = `http://127.0.0.1:${port}`;

  const chrome = findChrome();
  const profile = path.join(sandbox, "chrome");
  const browser = spawn(
    chrome,
    [
      "--headless=new",
      "--remote-debugging-port=0",
      `--user-data-dir=${profile}`,
      "--no-first-run",
      "--no-default-browser-check",
      "--no-sandbox",
      "--disable-gpu",
      "--hide-scrollbars",
      "--force-device-scale-factor=1",
      "--force-color-profile=srgb",
      "--lang=zh-CN",
      "--window-size=1280,800",
      "about:blank",
    ],
    { env: isolatedEnv(), stdio: ["ignore", "ignore", "pipe"] },
  );
  children.push(browser);
  const wsUrl = await devtoolsUrl(profile);
  const cdp = await Cdp.connect(wsUrl);

  // 清单
  const probe = await Page.open(cdp, 800, 600);
  await probe.goto(`${base}/tests/shots/manifest.html`);
  await probe.waitFor("window.__shotsManifest", 30_000);
  const manifest = await probe.eval("window.__shotsManifest");
  await probe.close();

  const scenes = manifest.filter((s) => !only || only.test(s.id));
  console.log(`网页场景 ${scenes.length} 个`);
  // 先热一次：vite 第一次按需编译整张模块图要几秒，别算进第一张图的时限。
  const warm = await Page.open(cdp, 1280, 800);
  await warm.goto(`${base}/index.html?scene=`);
  await warm.waitFor("window.__shots && window.__shots.state !== 'booting'", 60_000).catch(() => {});
  await warm.goto(`${base}/settings.html?scene=`);
  await warm.waitFor("window.__shots && window.__shots.state !== 'booting'", 60_000).catch(() => {});
  await warm.close();

  for (const s of scenes) {
    const file = path.join(out, s.dir, `${s.id}.png`);
    mkdirSync(path.dirname(file), { recursive: true });
    const page = await Page.open(cdp, s.width, s.height, s.scale ?? 1);
    let ok = true;
    let note = "";
    try {
      await page.goto(`${base}/${s.page}.html?scene=${encodeURIComponent(s.id)}${s.query ? `&${s.query}` : ""}`);
      await page.waitFor("window.__shots && window.__shots.state !== 'booting'", 30_000);
      const h = await page.eval("({ state: window.__shots.state, error: window.__shots.error, unhandled: window.__shots.unhandled })");
      if (h.state === "failed") {
        ok = false;
        note = h.error;
      }
      if (h.unhandled.length > 0) problems.push(`${s.id}：假后端答不上 ${h.unhandled.join("、")}`);
      // 让最后一帧落定（过渡动画、字体）
      await sleep(350);
      writeFileSync(file, await page.png());
    } catch (e) {
      ok = false;
      note = e instanceof Error ? e.message : String(e);
    }
    const errs = page.consoleLines.filter((l) => l.startsWith("[error]") || l.startsWith("[exception]"));
    if (errs.length > 0) problems.push(`${s.id}：页里报错 ${errs.slice(0, 3).join(" | ").slice(0, 400)}`);
    if (!ok) problems.push(`${s.id}：没截成 —— ${note.split("\n")[0]}`);
    writeFileSync(path.join(sandbox, `${s.id}.console.txt`), page.consoleLines.join("\n"));
    await page.close();
    results.push({ ...s, file: path.relative(out, file), ok });
    console.log(`${ok ? "✓" : "✗"} ${s.dir}/${s.id}`);
  }
  cdp.close();
}

function findChrome() {
  const own = process.env.CCM_SHOTS_CHROME;
  if (own) return own;
  const cache = path.join(os.homedir(), ".cache/ms-playwright");
  const found = existsSync(cache)
    ? readdirSync(cache)
        .filter((d) => /^chromium-\d+$/.test(d))
        .map((d) => path.join(cache, d, "chrome-linux64/chrome"))
        .filter((p) => existsSync(p))
        .sort((a, b) => statSync(b).mtimeMs - statSync(a).mtimeMs)
    : [];
  if (found.length === 0) {
    throw new Error("找不到 Chromium：设 CCM_SHOTS_CHROME 指向一个 chrome 可执行文件（例如 Playwright 装的 Chrome for Testing）");
  }
  return found[0];
}

async function devtoolsUrl(profile) {
  const file = path.join(profile, "DevToolsActivePort");
  for (let i = 0; i < 200; i++) {
    if (existsSync(file)) {
      const [port, p] = readFileSync(file, "utf8").split("\n");
      if (port && p) return `ws://127.0.0.1:${port}${p}`;
    }
    await sleep(100);
  }
  throw new Error("浏览器没报出调试口");
}

function freePort() {
  return new Promise((resolve, reject) => {
    const srv = net.createServer();
    srv.listen(0, "127.0.0.1", () => {
      const { port } = srv.address();
      srv.close(() => resolve(port));
    });
    srv.on("error", reject);
  });
}

function parseArgs(argv) {
  const o = {};
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (!a.startsWith("--")) continue;
    const k = a.slice(2);
    if (["out", "only"].includes(k)) o[k] = argv[++i];
    else o[k] = true;
  }
  return o;
}


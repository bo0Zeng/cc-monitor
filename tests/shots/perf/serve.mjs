/**
 * 性能台架的伺服：量**生产构建**（压缩、整包、`import.meta.env.DEV` 那几支消除），不量 vite 开发服务器的逐模块加载。
 * 用仓里那份 `vite.config.ts`，只多截图工具那两个插件（主窗口入口头上插假适配层 · 真后端池），出主窗口 · 查看窗 · 设置窗三个入口，
 * 产物进 `.build/perf-dist/`，再用 vite 的预览服务器伺服它。
 *
 * 由 `bench.mjs` 以子进程拉起（HOME 隔离）；端口从参数来，起好了往标准输出打一行 `READY <端口>`。
 * `--dev`：不构建，直接起开发服务器（只给对照用）。
 * 性能那几个世界的盘在这里照配方造（`disks.ts`），页里只交配方名（`World.diskFrom`）。
 */
import { build, createServer, preview } from "vite";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { backendPool, SHOTS_NOW_MS } from "../real/pool.mjs";

const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../../..");
const port = Number(process.argv[2]);
const dev = process.argv.includes("--dev");
const outDir = path.join(repo, ".build/perf-dist");
const cacheDir = path.join(process.env.CCM_SHOTS_SANDBOX ?? path.join(repo, ".build/perf-sandbox"), "vite-cache");

const injectFake = {
  name: "ccm-perf-inject-fake",
  transformIndexHtml: {
    order: "pre",
    handler(html, ctx) {
      if (!/^\/(index|viewer|settings)\.html$/.test(ctx.path)) return html;
      return html.replace("<head>", '<head>\n    <script type="module" src="/tests/shots/fake/install.ts"></script>');
    },
  },
};

// 性能世界的盘在这里照配方造（`disks.ts`，经 vite 的 SSR 载入），不在页里造：几万条记录的正文占的是伺服这一侧，不占页的主线程。
// 同一张配方只造一次（拨在截图那一刻，盘每次一样）；页里接着写的那一截跟着答回去。
const ssr = await createServer({ root: repo, configFile: false, cacheDir: `${cacheDir}-ssr`, logLevel: "warn", appType: "custom", server: { middlewareMode: true, hmr: false, watch: null } });
const { recipeDisk } = await ssr.ssrLoadModule("/tests/shots/perf/disks.ts");
const made = new Map();
const disks = (recipe) => {
  if (!made.has(recipe)) made.set(recipe, Promise.resolve().then(() => recipeDisk(recipe, SHOTS_NOW_MS)));
  return made.get(recipe);
};

// 真后端池（`../real/pool.mjs`）：`REAL_OPS` 那几条帧命令由它答；二进制要先编好（`cd src/backend && cargo build --features shots --target-dir ../../.build/backend-shots`，同截图台架那一份）。
const pool = backendPool({ repo, sandbox: process.env.CCM_SHOTS_SANDBOX ?? path.join(repo, ".build/perf-sandbox"), disks });
process.on("exit", () => pool.close());

const base = { root: repo, configFile: path.join(repo, "vite.config.ts"), plugins: [injectFake, pool.plugin], cacheDir, logLevel: "warn", clearScreen: false };

let server;
if (dev) {
  server = await createServer({ ...base, server: { port, strictPort: true, host: "127.0.0.1", hmr: false } });
  await server.listen();
} else {
  await build({ ...base, build: { outDir, emptyOutDir: true, sourcemap: process.env.CCM_SHOTS_SOURCEMAP === "1", rollupOptions: { input: { main: path.join(repo, "index.html"), viewer: path.join(repo, "viewer.html"), settings: path.join(repo, "settings.html") } } } });
  server = await preview({ ...base, build: { outDir }, preview: { port, strictPort: true, host: "127.0.0.1" } });
}
console.log(`READY ${port}`);

const stop = async () => {
  await (server.close?.() ?? server.httpServer?.close());
  await ssr.close();
  process.exit(0);
};
process.on("SIGTERM", stop);
process.on("SIGINT", stop);

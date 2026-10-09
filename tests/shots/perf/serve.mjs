/**
 * 性能台架的伺服：量**生产构建**（压缩、整包、`import.meta.env.DEV` 那几支消除），不量 vite 开发服务器的逐模块加载。
 * 用仓里那份 `vite.config.ts`，只多截图工具那一个插件（主窗口入口头上插假后端），只出主窗口一个入口，
 * 产物进 `.build/perf-dist/`，再用 vite 的预览服务器伺服它。
 *
 * 由 `bench.mjs` 以子进程拉起（HOME 隔离）；端口从参数来，起好了往标准输出打一行 `READY <端口>`。
 * `--dev`：不构建，直接起开发服务器（只给对照用）。
 */
import { build, createServer, preview } from "vite";
import path from "node:path";
import { fileURLToPath } from "node:url";

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
      if (!/^\/(index|viewer)\.html$/.test(ctx.path)) return html;
      return html.replace("<head>", '<head>\n    <script type="module" src="/tests/shots/fake/install.ts"></script>');
    },
  },
};

const base = { root: repo, configFile: path.join(repo, "vite.config.ts"), plugins: [injectFake], cacheDir, logLevel: "warn", clearScreen: false };

let server;
if (dev) {
  server = await createServer({ ...base, server: { port, strictPort: true, host: "127.0.0.1", hmr: false } });
  await server.listen();
} else {
  await build({ ...base, build: { outDir, emptyOutDir: true, sourcemap: process.env.CCM_SHOTS_SOURCEMAP === "1", rollupOptions: { input: { main: path.join(repo, "index.html"), viewer: path.join(repo, "viewer.html") } } } });
  server = await preview({ ...base, build: { outDir }, preview: { port, strictPort: true, host: "127.0.0.1" } });
}
console.log(`READY ${port}`);

const stop = async () => {
  await (server.close?.() ?? server.httpServer?.close());
  process.exit(0);
};
process.on("SIGTERM", stop);
process.on("SIGINT", stop);

/**
 * 截图工具的 vite：用仓里那份 `vite.config.ts`（同一套层插件、同三个入口 html），只多一个插件 ——
 * 给入口 html 头上插一句加载假后端的脚本，排在产品入口模块之前。
 *
 * 由 `run.mjs` 以子进程拉起（HOME 隔离）；端口从参数来，起好了往标准输出打一行 `READY <端口>`。
 */
import { createServer } from "vite";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const port = Number(process.argv[2]);

const injectFake = {
  name: "ccm-shots-inject-fake",
  transformIndexHtml: {
    order: "pre",
    handler(html, ctx) {
      // 只给三扇产品窗口的入口插；工具自己的页不插。
      if (!/^\/(index|settings|viewer)\.html$/.test(ctx.path)) return html;
      return html.replace(
        "<head>",
        '<head>\n    <script type="module" src="/tests/shots/fake/install.ts"></script>',
      );
    },
  },
};

const server = await createServer({
  root: repo,
  configFile: path.join(repo, "vite.config.ts"),
  plugins: [injectFake],
  // 依赖预构建的缓存放进沙箱：node_modules 可能是与别的工作树共用的那一份，别往里写。
  cacheDir: path.join(process.env.CCM_SHOTS_SANDBOX ?? path.join(repo, ".build/shots-sandbox"), "vite-cache"),
  logLevel: "warn",
  clearScreen: false,
  server: { port, strictPort: true, host: "127.0.0.1", hmr: false },
  optimizeDeps: { force: false },
});
await server.listen();
console.log(`READY ${port}`);

const stop = async () => {
  await server.close();
  process.exit(0);
};
process.on("SIGTERM", stop);
process.on("SIGINT", stop);

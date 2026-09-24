// @vitest-environment node
/**
 * 三入口拆分的判据（`设计/01 §1.2`）：**对构建产物的模块图做零命中断言。**
 *
 * ```
 * index.html     → entry-main      主窗口
 * settings.html  → entry-settings  只含设置面板 ＋ 主题 ＋ 键位
 * viewer.html    → entry-viewer    只含 tab 管理 ＋ 渲染栈
 * ```
 * ⇒ 设置窗的模块图里**根本没有**语法高亮 / 数学排版 / tab 管理。冷开就快。
 *
 * # 为什么非得真跑一遍 `vite build`
 *
 * 「看起来拆开了」有三种假法，静态读 import 一种都逮不到：
 * ① 入口文件自己干净，但它 import 的某个模块**间接**带进了 `tabs.ts`（传递闭包才看得见）；
 * ② rollup 把两个入口共用的东西抽成**共享 chunk**，那个 chunk 里混着渲染栈 ——
 *    入口 chunk 本身干净，窗口一打开照样要加载它；
 * ③ `vite.config.ts` 的 `input` 表与 html 对不上，构建出来的根本不是这三个入口。
 * ⇒ 本文件用**本仓自己的 `vite.config.ts`** 起一次内存构建（`write: false`，不碰 `.build/`），
 *   从每个入口 chunk 出发沿 `imports` ＋ `dynamicImports` 走完整个闭包，再断言。
 *   判据读的就是执行链上那张 `input` 表 —— 没有第二份副本可漂。
 *
 * # 反空真
 *
 * 「零命中」在「闭包是空的」时同样成立。所以每一条零命中都配一条**同一个谓词的正控**：
 * 禁止清单里的**每一个**模式，都必须在**别的**窗口的闭包里**命中** ——
 * 一个拼错的模式到处零命中，正控会当场红；
 * 另外每个窗口的闭包必须**含有**它该有的东西（设置窗有 `settings/panel.ts`，
 * viewer 有 `tabs.ts` ＋ `render.ts` ＋ highlight.js），免得零命中是因为整张图空了。
 *
 * # 买到 / 买不到
 *
 * - ✅ 买到：设置窗 / viewer 窗**加载的 JS 模块集合**里没有不该有的东西；Tauri 开窗那两处
 *   指向的 html 确实是构建输入之一；每个 html 加载的确实是自己那个入口。
 * - ❌ 买不到：窗口**真的开得出来、长得对**（本机无图形会话）。dev 模式下 vite 按 url 直接
 *   伺服 `settings.html` / `viewer.html`，与构建产物是同一张 `input` 表，但 dev 那条路本文件没跑。
 * - ❌ 买不到：viewer 闭包里**带进了什么多余的**（`tabs.ts` 的依赖链今天拖进了
 *   `sftp/panel.ts` / `tasks-panel.ts` 等，`tabs.ts` 不在本轮写区）。本文件只钉「不许有」清单，
 *   不钉「只许有」—— 钉全集会让每一次 `tabs.ts` 的正常改动都红在一个与拆分无关的数上。
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { build, type Rollup } from "vite";
import { describe, it, expect, beforeAll } from "vitest";
import { REPO_ROOT } from "./test-support/repo-root.ts";

const TIMEOUT_MS = 180_000;

/** 三个窗口：`vite.config.ts` 的 `input` 键 → 它的 html → 它的入口模块。 */
const WINDOWS = {
  main: { html: "index.html", entry: "src/entry-main.ts" },
  settings: { html: "settings.html", entry: "src/entry-settings.ts" },
  viewer: { html: "viewer.html", entry: "src/entry-viewer.ts" },
} as const;
type Win = keyof typeof WINDOWS;

/**
 * 模块 id 的判别式。`npm:` 前缀认 `node_modules/<包>/`，其余认仓相对路径（全等）。
 * ⚠ 刻意全等而不是子串：`src/tabs.ts` 的子串会误中 `src/views/tabs.ts` 之类的将来文件。
 */
function hit(mods: ReadonlySet<string>, pat: string): string[] {
  if (pat.startsWith("npm:")) {
    const needle = `node_modules/${pat.slice(4)}/`;
    return [...mods].filter((m) => m.includes(needle));
  }
  return mods.has(pat) ? [pat] : [];
}

/** 设置窗：只含设置面板 ＋ 主题 ＋ 键位 ⇒ 渲染栈与 tab 管理一个都不许有。 */
const FORBIDDEN_IN_SETTINGS: readonly { pat: string; what: string }[] = [
  { pat: "npm:highlight.js", what: "语法高亮" },
  { pat: "npm:katex", what: "数学排版" },
  { pat: "npm:marked-katex-extension", what: "数学排版（marked 插件）" },
  { pat: "npm:marked", what: "markdown 渲染" },
  { pat: "npm:dompurify", what: "渲染栈的 HTML 净化" },
  { pat: "src/render.ts", what: "渲染栈" },
  { pat: "src/render-stream-record.ts", what: "渲染栈（逐条记录）" },
  { pat: "src/cards/index.ts", what: "卡片系" },
  { pat: "src/tabs.ts", what: "tab 管理" },
  { pat: "src/tab-bar-state.ts", what: "tab 管理（tab 栏状态）" },
  { pat: "src/tab-collections.ts", what: "tab 管理（标签页集合）" },
  { pat: "src/main.ts", what: "主窗口 bootstrap" },
  { pat: "src/entry-main.ts", what: "主窗口入口" },
  { pat: "src/entry-viewer.ts", what: "viewer 入口" },
];

/** viewer 窗：只含 tab 管理 ＋ 渲染栈 ⇒ 主窗口 chrome 与设置面板一个都不许有。 */
const FORBIDDEN_IN_VIEWER: readonly { pat: string; what: string }[] = [
  { pat: "src/settings/panel.ts", what: "设置面板" },
  { pat: "src/keybindings/editor.ts", what: "键位编辑器（设置面板的一节）" },
  { pat: "src/views/history.ts", what: "历史视图" },
  { pat: "src/views/panorama.ts", what: "代码全景" },
  { pat: "src/views/grid-monitor.ts", what: "网格监控" },
  { pat: "src/views/command-bar.ts", what: "命令栏" },
  { pat: "src/views/cc-bus-view.ts", what: "cc-bus 视图" },
  { pat: "src/views/inbox-view.ts", what: "收件箱视图" },
  { pat: "src/main.ts", what: "主窗口 bootstrap" },
  { pat: "src/entry-main.ts", what: "主窗口入口" },
  { pat: "src/entry-settings.ts", what: "设置窗入口" },
];

/** 反空真：每个窗口**必须有**的东西（没有就说明闭包走空了，零命中不作数）。 */
const REQUIRED: Record<Win, readonly string[]> = {
  main: ["src/main.ts", "src/tabs.ts", "src/render.ts", "npm:highlight.js", "npm:katex", "src/views/history.ts"],
  settings: ["src/entry-settings.ts", "src/settings/panel.ts", "src/theme.ts", "src/keybindings/registry.ts", "src/keybindings/editor.ts"],
  viewer: ["src/entry-viewer.ts", "src/tabs.ts", "src/render.ts", "npm:highlight.js", "npm:katex", "src/theme.ts"],
};

/** 一个窗口的闭包：它会加载的全部 JS 模块 ＋ 全部 CSS 资产。 */
interface Closure {
  readonly facade: string;
  readonly modules: Set<string>;
  readonly cssAssets: Map<string, string>;
}

let CLOSURES: Record<Win, Closure>;
let INPUT: Record<string, string>;

async function buildClosures(): Promise<{ closures: Record<Win, Closure>; input: Record<string, string> }> {
  let input: Record<string, string> = {};
  const out = (await build({
    root: REPO_ROOT,
    configFile: resolve(REPO_ROOT, "vite.config.ts"),
    logLevel: "silent",
    build: { write: false },
    plugins: [
      {
        name: "entry-graphs-capture-input",
        // 读的是**合并后**的最终配置 —— 就是真构建用的那一张 input 表。
        configResolved(cfg) {
          input = { ...(cfg.build.rollupOptions.input as Record<string, string>) };
        },
      },
    ],
  })) as Rollup.RollupOutput | Rollup.RollupOutput[];
  const outputs = Array.isArray(out) ? out : [out];
  const chunks = new Map<string, Rollup.OutputChunk>();
  const assets = new Map<string, Rollup.OutputAsset>();
  for (const o of outputs) {
    for (const item of o.output) {
      if (item.type === "chunk") chunks.set(item.fileName, item);
      else assets.set(item.fileName, item);
    }
  }
  const rel = (id: string): string => id.split("\\").join("/").replace(`${REPO_ROOT.split("\\").join("/")}/`, "");
  const closures = {} as Record<Win, Closure>;
  for (const win of Object.keys(WINDOWS) as Win[]) {
    const entry = [...chunks.values()].filter((c) => c.isEntry && c.name === win);
    if (entry.length !== 1) throw new Error(`构建产物里名为 ${win} 的入口 chunk 有 ${entry.length} 个（应恰 1）`);
    const modules = new Set<string>();
    const cssAssets = new Map<string, string>();
    const seen = new Set<string>();
    const walk = (file: string): void => {
      if (seen.has(file)) return;
      seen.add(file);
      const c = chunks.get(file);
      if (!c) throw new Error(`闭包走到一个不存在的 chunk：${file}`);
      for (const id of Object.keys(c.modules)) modules.add(rel(id));
      const meta = (c as Rollup.OutputChunk & { viteMetadata?: { importedCss: Set<string> } }).viteMetadata;
      for (const css of meta?.importedCss ?? []) {
        const a = assets.get(css);
        cssAssets.set(css, typeof a?.source === "string" ? a.source : Buffer.from(a?.source ?? "").toString("utf8"));
      }
      for (const next of [...c.imports, ...c.dynamicImports]) walk(next);
    };
    walk(entry[0].fileName);
    closures[win] = { facade: rel(entry[0].facadeModuleId ?? ""), modules, cssAssets };
  }
  return { closures, input };
}

beforeAll(async () => {
  const r = await buildClosures();
  CLOSURES = r.closures;
  INPUT = r.input;
}, TIMEOUT_MS);

describe("三入口 · 住址对账（html ↔ 入口模块 ↔ vite input ↔ Tauri 开窗）", () => {
  it("vite 的 input 表恰好是这三个 html，入口 chunk 的门面就是那个 html", () => {
    expect(
      Object.fromEntries(Object.entries(INPUT).map(([k, v]) => [k, v.split("\\").join("/").split("/").pop()])),
    ).toEqual(Object.fromEntries(Object.entries(WINDOWS).map(([k, v]) => [k, v.html])));
    for (const win of Object.keys(WINDOWS) as Win[]) {
      expect(CLOSURES[win].facade, `${win} 入口 chunk 的门面不是 ${WINDOWS[win].html}`).toBe(WINDOWS[win].html);
    }
  });

  it("每个 html 恰好加载一个入口模块，且就是它自己那个", () => {
    for (const w of Object.values(WINDOWS)) {
      const html = readFileSync(resolve(REPO_ROOT, w.html), "utf8");
      const scripts = [...html.matchAll(/<script\b[^>]*\bsrc="\/([^"]+)"/g)].map((m) => m[1]);
      expect(scripts, `${w.html} 加载的入口模块不对`).toEqual([w.entry]);
    }
  });

  it("Tauri 开窗那两处指向各自的 html（不再是 index.html?viewer= / ?settings=1）", () => {
    const rs = readFileSync(resolve(REPO_ROOT, "src/bridge/src/lib.rs"), "utf8");
    const urls = [...rs.matchAll(/WebviewUrl::App\(\s*(?:format!\()?"([^"]+)"/g)].map((m) => m[1]);
    // 分母：今天恰好两处开窗（设置窗 ＋ viewer 窗）。多了少了都要有人来看一眼。
    expect(urls.sort()).toEqual(["settings.html", "viewer.html?viewer={session_id}"]);
    const htmls = new Set<string>(Object.values(WINDOWS).map((w) => w.html));
    for (const u of urls) expect(htmls.has(u.split("?")[0]), `lib.rs 开窗指向 ${u}，它不是构建输入之一`).toBe(true);
    // 主窗口由 tauri.conf.json 的 windows[0] 开，不写 url ＝ 默认 index.html。
    const conf = JSON.parse(readFileSync(resolve(REPO_ROOT, "src/bridge/tauri.conf.json"), "utf8")) as {
      app: { windows: { url?: string }[] };
    };
    expect(conf.app.windows.map((w) => w.url ?? "index.html")).toEqual(["index.html"]);
  });
});

describe("三入口 · 模块图零命中（对构建产物）", () => {
  it("反空真：每个窗口的闭包都有它该有的东西", () => {
    for (const win of Object.keys(WINDOWS) as Win[]) {
      const mods = CLOSURES[win].modules;
      expect(mods.size, `${win} 的闭包只有 ${mods.size} 个模块 —— 闭包走空了`).toBeGreaterThan(40);
      const missing = REQUIRED[win].filter((p) => hit(mods, p).length === 0);
      expect(missing, `${win} 窗口的闭包里缺这些 —— 下面的零命中不作数`).toEqual([]);
    }
  });

  it("正控：禁止清单里的每个模式，在**别的**窗口的闭包里都命中（拼错的模式会在这里红）", () => {
    // 同一个谓词 `hit` —— 它在 A 窗口里说「零命中」之前，先证明它在 B 窗口里看得见同一个东西。
    // ⚠ 第一版拿「主窗口什么都有」当正控，当场红在 `settings/panel.ts` / `keybindings/editor.ts` 上：
    //   主窗口只 import 了 `settings/index.ts` 的一个事件名常量，rollup 把面板整个摇掉了 ——
    //   **主窗口并不是什么都有**。正控改成「任一别的窗口命中」。
    const others = (w: Win): Set<string> =>
      new Set((Object.keys(WINDOWS) as Win[]).filter((x) => x !== w).flatMap((x) => [...CLOSURES[x].modules]));
    const blind = [
      ...FORBIDDEN_IN_SETTINGS.filter((f) => hit(others("settings"), f.pat).length === 0),
      ...FORBIDDEN_IN_VIEWER.filter((f) => hit(others("viewer"), f.pat).length === 0),
    ].map((f) => f.pat);
    expect(blind, "这些模式在别的窗口里也零命中 —— 它们是瞎的，零命中不作数").toEqual([]);
  });

  it("🔴 设置窗：没有语法高亮 / 数学排版 / markdown / 渲染栈 / tab 管理", () => {
    const mods = CLOSURES.settings.modules;
    const leaked = FORBIDDEN_IN_SETTINGS.flatMap((f) => hit(mods, f.pat).map((m) => `${f.what}：${m}`));
    expect(leaked, "设置窗的模块图里混进了不该有的东西（`设计/01 §1.2`：只含设置面板 ＋ 主题 ＋ 键位）").toEqual([]);
    console.log(`  ok   entry-graphs  settings 闭包 ${mods.size} 个模块，${FORBIDDEN_IN_SETTINGS.length} 个禁止模式零命中`);
  });

  it("🔴 viewer 窗：没有设置面板 / 历史 / 全景 / 网格 / 命令栏", () => {
    const mods = CLOSURES.viewer.modules;
    const leaked = FORBIDDEN_IN_VIEWER.flatMap((f) => hit(mods, f.pat).map((m) => `${f.what}：${m}`));
    expect(leaked, "viewer 窗的模块图里混进了不该有的东西（`设计/01 §1.2`：只含 tab 管理 ＋ 渲染栈）").toEqual([]);
    console.log(`  ok   entry-graphs  viewer 闭包 ${mods.size} 个模块，${FORBIDDEN_IN_VIEWER.length} 个禁止模式零命中`);
  });
});

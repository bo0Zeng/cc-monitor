// @vitest-environment node
/**
 * 三入口拆分的判据：**对构建产物的模块图做零命中断言。**
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
 *   `tasks-panel.ts` 等，`tabs.ts` 不在本轮写区；老 SFTP 面板已退役，那一条不在了）。本文件只钉「不许有」清单，
 *   不钉「只许有」—— 钉全集会让每一次 `tabs.ts` 的正常改动都红在一个与拆分无关的数上。
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { build, type Rollup } from "vite";
import ts from "typescript";
import { describe, it, expect, beforeAll } from "vitest";
import { stripCodeComments } from "../../evidence/S25-class-ledger.ts";
import { REPO_ROOT } from "../../test-support/repo-root.ts";
import { productionTsFiles } from "../../test-support/production-sources.ts";

const TIMEOUT_MS = 180_000;

/** 三个窗口：`vite.config.ts` 的 `input` 键 → 它的 html → 它的入口模块。 */
const WINDOWS = {
  main: { html: "index.html", entry: "src/frontend/ui/entry-main.ts" },
  settings: { html: "settings.html", entry: "src/frontend/ui/entry-settings.ts" },
  viewer: { html: "viewer.html", entry: "src/frontend/ui/entry-viewer.ts" },
} as const;
type Win = keyof typeof WINDOWS;

/**
 * 模块 id 的判别式。`npm:` 前缀认 `node_modules/<包>/`，其余认仓相对路径（全等）。
 * ⚠ 刻意全等而不是子串：`src/frontend/ui/tabs.ts` 的子串会误中 `src/frontend/ui/views/tabs.ts` 之类的将来文件。
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
  { pat: "src/frontend/ui/render.ts", what: "渲染栈" },
  { pat: "src/frontend/ui/render-stream-record.ts", what: "渲染栈（逐条记录）" },
  { pat: "src/frontend/ui/cards/index.ts", what: "卡片系" },
  { pat: "src/frontend/ui/tabs.ts", what: "tab 管理" },
  { pat: "src/frontend/ui/tab-bar-state.ts", what: "tab 管理（tab 栏状态）" },
  { pat: "src/frontend/ui/tab-collections.ts", what: "tab 管理（标签页集合）" },
  // `tabs.ts` 拆成 13 份之后，「设置窗里没有 tab 管理」要对每一份都成立 ——
  //   只钉 `tabs.ts` 一个名字，某一份被设置窗间接带进来时这里看不见。正控照旧（每一份都得在主窗 / viewer 的闭包里命中）。
  { pat: "src/frontend/ui/tab-model.ts", what: "tab 管理（tab 的形状与标题）" },
  { pat: "src/frontend/ui/tab-store.ts", what: "tab 管理（会话状态账 / store）" },
  { pat: "src/frontend/ui/tab-router.ts", what: "tab 管理（路由）" },
  { pat: "src/frontend/ui/tab-session-facts.ts", what: "tab 管理（从记录里抽事实）" },
  { pat: "src/frontend/ui/tab-stream-view.ts", what: "tab 管理（实时流视图）" },
  { pat: "src/frontend/ui/tab-bar-view.ts", what: "tab 管理（tab 栏视图）" },
  { pat: "src/frontend/ui/tab-bar-drag.ts", what: "tab 管理（拖拽）" },
  { pat: "src/frontend/ui/tab-drop.ts", what: "tab 管理（落点算术）" },
  { pat: "src/frontend/ui/tab-bar-prefs.ts", what: "tab 管理（集合 / 固定 / 顺序落盘）" },
  { pat: "src/frontend/ui/tab-menu.ts", what: "tab 管理（右键菜单项）" },
  { pat: "src/frontend/ui/tab-session-actions.ts", what: "tab 管理（会话动作）" },
  { pat: "src/frontend/ui/main.ts", what: "主窗口 bootstrap" },
  { pat: "src/frontend/ui/entry-main.ts", what: "主窗口入口" },
  { pat: "src/frontend/ui/entry-viewer.ts", what: "viewer 入口" },
];

/** viewer 窗：只含 tab 管理 ＋ 渲染栈 ⇒ 主窗口 chrome 与设置面板一个都不许有。 */
const FORBIDDEN_IN_VIEWER: readonly { pat: string; what: string }[] = [
  { pat: "src/frontend/ui/settings/panel.ts", what: "设置面板" },
  { pat: "src/frontend/ui/keybindings/editor.ts", what: "键位编辑器（设置面板的一节）" },
  { pat: "src/frontend/ui/views/history.ts", what: "历史视图" },
  { pat: "src/frontend/ui/views/grid-monitor.ts", what: "网格监控" },
  { pat: "src/frontend/ui/views/command-bar.ts", what: "命令栏" },
  { pat: "src/frontend/ui/views/cc-bus-view.ts", what: "cc-bus 视图" },
  { pat: "src/frontend/ui/main.ts", what: "主窗口 bootstrap" },
  { pat: "src/frontend/ui/entry-main.ts", what: "主窗口入口" },
  { pat: "src/frontend/ui/entry-settings.ts", what: "设置窗入口" },
];

/** 反空真：每个窗口**必须有**的东西（没有就说明闭包走空了，零命中不作数）。 */
const REQUIRED: Record<Win, readonly string[]> = {
  main: ["src/frontend/ui/main.ts", "src/frontend/ui/tabs.ts", "src/frontend/ui/render.ts", "npm:highlight.js", "npm:katex", "src/frontend/ui/views/history.ts"],
  settings: ["src/frontend/ui/entry-settings.ts", "src/frontend/ui/settings/panel.ts", "src/frontend/ui/theme.ts", "src/frontend/ui/keybindings/registry.ts", "src/frontend/ui/keybindings/editor.ts"],
  viewer: ["src/frontend/ui/entry-viewer.ts", "src/frontend/ui/tabs.ts", "src/frontend/ui/render.ts", "npm:highlight.js", "npm:katex", "src/frontend/ui/theme.ts"],
};

/** 一个窗口的闭包：它会加载的全部 JS 模块 ＋ 全部 CSS 资产。 */
interface Closure {
  readonly facade: string;
  readonly modules: Set<string>;
  readonly cssAssets: Map<string, string>;
  /** 闭包里全部 JS chunk 的代码拼起来 —— CSS Modules 的哈希类名要在这里真出现（代码真用上了它）。 */
  readonly jsCode: string;
}

let CLOSURES: Record<Win, Closure>;
let INPUT: Record<string, string>;
/** 构建产物：html 文件名 → 它按序链的 CSS 资产全文（每个窗口真正加载的样式，就是这一串）。 */
let BUILT_CSS_BY_HTML: Record<string, string[]>;

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
    let jsCode = "";
    const walk = (file: string): void => {
      if (seen.has(file)) return;
      seen.add(file);
      const c = chunks.get(file);
      if (!c) throw new Error(`闭包走到一个不存在的 chunk：${file}`);
      for (const id of Object.keys(c.modules)) modules.add(rel(id));
      jsCode += c.code;
      const meta = (c as Rollup.OutputChunk & { viteMetadata?: { importedCss: Set<string> } }).viteMetadata;
      for (const css of meta?.importedCss ?? []) {
        const a = assets.get(css);
        cssAssets.set(css, typeof a?.source === "string" ? a.source : Buffer.from(a?.source ?? "").toString("utf8"));
      }
      for (const next of [...c.imports, ...c.dynamicImports]) walk(next);
    };
    walk(entry[0].fileName);
    closures[win] = { facade: rel(entry[0].facadeModuleId ?? ""), modules, cssAssets, jsCode };
  }
  BUILT_CSS_BY_HTML = {};
  for (const w of Object.values(WINDOWS)) {
    const html = assets.get(w.html);
    const text = typeof html?.source === "string" ? html.source : Buffer.from(html?.source ?? "").toString("utf8");
    BUILT_CSS_BY_HTML[w.html] = [...text.matchAll(/<link rel="stylesheet"[^>]*href="\/([^"]+\.css)"/g)].map((m) => {
      const a = assets.get(m[1]);
      return typeof a?.source === "string" ? a.source : Buffer.from(a?.source ?? "").toString("utf8");
    });
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
    const rs = readFileSync(resolve(REPO_ROOT, "src/frontend/shell/src/lib.rs"), "utf8");
    const urls = [...rs.matchAll(/WebviewUrl::App\(\s*(?:format!\()?"([^"]+)"/g)].map((m) => m[1]);
    // 分母：今天恰好两处开窗（设置窗 ＋ viewer 窗）。多了少了都要有人来看一眼。
    // viewer 的 URL 多带一个 `origin`：独立窗口自己订 `session-lines/<sid>`，要知道会话在哪台机器上。
    expect(urls.sort()).toEqual(["settings.html", "viewer.html?viewer={session_id}&origin={origin_q}"]);
    const htmls = new Set<string>(Object.values(WINDOWS).map((w) => w.html));
    for (const u of urls) expect(htmls.has(u.split("?")[0]), `lib.rs 开窗指向 ${u}，它不是构建输入之一`).toBe(true);
    // 主窗口由 tauri.conf.json 的 windows[0] 开，不写 url ＝ 默认 index.html。
    const conf = JSON.parse(readFileSync(resolve(REPO_ROOT, "src/frontend/shell/tauri.conf.json"), "utf8")) as {
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
    expect(leaked, "设置窗的模块图里混进了不该有的东西（只含设置面板 ＋ 主题 ＋ 键位）").toEqual([]);
    console.log(`  ok   entry-graphs  settings 闭包 ${mods.size} 个模块，${FORBIDDEN_IN_SETTINGS.length} 个禁止模式零命中`);
  });

  it("🔴 viewer 窗：没有设置面板 / 历史 / 网格 / 命令栏", () => {
    const mods = CLOSURES.viewer.modules;
    const leaked = FORBIDDEN_IN_VIEWER.flatMap((f) => hit(mods, f.pat).map((m) => `${f.what}：${m}`));
    expect(leaked, "viewer 窗的模块图里混进了不该有的东西（只含 tab 管理 ＋ 渲染栈）").toEqual([]);
    console.log(`  ok   entry-graphs  viewer 闭包 ${mods.size} 个模块，${FORBIDDEN_IN_VIEWER.length} 个禁止模式零命中`);
  });
});

// ═══════════════════════════ 子步 2：CSS 按窗口拆 ═══════════════════════════
//
// 每个窗口的 CSS 清单**只有一份**：它的 html 里 `<link rel="stylesheet">` 的列表（按序）。
// 下面三条都从那份清单出发：
//   ① 清单自洽 —— `layers.css` 排第一（它定层的先后）、每个 `src/**/*.css` 至少被一个窗口链到；
//   ② 🔴 零命中（对**构建产物**的 CSS）—— 设置窗的 CSS 里没有高亮 / 数学 / tab / 卡片 / 流；
//      viewer 的 CSS 里没有设置面板 / 历史视图 / 网格 / 命令栏；
//   ③ 完整性 —— 窗口模块图里的代码**挂得上**的类，它在 CSS 里的每一条规则都在这个窗口的清单里
//      （拆文件最怕的就是「某个窗口少链了一份，样式静默没了」）。
//
// 买到 / 买不到（③）：
// - ✅ 按「代码里怎么挂类」的四种写法（`className =` / `class="…"` / `classList.*("…")` /
//   `querySelector(".…")` 系）＋ 模板里紧贴 `${}` 的前缀，逐窗口算出挂得上的类，再对 CSS 规则取交。
// - ❌ 运行时从数据拼出来、四种写法之外的类名，本条看不见 —— 那一族缺了样式不会在这里红。
//   拆分当时另用一把更宽的尺子（全部字符串字面量里的类名形 token）逐块分配过文件，
//   那把尺子噪声太大（`role="tab"`、主题键 `"card"` 都会被当成类），不适合做常驻判据。

/**
 * `src/**\/*.css` 的全文。人群由 `import.meta.glob` 的**键**给出（构建期展开，不新增目录遍历者），
 * 内容用 `readFileSync` 读。
 * ⚠ **不能用 `?raw` 直接取内容**：vitest 对 `.css` 模块默认整份替换成空串（`test.css` 未开），
 *   `?raw` 也一样 —— 第一版就这么写的，全部 CSS 读成 `""`，下面「完整性」那条**零命中地绿**，
 *   是「完整性的正控」那条当场逮住的。
 */
const CSS_SOURCES: Record<string, string> = Object.fromEntries(
  Object.keys(import.meta.glob("../../../src/**/*.css")).map((p) => {
    const rel = p.replace(/^(\.\.\/)+/, "");
    return [rel, readFileSync(resolve(REPO_ROOT, rel), "utf8")];
  }),
);

/**
 * CSS Modules（`*.module.css`）不走 html 的 `<link>` 清单：它们由 TS
 * `import s from "./x.module.css"` 带进窗口的模块图，类名构建时哈希。⇒ 「每份 CSS 都被某个窗口链到」与
 * 「完整性」两条只对**全局**样式文件成立；module 那一侧另有下面「CSS Modules 在产物里」那一组判。
 */
const isModuleCss = (f: string): boolean => f.endsWith(".module.css");

function linksOf(html: string): string[] {
  return [...readFileSync(resolve(REPO_ROOT, html), "utf8").matchAll(/<link rel="stylesheet" href="\/([^"]+)"/g)].map(
    (m) => m[1],
  );
}

/** 一条规则：它的选择器列表（已去注释、压空白）。只关心选择器，不关心声明。 */
function selectorsOf(css: string): string[][] {
  const text = css.replace(/\/\*[\s\S]*?\*\//g, " ");
  const out: string[][] = [];
  let buf = "";
  let depthInRule = false;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (c === '"' || c === "'") {
      const end = text.indexOf(c, i + 1);
      buf += text.slice(i, end + 1);
      i = end;
      continue;
    }
    if (depthInRule) {
      // 声明里的字符串（`content: ""`）上面那一支已经并进 buf 了：出规则时清掉，别漏进下一条的选择器。
      if (c === "}") {
        depthInRule = false;
        buf = "";
      }
      continue;
    }
    if (c === ";") {
      buf = "";
      continue;
    }
    if (c === "{") {
      const prelude = buf.trim().replace(/\s+/g, " ");
      buf = "";
      if (prelude.startsWith("@")) {
        // @keyframes 的内部是「0% {…}」这种伪规则，整块跳过
        if (/^@(?:-webkit-)?keyframes\b/.test(prelude)) {
          let d = 1;
          while (d > 0 && ++i < text.length) d += text[i] === "{" ? 1 : text[i] === "}" ? -1 : 0;
        }
        continue;
      }
      out.push(splitTop(prelude));
      depthInRule = true;
      continue;
    }
    if (c === "}") {
      buf = "";
      continue;
    }
    buf += c;
  }
  return out;
}

function splitTop(prelude: string): string[] {
  const parts: string[] = [];
  let d = 0;
  let cur = "";
  for (const c of prelude) {
    if (c === "(") d++;
    if (c === ")") d--;
    if (c === "," && d === 0) {
      parts.push(cur.trim());
      cur = "";
      continue;
    }
    cur += c;
  }
  if (cur.trim()) parts.push(cur.trim());
  return parts;
}

/** 一个选择器要命中，元素树上**必须**有哪些类与 id（`:not(…)` 里的、属性选择器里的不算）。 */
function needs(sel: string): { classes: string[]; ids: string[] } {
  let s = sel;
  for (;;) {
    const i = s.indexOf(":not(");
    if (i < 0) break;
    let d = 0;
    let k = i + 4;
    for (; k < s.length; k++) {
      if (s[k] === "(") d++;
      else if (s[k] === ")" && --d === 0) break;
    }
    s = s.slice(0, i) + s.slice(k + 1);
  }
  s = s.replace(/\[[^\]]*\]/g, "");
  return {
    classes: [...s.matchAll(/\.(-?[_a-zA-Z][\w-]*)/g)].map((m) => m[1]),
    ids: [...s.matchAll(/#(-?[_a-zA-Z][\w-]*)/g)].map((m) => m[1]),
  };
}

/** 一份源码里「挂得上」的类（四种写法）＋ 模板里紧贴 `${}` 的前缀。 */
function attachedIn(raw: string, classes: Set<string>, prefixes: Set<string>): void {
  const text = stripCodeComments(raw);
  const CLASS = /^-?[_a-zA-Z][\w-]*$/;
  const take = (s: string, tpl: boolean): void => {
    const parts = tpl ? s.split(/\$\{[^}]*\}/) : [s];
    parts.forEach((p, i) => {
      const toks = p.split(/\s+/);
      toks.forEach((t, j) => {
        if (!t) return;
        if (tpl && j === toks.length - 1 && i < parts.length - 1 && /[\w-]$/.test(t)) prefixes.add(t);
        else if (CLASS.test(t)) classes.add(t);
      });
    });
  };
  for (const m of text.matchAll(/\bclassName\s*(?:=|\+=|:)\s*(["'`])((?:(?!\1)[^\\]|\\.)*)\1/g)) take(m[2], m[1] === "`");
  for (const m of text.matchAll(/\bclass\s*=\s*\\?"([^"\n<>]*)\\?"/g)) take(m[1], true);
  for (const m of text.matchAll(/\bclassList\.(?:add|remove|toggle|contains|replace)\(([^)\n]*)\)/g))
    for (const lit of m[1].matchAll(/"([^"\n]*)"|'([^'\n]*)'|`([^`\n]*)`/g)) take(lit[1] ?? lit[2] ?? lit[3] ?? "", lit[3] !== undefined);
  for (const m of text.matchAll(/\b(?:querySelector|querySelectorAll|closest|matches)\(\s*(["'`])([^"'`\n]*)\1/g))
    for (const c of m[2].matchAll(/\.(-?[_a-zA-Z][\w-]*)/g)) classes.add(c[1]);
}

/** 窗口 W 的代码挂得上的类 / 前缀 / id（按窗口缓存 —— 源码在一次运行里不变）。 */
const REACHABLE = new Map<Win, { classes: Set<string>; prefixes: string[]; ids: Set<string> }>();
function reachable(win: Win): { classes: Set<string>; prefixes: string[]; ids: Set<string> } {
  const hitCache = REACHABLE.get(win);
  if (hitCache) return hitCache;
  const r = computeReachable(win);
  REACHABLE.set(win, r);
  return r;
}
function computeReachable(win: Win): { classes: Set<string>; prefixes: string[]; ids: Set<string> } {
  const classes = new Set<string>();
  const prefixes = new Set<string>();
  const ids = new Set<string>();
  const files = [...CLOSURES[win].modules].filter((m) => m.startsWith("src/") && m.endsWith(".ts"));
  const texts = files.map((f) => readFileSync(resolve(REPO_ROOT, f), "utf8"));
  // `${CONST}` 形的常量拼接：先把全窗口的 `const X = "字面量"` 收起来填回去再扫一遍
  const consts = new Map<string, string>();
  for (const t of texts)
    for (const m of t.matchAll(/\b(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*(?::[^=;\n]+)?=\s*(["'])((?:(?!\2)[^\\\n]|\\.)*)\2/g))
      consts.set(m[1], m[3]);
  for (const t of texts) {
    attachedIn(t, classes, prefixes);
    attachedIn(t.replace(/\$\{\s*([A-Za-z_$][\w$]*)\s*\}/g, (a, id: string) => consts.get(id) ?? a), classes, prefixes);
    for (const m of t.matchAll(/\.id\s*=\s*["'`]([\w-]+)["'`]/g)) ids.add(m[1]);
  }
  for (const m of readFileSync(resolve(REPO_ROOT, WINDOWS[win].html), "utf8").matchAll(/\bid="([\w-]+)"/g)) ids.add(m[1]);
  return { classes, prefixes: [...prefixes].filter((p) => p.length >= 4), ids };
}

/** 规则里有没有一个选择器，在窗口 W 里挂得上。 */
function matchableIn(sels: string[], r: ReturnType<typeof reachable>): boolean {
  const has = (c: string): boolean => r.classes.has(c) || r.prefixes.some((p) => c.startsWith(p) && c.length > p.length);
  return sels.some((sel) => {
    const n = needs(sel);
    return n.classes.every(has) && n.ids.every((i) => r.ids.has(i));
  });
}

const CSS_RULES: Record<string, string[][]> = Object.fromEntries(
  Object.entries(CSS_SOURCES)
    .filter(([f]) => !isModuleCss(f))
    .map(([f, css]) => [f, selectorsOf(css)]),
);

/** 完整性：W 挂得上的每一条规则，它所在的文件都在 W 的清单里。返回缺的（文件 → 选择器）。 */
function missingIn(win: Win, links: readonly string[]): string[] {
  const r = reachable(win);
  const linked = new Set(links);
  const out: string[] = [];
  for (const [file, rules] of Object.entries(CSS_RULES)) {
    if (linked.has(file)) continue;
    for (const sels of rules) if (matchableIn(sels, r)) out.push(`${file} :: ${sels.join(", ")}`);
  }
  return out;
}

/** 构建产物 CSS 里出现过的类名（压缩过的文本，只取选择器段）。 */
function builtClasses(win: Win): Set<string> {
  const out = new Set<string>();
  for (const css of CLOSURES[win].cssAssets.values()) for (const sels of selectorsOf(css)) for (const s of sels) for (const c of needs(s).classes) out.add(c);
  return out;
}

/** 设置窗的 CSS 里不许出现的类族（前缀；`^tab$` 这种精确名也按前缀写，靠 `-` 边界区分）。 */
const CSS_FORBIDDEN_IN_SETTINGS = ["hljs", "katex", "tab", "card", "code-block", "code-copy", "stream", "block-", "branch-fold", "live-dot"];
/** viewer 的 CSS 里不许出现的类族。 */
const CSS_FORBIDDEN_IN_VIEWER = ["settings-panel", "settings-body", "kb-editor", "grid-monitor-cell", "command-bar", "ext-", "accounts-row"];
function familyHits(classes: Set<string>, fam: string): string[] {
  return [...classes].filter((c) => (fam.endsWith("-") ? c.startsWith(fam) : c === fam || c.startsWith(`${fam}-`)));
}

describe("子步 2 · CSS 按窗口拆（清单 ＝ 各 html 的 <link> 列表）", () => {
  it("清单自洽：每个窗口先链 layers.css；每份 src CSS 至少被一个窗口链到；链到的文件都存在", () => {
    const all = new Set<string>();
    for (const w of Object.values(WINDOWS)) {
      const links = linksOf(w.html);
      expect(links[0], `${w.html} 的第一条样式表不是 layers.css —— 层的先后会随窗口而变`).toBe("src/frontend/ui/styles/layers.css");
      for (const l of links) {
        expect(CSS_SOURCES[l], `${w.html} 链了一份不存在的样式表 ${l}`).toBeDefined();
        all.add(l);
      }
    }
    expect(Object.keys(CSS_SOURCES).length, "glob 没扫到 CSS —— 下面那条零命中地绿").toBeGreaterThan(5);
    const empty = Object.entries(CSS_SOURCES).filter(([, t]) => t.trim().length < 50).map(([f]) => f);
    expect(empty, "这些 CSS 读出来是空的 —— 读法坏了（`?raw` 在 vitest 里就是这么坏的）").toEqual([]);
    expect(
      Object.keys(CSS_SOURCES).filter((f) => !all.has(f) && !isModuleCss(f)),
      "这些 CSS 没有任何窗口链它 —— 写了等于没写",
    ).toEqual([]);
    expect(
      [...all].filter(isModuleCss),
      "html 直接链了 CSS Module —— 那样类名不哈希、TS 那边的 `s.xxx` 对不上它（module 只许经 TS 导入）",
    ).toEqual([]);
  });

  it("🔴 设置窗的构建产物 CSS：没有高亮 / 数学 / tab / 卡片 / 流 / 折叠块", () => {
    const got = builtClasses("settings");
    expect(got.size, "设置窗的构建 CSS 一个类都没读到 —— 零命中不作数").toBeGreaterThan(200);
    const leaked = CSS_FORBIDDEN_IN_SETTINGS.flatMap((f) => familyHits(got, f));
    expect(leaked, "设置窗的 CSS 里混进了渲染栈 / tab 管理的样式").toEqual([]);
  });

  it("🔴 viewer 的构建产物 CSS：没有设置面板 / 历史视图 / 网格 / 命令栏", () => {
    const got = builtClasses("viewer");
    expect(got.size, "viewer 的构建 CSS 一个类都没读到 —— 零命中不作数").toBeGreaterThan(200);
    const leaked = CSS_FORBIDDEN_IN_VIEWER.flatMap((f) => familyHits(got, f));
    expect(leaked, "viewer 的 CSS 里混进了主窗 chrome / 设置面板的样式").toEqual([]);
  });

  it("正控：两张 CSS 禁止表里的每一族，在主窗的构建 CSS 里都命中（拼错的族在这里红）", () => {
    const main = builtClasses("main");
    const settings = builtClasses("settings");
    const blind = [
      ...CSS_FORBIDDEN_IN_SETTINGS.filter((f) => familyHits(main, f).length === 0),
      ...CSS_FORBIDDEN_IN_VIEWER.filter((f) => familyHits(main, f).length === 0 && familyHits(settings, f).length === 0),
    ];
    expect(blind, "这些族在别的窗口里也零命中 —— 它们是瞎的").toEqual([]);
  });

  it("完整性：每个窗口的代码挂得上的类，它们的每条规则都在这个窗口的清单里", () => {
    for (const win of Object.keys(WINDOWS) as Win[]) {
      const r = reachable(win);
      expect(r.classes.size, `${win} 挂得上的类只收到 ${r.classes.size} 个 —— 抽取坏了`).toBeGreaterThan(150);
      expect(missingIn(win, linksOf(WINDOWS[win].html)), `${win} 窗口挂得上、却没链到样式的规则`).toEqual([]);
    }
  }, TIMEOUT_MS);

  it("完整性的正控：从每个窗口的清单里各摘掉一份，这条判据必须红", () => {
    for (const win of Object.keys(WINDOWS) as Win[]) {
      const links = linksOf(WINDOWS[win].html);
      const blind = links
        .filter((l) => !["src/frontend/ui/styles/layers.css"].includes(l))
        .filter((drop) => missingIn(win, links.filter((l) => l !== drop)).length === 0);
      // `layers.css` 只有一句层声明、没有规则，不在这里判（它的住址由「清单自洽」那条钉：必须排第一）。
      // `reset.css` / `tokens.css` 只有 `*` / `html` / `:root` 这种无类选择器 —— 它们在任何窗口都挂得上，
      // 所以摘掉同样会红（第一版以为这两份看不见，现打是看得见的）。
      expect(blind, `${win}：摘掉这些文件，完整性判据照样绿`).toEqual([]);
    }
  }, TIMEOUT_MS);
});

// ═══════════════════════════ CSS Modules 在构建产物里（件 10）═══════════════════════════
//
// 源码那一侧（全局样式文件集合 == 登记表 · 逐文件类型 == 类名集合 · 只经默认导入用 · 每个类都有人取 ·
// tsc 真吃到逐文件类型）住 `tests/frontend/ui/css-modules.vitest.ts`。这里判**产物**，因为有两件事只在构建里发生：
//   ① 类名真的哈希了、代码里真的是那个哈希名（vite 的 CSS Modules 没生效时，产物里是原名 `.chip`，
//      而 TS 侧 `s.chip` 在 vitest 里照样拿到一个串 —— 源码判据一格都看不见）；
//   ② 次序：JS 导入的 module 样式排在 html 链的全局样式**之后**。示范组件的 delta 与它叠的全局基类
//      `.status-tasks` 同层同特异度（`font: inherit` 对 `font-variant-numeric` · `:hover` 对 `color`/`background`），
//      谁后谁赢 —— 这条次序一翻，delta 静默失效。
// 次序**只对「叠在全局基类上的 delta」**要求：module 的类与某个全局类挂在同一个元素上，
//      才有「同层同特异度谁后谁赢」这回事。不叠在任何全局类上的 module（元素只挂它自己的哈希类）不受次序约束 ——
//      它被两个窗口共用时 vite 把它的 CSS 放进共享 chunk，那份样式排在全局之前，而那对它不构成任何覆盖关系。
//      「叠不叠」由 [`moduleStacking`] 从导入方的 TS 里认（下面「叠没叠」那一格 == 手写表，两向）。
// ⚠ 哈希名的形状认的是 vite 的默认 `_[local]_[hash]…`（现打 `_chip_16b8l_4`）；有人在 `vite.config.ts` 里改
//   `css.modules.generateScopedName`，下面「恰有一个哈希名」那条会红 —— 那时照新形状改这里的正则。

/** 全局样式文件（非 module）里出现过的类名。 */
const GLOBAL_CLASSES = new Set(Object.values(CSS_RULES).flatMap((rules) => rules.flat().flatMap((sel) => needs(sel).classes)));
/** 每份 `.module.css` → 它源码里的类名（去重）。 */
const MODULE_CLASSES: Record<string, string[]> = Object.fromEntries(
  Object.entries(CSS_SOURCES)
    .filter(([f]) => isModuleCss(f))
    .map(([f, css]) => [f, [...new Set(selectorsOf(css).flat().flatMap((sel) => needs(sel).classes))]]),
);
const hashedOf = (built: Set<string>, k: string): string[] =>
  [...built].filter((c) => c.startsWith(`_${k}_`) && /^_[\w-]+$/.test(c));

/** 一个窗口产物 CSS（按 `<link>` 序拼起来）里，第一条 module 规则之后还出现的全局规则。 */
function globalRulesAfterFirstModule(text: string, hashed: readonly string[]): string[] {
  const at = Math.min(...hashed.map((h) => text.indexOf(`.${h}`)).filter((i) => i >= 0));
  if (!Number.isFinite(at)) return [];
  const from = Math.max(text.lastIndexOf("}", at), text.lastIndexOf("{", at)) + 1;
  const hs = new Set(hashed);
  return selectorsOf(text.slice(from))
    .flat()
    .filter((sel) => {
      const cs = needs(sel).classes;
      return cs.some((c) => GLOBAL_CLASSES.has(c)) && !cs.some((c) => hs.has(c));
    });
}

/**
 * 一份 module 叠没叠在全局基类上：在导入它的生产 TS 里，看它的类挂到了哪个元素上、那个元素还挂了什么。
 *
 * 认的挂法（按接收者的**源码文本**归到同一个元素：`btn.className = …` 与 `btn.classList.toggle(…)` 算同一个 `btn`）：
 * - `<接收者>.className = <表达式>` —— 表达式里的字符串 / 模板字面量按空白切出类名，`s.<类>` 认成 module 类；
 * - `<接收者>.classList.add / toggle / replace(…)` —— 实参同上（`remove` / `contains` 不算挂）。
 * 某个接收者身上**同时**有这份 module 的类与一个全局类（`GLOBAL_CLASSES`）⇒ 叠。
 * 🔴 保守的那一向：`s.<类>` 出现在上面两形**之外**（传给函数、塞进对象、拼进别的串……）⇒ 认不出挂到哪 ⇒ **按「叠」算**
 *   （要求次序 —— 红得出来，不会静默放过）。同名接收者在不同函数里指不同元素 ⇒ 同样只会多判「叠」。
 */
function moduleStacking(
  sources: ReadonlyArray<{ file: string; text: string }>,
  modulePath: string,
  globals: ReadonlySet<string>,
): { stacked: boolean; why: string[] } {
  const why: string[] = [];
  for (const { file, text } of sources) {
    const sf = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS);
    const names = new Set<string>();
    for (const st of sf.statements) {
      if (
        ts.isImportDeclaration(st) &&
        ts.isStringLiteral(st.moduleSpecifier) &&
        st.moduleSpecifier.text.endsWith(".module.css") &&
        st.importClause?.name &&
        resolve(REPO_ROOT, file, "..", st.moduleSpecifier.text) === resolve(REPO_ROOT, modulePath)
      ) {
        names.add(st.importClause.name.text);
      }
    }
    if (names.size === 0) continue;
    const isModRef = (n: ts.Node): boolean =>
      ts.isPropertyAccessExpression(n) && ts.isIdentifier(n.expression) && names.has(n.expression.text);
    /** 接收者文本 → 挂上去的 `{ module: 有没有, globals: 哪几个 }`。 */
    const onEl = new Map<string, { module: boolean; globals: Set<string> }>();
    const attached = new Set<ts.Node>();
    const collect = (recv: string, exprs: readonly ts.Node[]): void => {
      const e = onEl.get(recv) ?? { module: false, globals: new Set<string>() };
      const walk = (n: ts.Node): void => {
        if (isModRef(n)) {
          e.module = true;
          attached.add(n);
          return;
        }
        if (ts.isStringLiteralLike(n) || ts.isTemplateHead(n) || ts.isTemplateMiddle(n) || ts.isTemplateTail(n)) {
          for (const tok of n.text.split(/\s+/)) if (globals.has(tok)) e.globals.add(tok);
        }
        ts.forEachChild(n, walk);
      };
      exprs.forEach(walk);
      onEl.set(recv, e);
    };
    const visit = (n: ts.Node): void => {
      if (
        ts.isBinaryExpression(n) &&
        n.operatorToken.kind === ts.SyntaxKind.EqualsToken &&
        ts.isPropertyAccessExpression(n.left) &&
        n.left.name.text === "className"
      ) {
        collect(n.left.expression.getText(sf), [n.right]);
      } else if (
        ts.isCallExpression(n) &&
        ts.isPropertyAccessExpression(n.expression) &&
        ["add", "toggle", "replace"].includes(n.expression.name.text) &&
        ts.isPropertyAccessExpression(n.expression.expression) &&
        n.expression.expression.name.text === "classList"
      ) {
        collect(n.expression.expression.expression.getText(sf), n.arguments);
      }
      ts.forEachChild(n, visit);
    };
    visit(sf);
    for (const [recv, e] of onEl) {
      if (e.module && e.globals.size > 0) why.push(`${file}：${recv} 同挂 ${[...e.globals].join(" ")}`);
    }
    const loose = (n: ts.Node): void => {
      if (isModRef(n) && !attached.has(n)) {
        const parent = n.parent;
        // `classList.remove(s.x)` / `classList.contains(s.x)` 不是挂，也不是认不出的用法。
        const isRemoveLike =
          ts.isCallExpression(parent) &&
          ts.isPropertyAccessExpression(parent.expression) &&
          ["remove", "contains"].includes(parent.expression.name.text);
        if (!isRemoveLike) why.push(`${file}：认不出 ${n.getText(sf)} 挂到哪（按「叠」算）`);
      }
      ts.forEachChild(n, loose);
    };
    loose(sf);
  }
  return { stacked: why.length > 0, why };
}

/**
 * 每份 module 叠没叠在全局基类上 —— **手写**（期望不从 [`moduleStacking`] 派生）。新长一份 module ⇒ 这张表红，
 * 回来写清它叠不叠、为什么。
 */
const MODULE_STACKING: Record<string, { stacked: boolean; why: string }> = {
  "src/frontend/ui/usage-hud.module.css": { stacked: true, why: "`.chip` 叠在全局 `.status-tasks` 上（usage-hud.ts 里 btn.className 同时挂 status-tasks 与 s.chip）" },
  "src/frontend/ui/acct.module.css": { stacked: true, why: "状态栏账号按钮 `.acctChip` 叠在全局 `.status-account` 上（account-chip.ts 里 btn.className 同时挂两样）；面板里经小工具函数挂的类量具认不出挂到哪，按「叠」算" },
  "src/frontend/ui/acct-session.module.css": { stacked: true, why: "换号条 · 提示条：类经小工具函数挂，量具认不出挂到哪，按「叠」算（实际只挂自己的哈希类）" },
  "src/frontend/ui/tab-quota.module.css": { stacked: false, why: "tab 标题后 `✕ 5h` 那一格：只挂自己的哈希类（不叠全局类）" },
  "src/frontend/ui/views/history.module.css": { stacked: true, why: "历史页根上同时挂全局 `history-view`、搜索框挂 `history-search`（只当截图 / 端到端找它的钩子，全局 CSS 里没有这两条）" },
  "src/frontend/ui/live-card.module.css": { stacked: false, why: "活卡（`live-card-view.ts` 画）：卡 / 顶上那行 / 正文只挂自己的哈希类（不叠全局类）" },
  "src/frontend/ui/needs-bar.module.css": { stacked: false, why: "「需要你」钉条（`needs-bar.ts`）：只挂自己的哈希类" },
  "src/frontend/ui/session-head.module.css": { stacked: false, why: "会话头（`session-head.ts`）：只挂自己的哈希类" },
  "src/frontend/ui/record-file-notice.module.css": { stacked: false, why: "tab 顶上「记录文件不见了 / 已从头重读」那一句：只挂自己的哈希类 `.notice`（不叠全局类）" },
  "src/frontend/ui/tab-group-rename.module.css": { stacked: false, why: "组头就地改名的输入框只挂自己的哈希类" },
  "src/frontend/ui/kit/icon.module.css": { stacked: false, why: "图标件：svg 只挂自己的哈希类" },
  "src/frontend/ui/kit/badge.module.css": { stacked: false, why: "通用组件：只挂自己的哈希类" },
  "src/frontend/ui/kit/banner.module.css": { stacked: false, why: "通用组件：只挂自己的哈希类" },
  "src/frontend/ui/kit/block.module.css": { stacked: false, why: "通用组件：只挂自己的哈希类" },
  "src/frontend/ui/kit/button.module.css": { stacked: false, why: "通用组件：只挂自己的哈希类" },
  "src/frontend/ui/kit/card.module.css": { stacked: false, why: "通用组件：只挂自己的哈希类" },
  "src/frontend/ui/kit/chip.module.css": { stacked: false, why: "通用组件：只挂自己的哈希类" },
  "src/frontend/ui/kit/dialog.module.css": { stacked: false, why: "通用组件：只挂自己的哈希类" },
  "src/frontend/ui/kit/drawer.module.css": { stacked: false, why: "通用组件：只挂自己的哈希类" },
  "src/frontend/ui/kit/empty.module.css": { stacked: false, why: "通用组件：只挂自己的哈希类" },
  "src/frontend/ui/kit/field.module.css": { stacked: false, why: "通用组件：只挂自己的哈希类" },
  "src/frontend/ui/kit/fold.module.css": { stacked: false, why: "通用组件：只挂自己的哈希类" },
  "src/frontend/ui/kit/list-row.module.css": { stacked: false, why: "通用组件：只挂自己的哈希类" },
  "src/frontend/ui/kit/menu.module.css": { stacked: false, why: "通用组件：只挂自己的哈希类" },
  "src/frontend/ui/kit/meter.module.css": { stacked: false, why: "通用组件：只挂自己的哈希类" },
  "src/frontend/ui/kit/popover.module.css": { stacked: false, why: "通用组件：只挂自己的哈希类" },
  "src/frontend/ui/kit/split-button.module.css": { stacked: false, why: "通用组件（拆分按钮，暂定）：只挂自己的哈希类" },
  "src/frontend/ui/views/session-viewer.module.css": { stacked: false, why: "只读查看器的外框（头 · 工具行 · 底一行）：只挂自己的哈希类；判据找元素用 `data-role`" },
  "src/frontend/ui/find-strip.module.css": { stacked: false, why: "会话内查找（框 ＋ 命中清单）：只挂自己的哈希类；判据找元素用 `data-role`" },
  "src/frontend/ui/kit/progress.module.css": { stacked: false, why: "通用组件：只挂自己的哈希类" },
  "src/frontend/ui/kit/skeleton.module.css": { stacked: false, why: "通用组件：只挂自己的哈希类" },
  "src/frontend/ui/kit/status-dot.module.css": { stacked: false, why: "通用组件：只挂自己的哈希类" },
  "src/frontend/ui/kit/switch.module.css": { stacked: false, why: "通用组件：只挂自己的哈希类" },
  "src/frontend/ui/kit/tabs.module.css": { stacked: false, why: "通用组件：只挂自己的哈希类" },
  "src/frontend/ui/kit/toast.module.css": { stacked: false, why: "通用组件：只挂自己的哈希类" },
  "src/frontend/ui/kit/tooltip.module.css": { stacked: false, why: "通用组件：只挂自己的哈希类" },
};

/**
 * 通用组件里还没有任何一面用上的那几件（地基先立、各面照稿施工时接上）。登记了却已经进了某个窗口 ⇒ 红（摘掉登记）；
 * 没登记又不在任何窗口里 ⇒ 照旧红。只许缩。
 */
const KIT_AWAITING_FACES: ReadonlySet<string> = new Set(
  // banner · drawer · fold · meter · tabs 已由主窗口的「账号」面板用上；badge · status-dot 由标签页栏（状态点 · 机器徽标 · 「需要你」计数）用上；
  // progress 由消息流过程里那一步的「在跑」转圈用上（主窗口第 2 批）；empty · skeleton · switch 由历史页（空态 · 骨架 · 筛选里的勾与单选）用上。
  ["block", "card", "chip", "field", "list-row"].map(
    (k) => `src/frontend/ui/kit/${k}.module.css`,
  ),
);

/** 已有面用上、但其中一形还没有面的 kit 件：那一形的类今天产物 JS 里没有（两向：用上了就从这里摘）。 */
const KIT_PARTIAL: Readonly<Record<string, readonly string[]>> = {
  // （分段按钮 C23 账号面板用上了，分栏 C5 那一形由历史页「按时间 | 按项目」用上了 ⇒ tabs 那一条摘了。）
  // 转圈（`spinner`）消息流与历史页都用上了；进度条（C17）那一形还没有面。
  "src/frontend/ui/kit/progress.module.css": ["progressFill", "progressReadout", "progressRow", "progressTrack"],
  // 勾与单选（`checkbox` · `radio`）历史页的筛选用上了；开关那一形还没有面。
  "src/frontend/ui/kit/switch.module.css": ["swRow", "swSwitch", "swText"],
};

describe("〔UC2〕CSS Modules 在构建产物里（件 10）", () => {
  it("每份 .module.css 都进了某个窗口的模块图；每个类在产物 CSS 里恰有一个哈希名、原名不出现、哈希名在那个窗口的 JS 里真出现", () => {
    const mods = Object.keys(MODULE_CLASSES);
    expect(mods.length, "一份 `.module.css` 都没有 —— 本组零命中地绿（件 10 的示范被删了？）").toBeGreaterThan(0);
    let judged = 0;
    for (const f of mods) {
      const wins = (Object.keys(WINDOWS) as Win[]).filter((w) => CLOSURES[w].modules.has(f));
      if (KIT_AWAITING_FACES.has(f)) {
        expect(wins, `${f} 已经有窗口用上了 —— 把它从 KIT_AWAITING_FACES 里摘掉`).toEqual([]);
        continue;
      }
      expect(wins, `${f} 不在任何窗口的模块图里 —— 没有代码导入它，它进不了产物`).not.toEqual([]);
      expect(MODULE_CLASSES[f].length, `${f} 里一个类都没抽到 —— 下面零命中地绿`).toBeGreaterThan(0);
      for (const w of wins) {
        const built = builtClasses(w);
        for (const k of MODULE_CLASSES[f]) {
          const hashed = hashedOf(built, k);
          expect(hashed, `${w}：${f} 的 .${k} 在产物 CSS 里应恰有一个哈希名`).toHaveLength(1);
          expect(built.has(k) && !GLOBAL_CLASSES.has(k), `${w}：产物 CSS 里出现了原名 .${k} —— CSS Modules 没生效`).toBe(false);
          const partial = KIT_PARTIAL[f]?.includes(k) ?? false;
          expect(CLOSURES[w].jsCode.includes(hashed[0]), partial ? `${w}：${f} 的 .${k} 已经用上了 —— 从 KIT_PARTIAL 摘掉` : `${w}：哈希名 ${hashed[0]} 不在 JS 里 —— 代码没用上它`).toBe(!partial);
          judged++;
        }
      }
    }
    expect(judged).toBeGreaterThan(0);
  });

  it("〔W5-UI〕叠没叠：每份 module 由导入方 TS 认出的「叠在全局基类上」== 手写表（两向）；量具正反控", () => {
    const sources = productionTsFiles("src").map((f) => ({ file: f.file, text: f.text }));
    const got = Object.fromEntries(Object.keys(MODULE_CLASSES).map((f) => [f, moduleStacking(sources, f, GLOBAL_CLASSES).stacked]));
    const want = Object.fromEntries(Object.entries(MODULE_STACKING).map(([f, v]) => [f, v.stacked]));
    expect(got, "module 叠没叠变了 / 有 module 没进手写表 —— 回 MODULE_STACKING 写清").toEqual(want);
    expect(Object.values(want).some(Boolean), "手写表里一份「叠」的都没有 —— 下面的次序判据零命中地绿").toBe(true);
    // 量具正反控（合成源；`status-tasks` 必须真是全局类，否则正控空转）
    expect(GLOBAL_CLASSES.has("status-tasks"), "正控用的全局类不在了 —— 换一个真全局类").toBe(true);
    const probe = (body: string): boolean =>
      moduleStacking([{ file: "src/probe.ts", text: `import s from "./probe.module.css";\n${body}` }], "src/probe.module.css", GLOBAL_CLASSES).stacked;
    expect(probe("el.className = `status-tasks ${s.a}`;"), "同一句里叠").toBe(true);
    expect(probe(`el.className = s.a; el.classList.add("status-tasks");`), "同一接收者分两句叠").toBe(true);
    expect(probe(`el.classList.toggle(s.a, on); el.className = "status-tasks";`), "toggle 挂上去也算").toBe(true);
    expect(probe(`help(s.a);`), "认不出挂到哪 ⇒ 按叠算（保守）").toBe(true);
    expect(probe(`a.className = s.a; b.className = "status-tasks";`), "不同元素不算叠").toBe(false);
    expect(probe(`a.className = s.a; a.classList.remove(s.a); a.className = "not-a-global-xyz";`), "非全局类 / remove 不算叠").toBe(false);
  }, TIMEOUT_MS); // 全仓生产 TS 逐份建 AST：整套并跑时 5 s 默认期限不够（现打 7.7 s）

  it("次序：每个窗口产物 CSS 里，第一条**叠在全局基类上的** module 规则之后不再有全局样式的规则（module 的 delta 叠在全局基类之上）", () => {
    let judged = 0;
    for (const win of Object.keys(WINDOWS) as Win[]) {
      const built = builtClasses(win);
      // 只数叠在全局基类上的那几份（手写表；上一格钉它 == 量具）。
      const hashed = Object.entries(MODULE_CLASSES)
        .filter(([f]) => MODULE_STACKING[f]?.stacked !== false)
        .flatMap(([, ks]) => ks.flatMap((k) => hashedOf(built, k)));
      if (hashed.length === 0) continue;
      const text = BUILT_CSS_BY_HTML[WINDOWS[win].html].join("\n");
      expect(globalRulesAfterFirstModule(text, hashed), `${win}：module 规则后面还跟着全局规则 —— 同特异度时全局压过 module`).toEqual([]);
      // 正控：同一个谓词对「module 之后再追加一条全局规则」必须红（谓词瞎了，上面那条零命中地绿）
      const probe = [...GLOBAL_CLASSES][0];
      expect(globalRulesAfterFirstModule(`${text}.${probe}{color:red}`, hashed), "次序谓词认不出追加在后面的全局规则").toHaveLength(1);
      judged++;
    }
    expect(judged, "没有任何窗口的产物里有 module 规则 —— 本条零命中地绿").toBeGreaterThan(0);
  });
});

// ═══════════════════════════ 子步 3：层真包进去（对构建产物）═══════════════════════════
//
// 源码那一侧（每条规则都在层里、层名不拼错、声明次序）住 `tests/frontend/ui/css-ledger.vitest.ts` 格 ⑤。
// 这里判**产物**，因为有两件事只在构建里发生：
//   ① 第三方 CSS（highlight.js 主题、KaTeX）由 `vite.config.ts` 的 postcss 插件包进 `@layer vendor` ——
//      插件没生效，它们就是无层的，会反过来压住我们所有的覆盖；
//   ② vite 按 JS chunk 重新归并 CSS 资产、重排 `<link>` —— 层声明那一句必须仍在每个窗口的**第一份**资产的最前面。

/** 压缩过的 CSS：顶层（不在任何 at-rule 里）的规则 ＋ 每条规则所在的层。 */
function builtLayerScan(css: string): { topLevelRules: string[]; layerOfClass: Map<string, Set<string>>; firstLayerStmt: string | null } {
  const topLevelRules: string[] = [];
  const layerOfClass = new Map<string, Set<string>>();
  let firstLayerStmt: string | null = null;
  const stack: string[] = [];
  let buf = "";
  for (let i = 0; i < css.length; i++) {
    const c = css[i];
    if (c === '"' || c === "'") {
      const end = css.indexOf(c, i + 1);
      buf += css.slice(i, end + 1);
      i = end;
      continue;
    }
    if (c === "/" && css[i + 1] === "*") {
      i = css.indexOf("*/", i + 2) + 1;
      continue;
    }
    if (c === ";") {
      const t = buf.trim();
      if (firstLayerStmt === null && /^@layer\s/.test(t) && stack.length === 0) firstLayerStmt = t;
      buf = "";
      continue;
    }
    if (c === "{") {
      const prelude = buf.trim();
      buf = "";
      if (prelude.startsWith("@")) {
        if (/^@(?:-webkit-)?keyframes\b|^@font-face\b/.test(prelude)) {
          let d = 1;
          while (d > 0 && ++i < css.length) d += css[i] === "{" ? 1 : css[i] === "}" ? -1 : 0;
          continue;
        }
        stack.push(prelude);
        continue;
      }
      const layer = [...stack].reverse().find((a) => a.startsWith("@layer "));
      if (!layer) topLevelRules.push(prelude.slice(0, 60));
      for (const m of prelude.matchAll(/\.(-?[_a-zA-Z][\w-]*)/g)) {
        const set = layerOfClass.get(m[1]) ?? new Set<string>();
        set.add(layer ? layer.slice(7) : "<无层>");
        layerOfClass.set(m[1], set);
      }
      let d = 1;
      while (d > 0 && ++i < css.length) d += css[i] === "{" ? 1 : css[i] === "}" ? -1 : 0;
      continue;
    }
    if (c === "}") {
      stack.pop();
      buf = "";
      continue;
    }
    buf += c;
  }
  return { topLevelRules, layerOfClass, firstLayerStmt };
}

describe("子步 3 · 层真包进去（对构建产物）", () => {
  it("每个窗口：第一份 CSS 资产以层声明开头，且那句与 layers.css 逐字同序", () => {
    // 先剥注释：layers.css 的头注里逐字写着 `@layer a, b, c;` 这句示意（第一版就咬到了它）
    const layersSrc = readFileSync(resolve(REPO_ROOT, "src/frontend/ui/styles/layers.css"), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
    const src = /@layer\s+([^;{]+);/.exec(layersSrc);
    expect(src, "layers.css 里没有 `@layer …;` 声明").toBeTruthy();
    const want = (src?.[1] ?? "").split(",").map((x) => x.trim());
    for (const w of Object.values(WINDOWS)) {
      const assets = BUILT_CSS_BY_HTML[w.html];
      expect(assets.length, `${w.html} 的构建产物一份 CSS 都没链 —— 下面几条零命中地绿`).toBeGreaterThan(0);
      const first = builtLayerScan(assets[0]).firstLayerStmt ?? "";
      expect(first.replace(/^@layer\s+/, "").split(",").map((x) => x.trim()), `${w.html} 第一份资产的层声明`).toEqual(want);
      expect(assets[0].trimStart().startsWith("@layer"), `${w.html} 第一份资产不是以层声明开头 —— 先出现的层名会抢走次序`).toBe(true);
    }
  });

  it("🔴 每个窗口的构建 CSS 里，一条无层规则都没有（含第三方）", () => {
    for (const w of Object.values(WINDOWS)) {
      const scans = BUILT_CSS_BY_HTML[w.html].map(builtLayerScan);
      const classes = scans.reduce((n, s) => n + s.layerOfClass.size, 0);
      expect(classes, `${w.html} 的构建 CSS 一个类都没扫到 —— 零命中不作数`).toBeGreaterThan(200);
      expect(scans.flatMap((s) => s.topLevelRules), `${w.html} 的构建产物里有无层规则 —— 它们压过所有有层的`).toEqual([]);
    }
  });

  it("正控：第三方的类（hljs / katex）在主窗与 viewer 的产物里存在，且**只**在 vendor 层里", () => {
    for (const html of ["index.html", "viewer.html"]) {
      const merged = new Map<string, Set<string>>();
      for (const s of BUILT_CSS_BY_HTML[html].map(builtLayerScan))
        for (const [c, ls] of s.layerOfClass) merged.set(c, new Set([...(merged.get(c) ?? []), ...ls]));
      const vendorish = [...merged].filter(([c]) => c === "hljs" || c.startsWith("hljs-") || c === "katex" || c.startsWith("katex-"));
      expect(vendorish.length, `${html}：产物里没有 hljs / katex 的类 —— 本条对着空气`).toBeGreaterThan(20);
      // 我们自己也写了几条带 `hljs` / `katex-display` 的覆盖（在 components 层），所以判的是「第三方那份在 vendor 里」：
      // 每个这一族的类至少有一处在 vendor 层，且**没有**任何一处无层
      const bad = vendorish.filter(([, ls]) => ls.has("<无层>")).map(([c]) => c);
      expect(bad, `${html}：这些第三方类出现在无层规则里 —— vite.config.ts 的 vendor 插件没生效`).toEqual([]);
      const inVendor = vendorish.filter(([, ls]) => ls.has("vendor")).length;
      expect(inVendor, `${html}：第三方类一个都不在 vendor 层里`).toBeGreaterThan(20);
    }
  });
});

describe("图标：打进产物的只有用到的那几个", () => {
  const ICON_TS = readFileSync(resolve(REPO_ROOT, "src/frontend/ui/kit/icon.ts"), "utf8");
  /** 登记表里导入的 svg 文件名。 */
  const registered = [...ICON_TS.matchAll(/@phosphor-icons\/core\/assets\/(?:regular|fill)\/([a-z0-9-]+)\.svg\?raw/g)].map((m) => m[1]);
  /** 登记表的名字（`SVG` 那张表的键）。 */
  const names = [...(/const SVG = \{([^}]*)\}/.exec(ICON_TS)?.[1] ?? "").matchAll(/^\s*(\w+):/gm)].map((m) => m[1]);

  it("三扇窗的产物里 Phosphor 的 svg == 登记表导入的那几份（整包没进来）", () => {
    const built = new Set<string>();
    for (const win of Object.keys(WINDOWS) as Win[])
      for (const id of CLOSURES[win].modules) {
        const m = /@phosphor-icons\/core\/assets\/(?:regular|fill)\/([a-z0-9-]+)\.svg/.exec(id);
        if (m) built.add(m[1]);
        expect(id.includes("@phosphor-icons/core/dist"), `整包进了产物：${id}`).toBe(false);
      }
    expect(registered.length, "登记表一个导入都没切到 —— 量具坏了").toBeGreaterThan(0);
    expect([...built].sort()).toEqual([...new Set(registered)].sort());
  });

  it("登记的每个名字都有人用，用到的都登记了", () => {
    // 用到 ＝ 导入图标件的生产文件里以字面量写出那个名字（`icon("x")`、按态取名的表、`icon: "x"` 这几形都是字面量）。
    const used = new Set<string>();
    const importsIcon = /from "(?:\.\.?\/)+(?:kit\/)?icon"/;
    for (const f of productionTsFiles()) {
      if (f.file.endsWith("kit/icon.ts") || !importsIcon.test(f.text)) continue;
      for (const m of f.text.matchAll(/"(\w+)"/g)) if (names.includes(m[1])) used.add(m[1]);
    }
    const called = new Set<string>();
    for (const f of productionTsFiles()) for (const m of f.text.matchAll(/\bicon\(\s*"(\w+)"/g)) called.add(m[1]);
    expect(names.length).toBe(registered.length);
    expect([...used].sort()).toEqual([...names].sort());
    expect([...called].filter((c) => !names.includes(c)), "叫了没登记的名字").toEqual([]);
  });
});

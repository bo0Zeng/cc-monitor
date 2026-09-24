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
 *   `tasks-panel.ts` 等，`tabs.ts` 不在本轮写区；〔F7b〕老 SFTP 面板已退役，那一条不在了）。本文件只钉「不许有」清单，
 *   不钉「只许有」—— 钉全集会让每一次 `tabs.ts` 的正常改动都红在一个与拆分无关的数上。
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { build, type Rollup } from "vite";
import { describe, it, expect, beforeAll } from "vitest";
import { stripCodeComments } from "./evidence/S25-class-ledger.ts";
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
  // 〔U2 · 第三波〕`tabs.ts` 拆成 13 份之后，「设置窗里没有 tab 管理」要对每一份都成立 ——
  //   只钉 `tabs.ts` 一个名字，某一份被设置窗间接带进来时这里看不见。正控照旧（每一份都得在主窗 / viewer 的闭包里命中）。
  { pat: "src/tab-model.ts", what: "tab 管理（tab 的形状与标题）" },
  { pat: "src/tab-store.ts", what: "tab 管理（会话状态账 / store）" },
  { pat: "src/tab-router.ts", what: "tab 管理（路由）" },
  { pat: "src/tab-session-facts.ts", what: "tab 管理（从记录里抽事实）" },
  { pat: "src/tab-stream-view.ts", what: "tab 管理（实时流视图）" },
  { pat: "src/tab-bar-view.ts", what: "tab 管理（tab 栏视图）" },
  { pat: "src/tab-bar-drag.ts", what: "tab 管理（拖拽）" },
  { pat: "src/tab-drop.ts", what: "tab 管理（落点算术）" },
  { pat: "src/tab-bar-prefs.ts", what: "tab 管理（集合 / 固定 / 顺序落盘）" },
  { pat: "src/tab-menu.ts", what: "tab 管理（右键菜单项）" },
  { pat: "src/tab-context-menu.ts", what: "tab 管理（右键菜单控件）" },
  { pat: "src/tab-session-actions.ts", what: "tab 管理（会话动作）" },
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

// ═══════════════════════════ 子步 2：CSS 按窗口拆 ═══════════════════════════
//
// 每个窗口的 CSS 清单**只有一份**：它的 html 里 `<link rel="stylesheet">` 的列表（按序）。
// 下面三条都从那份清单出发：
//   ① 清单自洽 —— `layers.css` 排第一（它定层的先后）、每个 `src/**/*.css` 至少被一个窗口链到；
//   ② 🔴 零命中（对**构建产物**的 CSS）—— 设置窗的 CSS 里没有高亮 / 数学 / tab / 卡片 / 流；
//      viewer 的 CSS 里没有设置面板 / 历史视图 / 全景 / 网格 / 命令栏；
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
  Object.keys(import.meta.glob("../src/**/*.css")).map((p) => {
    const rel = p.replace(/^\.\.\//, "");
    return [rel, readFileSync(resolve(REPO_ROOT, rel), "utf8")];
  }),
);

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
      if (c === "}") depthInRule = false;
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
  Object.entries(CSS_SOURCES).map(([f, css]) => [f, selectorsOf(css)]),
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
const CSS_FORBIDDEN_IN_VIEWER = ["settings-panel", "settings-body", "kb-editor", "history-view", "history-entry", "panorama-canvas", "grid-monitor-cell", "command-bar", "mcp-", "accounts-row"];
function familyHits(classes: Set<string>, fam: string): string[] {
  return [...classes].filter((c) => (fam.endsWith("-") ? c.startsWith(fam) : c === fam || c.startsWith(`${fam}-`)));
}

describe("子步 2 · CSS 按窗口拆（清单 ＝ 各 html 的 <link> 列表）", () => {
  it("清单自洽：每个窗口先链 layers.css；每份 src CSS 至少被一个窗口链到；链到的文件都存在", () => {
    const all = new Set<string>();
    for (const w of Object.values(WINDOWS)) {
      const links = linksOf(w.html);
      expect(links[0], `${w.html} 的第一条样式表不是 layers.css —— 层的先后会随窗口而变`).toBe("src/styles/layers.css");
      for (const l of links) {
        expect(CSS_SOURCES[l], `${w.html} 链了一份不存在的样式表 ${l}`).toBeDefined();
        all.add(l);
      }
    }
    expect(Object.keys(CSS_SOURCES).length, "glob 没扫到 CSS —— 下面那条零命中地绿").toBeGreaterThan(5);
    const empty = Object.entries(CSS_SOURCES).filter(([, t]) => t.trim().length < 50).map(([f]) => f);
    expect(empty, "这些 CSS 读出来是空的 —— 读法坏了（`?raw` 在 vitest 里就是这么坏的）").toEqual([]);
    expect(Object.keys(CSS_SOURCES).filter((f) => !all.has(f)), "这些 CSS 没有任何窗口链它 —— 写了等于没写").toEqual([]);
  });

  it("🔴 设置窗的构建产物 CSS：没有高亮 / 数学 / tab / 卡片 / 流 / 折叠块", () => {
    const got = builtClasses("settings");
    expect(got.size, "设置窗的构建 CSS 一个类都没读到 —— 零命中不作数").toBeGreaterThan(200);
    const leaked = CSS_FORBIDDEN_IN_SETTINGS.flatMap((f) => familyHits(got, f));
    expect(leaked, "设置窗的 CSS 里混进了渲染栈 / tab 管理的样式").toEqual([]);
  });

  it("🔴 viewer 的构建产物 CSS：没有设置面板 / 历史视图 / 全景 / 网格 / 命令栏", () => {
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
        .filter((l) => !["src/styles/layers.css"].includes(l))
        .filter((drop) => missingIn(win, links.filter((l) => l !== drop)).length === 0);
      // `layers.css` 只有一句层声明、没有规则，不在这里判（它的住址由「清单自洽」那条钉：必须排第一）。
      // `reset.css` / `tokens.css` 只有 `*` / `html` / `:root` 这种无类选择器 —— 它们在任何窗口都挂得上，
      // 所以摘掉同样会红（第一版以为这两份看不见，现打是看得见的）。
      expect(blind, `${win}：摘掉这些文件，完整性判据照样绿`).toEqual([]);
    }
  }, TIMEOUT_MS);
});

// ═══════════════════════════ 子步 3：层真包进去（对构建产物）═══════════════════════════
//
// 源码那一侧（每条规则都在层里、层名不拼错、声明次序）住 `tests/css-ledger.vitest.ts` 格 ⑤。
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
    const layersSrc = readFileSync(resolve(REPO_ROOT, "src/styles/layers.css"), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
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

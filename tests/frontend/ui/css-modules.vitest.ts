// @vitest-environment node
/**
 * 〔UC2〕**渐进式 CSS Modules 的判据**（`设计/41 §1` 选项 D · `§12` 件 10：「新组件一律 Modules；旧的动到哪个迁哪个」）。
 *
 * 件 10 批准了却「零落地、零判据」—— 纪律不在执行链上等于不存在。本文件把它能机判的那几半接进 `npm test`：
 *
 * | 格 | 判什么 | 形态 |
 * |---|---|---|
 * | M① | 全局（非 `.module.css`）样式文件的集合 == `GLOBAL_STYLESHEETS` | **两向相等** ⇒ 新长一份全局样式文件就红：新组件的样式要写成 `.module.css` |
 * | M② | 每份 `.module.css` 有逐文件类型 `.module.d.css.ts`，全文 == 由它的类名集合渲染出的规范文本；反方向没有孤儿 | 两向、逐字 |
 * | M③ | `src` 里 `.module.css` 只经**默认导入**用（`import s from "./x.module.css"`）；被导入的集合 == module 集合 | 零命中 ＋ 两向 |
 * | M④ | CSS → TS：module 里每个类都被导入方以 `s.<类>` / `s["<类>"]` 取过；导入对象只许这么用（不许整个传走、不许变量下标） | 两向相等 |
 * | M⑤ | `tsc` 真吃到逐文件类型：用**仓里的 tsconfig** 编探针，`s.<真类>` 零诊断、`s.<拼错>` 恰一条 TS2339；同一探针关掉 `allowArbitraryExtensions` 零诊断 | 正反两控 |
 *
 * 产物那一侧（类名真哈希、JS 真用上哈希名、module 规则排在全局规则之后）住 `tests/frontend/ui/entry-graphs.vitest.ts`
 * 「CSS Modules 在构建产物里」那一组 —— 它已经真跑一遍 `vite build`，不在这里再跑第二遍。
 *
 * ## 为什么 M⑤ 非有不可
 *
 * 现打：只接上 `vite/client` 的类型时，`import s from "./x.module.css"` 的类型是通配的
 * `{ readonly [key: string]: string }`，`s.nope` 在 `tsc --noEmit` 下 **0 错**；vitest 里导入对象是个 Proxy，
 * **任意键**都返回一个串 ⇒ 「写错类名变编译错误」这条卖点在默认配置下**不成立**，而且没有任何一处会叫。
 * 逐文件类型（`x.module.d.css.ts`）要 `tsconfig.json` 开 `allowArbitraryExtensions` 才会被认；不开时 tsc
 * **静默**落回通配。所以本格同时钉「开着时拼错会红」与「关掉就不红」—— 后一条证明红是那一项买的，
 * 不是探针自己别的地方写错了。
 *
 * ## 买不到（照实记）
 *
 * - 「**旧的动到哪个迁哪个**」机判不了（「这次改动算不算动了那个组件」没有机械定义）。靠评审与下一次动它的人；
 *   M① 只保证方向单调：全局样式文件不许新增（要新增就改登记表并写理由，审的人看得见）。
 * - M③ / M④ 走 TS 的 AST（不是正则）；M④ 按名字认导入对象、不做作用域分析 —— 同一份文件里另有局部变量与它同名会
 *   **假红**（吵闹，不静默）。
 * - M③ / M④ 只看 `src`；`tests/` 里的导入不算「有人用」（与类名账本同一条口径：测试不是用户）。
 * - 视觉：示范组件迁完「长得一样」没有目视（本机没有图形会话）；级联上的论证写在 `usage-hud.module.css` 头注与
 *   `调研/第四波记录/UC2.md §1.4`，次序由 entry-graphs 钉。
 */
import { existsSync, readFileSync } from "node:fs";
import { dirname, join, posix, resolve } from "node:path";
import ts from "typescript";
import { describe, it, expect } from "vitest";
import { buildLedger, type Ledger } from "../../evidence/S25-class-ledger.ts";
import { toPosix } from "../../test-support/posix-path.ts";
import { REPO_ROOT } from "../../test-support/repo-root.ts";

/**
 * ★ **全局样式文件登记表**（M①）。今天 10 份，全是三入口拆分那一拍按「哪几个窗口要它」切出来的（`设计/41 §9`）。
 *
 * 🔴 **只许缩**：一份全局样式整份迁成 module 之后删掉对应条目（两向相等会逼你删）；
 * 要新增一份全局样式文件 ⇒ 先问「它为什么不能是 `.module.css`」，答得出来再加一条并写清理由。
 * ⚠ 并发：同波别的路新建全局样式文件时，本表会在合并那一拍红 —— 那是本条要的红，不是假红。
 */
const GLOBAL_STYLESHEETS: readonly { file: string; why: string }[] = [
  { file: "src/frontend/ui/styles/layers.css", why: "层声明，三窗第一个链（`设计/41 §3`）" },
  { file: "src/frontend/ui/styles/reset.css", why: "最小重置，元素选择器（`§4`）" },
  { file: "src/frontend/ui/styles/tokens.css", why: "`:root` 令牌（`§2`）" },
  { file: "src/frontend/ui/styles/layout.css", why: "`#app` 网格与格子认领（id 选择器）" },
  { file: "src/frontend/ui/styles/shared.css", why: "三窗共用 chrome（旧，待迁）" },
  { file: "src/frontend/ui/styles.css", why: "tab 管理 ＋ 渲染栈（旧，待迁；路径被仓外判据与真引擎探针按名读）" },
  { file: "src/frontend/ui/styles/settings-shared.css", why: "设置表单原语 ＋ 账号选单（旧，待迁）" },
  { file: "src/frontend/ui/styles/main.css", why: "主窗 chrome（旧，待迁）" },
  { file: "src/frontend/ui/styles/settings.css", why: "设置窗 chrome（旧，待迁）" },
  { file: "src/frontend/ui/styles/viewer.css", why: "viewer 窗 chrome（旧，待迁）" },
];

let cached: Ledger | null = null;
function ledger(): Ledger {
  cached ??= buildLedger(REPO_ROOT);
  return cached;
}

const isModule = (f: string): boolean => f.endsWith(".module.css");
/** `src/x.module.css` → `src/x.module.d.css.ts`（TS `allowArbitraryExtensions` 找声明的规则：`{basename}.d.{ext}.ts`）。 */
const declOf = (css: string): string => css.replace(/\.css$/, ".d.css.ts");
const IDENT = /^[A-Za-z_$][\w$]*$/;

/** 逐文件类型的**规范文本**：由类名集合唯一决定（排序、合法标识符裸写、其余带引号）。 */
function renderModuleDecl(cssRel: string, classes: Iterable<string>): string {
  const base = posix.basename(cssRel);
  const keys = [...new Set(classes)].sort();
  return (
    `// 由 \`tests/frontend/ui/css-modules.vitest.ts\` 对拍：本文件 == 从 \`${base}\` 渲染出的规范文本（逐字）。\n` +
    "declare const classes: {\n" +
    keys.map((k) => `  readonly ${IDENT.test(k) ? k : JSON.stringify(k)}: string;\n`).join("") +
    "};\n" +
    "export default classes;\n"
  );
}

/**
 * 一份代码里对 `.module.css` 的全部导入（走 TS 的 AST，不靠正则）。
 * `bad` ＝ 提到 `.module.css` 的字符串里，**不是**「`import <标识符> from "./….module.css"`（无具名 / 命名空间绑定）」
 * 那一形的每一处：副作用导入、具名 / 命名空间导入、动态 `import()`、`export … from`、普通字符串都算。
 */
function moduleImportsOf(codeRel: string, text: string): { ok: { id: string; css: string }[]; bad: string[] } {
  const sf = ts.createSourceFile(codeRel, text, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS);
  const ok: { id: string; css: string }[] = [];
  const bad: string[] = [];
  const visit = (n: ts.Node): void => {
    if ((ts.isStringLiteral(n) || ts.isNoSubstitutionTemplateLiteral(n)) && /\.module\.css$/.test(n.text)) {
      const d = n.parent;
      const clause = ts.isImportDeclaration(d) && d.moduleSpecifier === n ? d.importClause : undefined;
      if (clause && clause.name && !clause.namedBindings && !clause.isTypeOnly && /^\.{1,2}\//.test(n.text)) {
        ok.push({ id: clause.name.text, css: posix.normalize(posix.join(posix.dirname(codeRel), n.text)) });
      } else {
        bad.push(`${codeRel}:${sf.getLineAndCharacterOfPosition(n.getStart()).line + 1} ${d.getText().slice(0, 80)}`);
      }
    }
    ts.forEachChild(n, visit);
  };
  visit(sf);
  return { ok, bad };
}

/**
 * 导入对象 `id` 在一份代码里被取过的类名；`loose` ＝ `id` 的其余出现（整个传走、变量下标、解构……）——
 * 那些写法 tsc 管不到类名，一律不许。⚠ 按名字认，不做作用域分析：同一份文件里另有局部变量也叫 `id` 会**假红**（吵闹）。
 */
function usesOf(text: string, id: string): { keys: Set<string>; loose: number } {
  const sf = ts.createSourceFile("x.ts", text, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS);
  const keys = new Set<string>();
  let loose = 0;
  const visit = (n: ts.Node): void => {
    if (ts.isIdentifier(n) && n.text === id) {
      const p = n.parent;
      if (ts.isImportClause(p)) {
        /* 导入本身 */
      } else if (ts.isPropertyAccessExpression(p) && p.expression === n) keys.add(p.name.text);
      else if (ts.isElementAccessExpression(p) && p.expression === n && ts.isStringLiteralLike(p.argumentExpression))
        keys.add(p.argumentExpression.text);
      else if (ts.isPropertyAccessExpression(p) && p.name === n) {
        /* 别人的属性碰巧同名（`x.s`），不是它 */
      } else loose++;
    }
    ts.forEachChild(n, visit);
  };
  visit(sf);
  return { keys, loose };
}

function moduleFiles(led: Ledger): string[] {
  return led.cssFiles.filter(isModule).sort();
}

describe("〔UC2〕M① 全局样式文件 == 登记表（新样式一律 .module.css）", () => {
  it("两向相等", () => {
    const led = ledger();
    const got = led.cssFiles.filter((f) => !isModule(f)).sort();
    const want = GLOBAL_STYLESHEETS.map((g) => g.file).sort();
    expect(
      got,
      "全局样式文件集合与 `GLOBAL_STYLESHEETS` 不等。\n" +
        "★ 多出来的：新样式请写成 `<组件>.module.css`（`设计/41` 件 10：新组件一律 Modules）。真要一份全局文件，先在登记表里写清为什么不能是 module。\n" +
        "★ 少了的：那份全局样式迁走 / 删掉了 —— 把登记表里那一条一起删（本表只许缩）。",
    ).toEqual(want);
    console.log(`  ok   UC2-M①  ${got.length} 份全局样式文件 == 登记表；module ${moduleFiles(led).length} 份`);
  });
});

describe("〔UC2〕M② 逐文件类型 == 类名集合（逐字）", () => {
  it("每份 .module.css 都有 .module.d.css.ts，全文 == 规范文本；没有孤儿声明", () => {
    const led = ledger();
    const mods = moduleFiles(led);
    expect(mods.length, "一份 `.module.css` 都没有 —— 本组零命中地绿（件 10 的示范被删了？）").toBeGreaterThan(0);
    const wrong: string[] = [];
    for (const f of mods) {
      const classes = [...(led.moduleClasses.get(f)?.keys() ?? [])];
      expect(classes.length, `${f} 里一个类都没抽到 —— 类型会是空对象，下面那条恒等照样成立`).toBeGreaterThan(0);
      const want = renderModuleDecl(f, classes);
      const p = resolve(REPO_ROOT, declOf(f));
      const got = existsSync(p) ? readFileSync(p, "utf8") : "<不存在>";
      if (got !== want) wrong.push(`── ${declOf(f)} 应为（照抄）：\n${want}── 现为：\n${got}`);
    }
    expect(wrong, "逐文件类型与 CSS 的类名集合漂了 —— tsc 会放过写错的名、或拦下写对的名").toEqual([]);
    const orphans = led.arbitraryDecls.filter((d) => !mods.map(declOf).includes(d));
    expect(orphans, "这些 `.d.<扩展名>.ts` 没有对应的 `.module.css` —— 孤儿声明").toEqual([]);
    console.log(`  ok   UC2-M②  ${mods.length} 份 module 的逐文件类型逐字对上；孤儿 0`);
  });

  it("死值验：规范文本对类名集合敏感（多一个 / 少一个类，文本都变）", () => {
    const a = renderModuleDecl("src/x.module.css", ["chip", "high"]);
    expect(renderModuleDecl("src/x.module.css", ["high", "chip", "chip"])).toBe(a);
    expect(renderModuleDecl("src/x.module.css", ["chip"])).not.toBe(a);
    expect(renderModuleDecl("src/x.module.css", ["chip", "high", "low"])).not.toBe(a);
    expect(renderModuleDecl("src/x.module.css", ["a-b"])).toContain('readonly "a-b": string;');
  });
});

describe("〔UC2〕M③ ④ 只经默认导入用；每个类都有人取（CSS → TS）", () => {
  it("导入形只有默认导入；被导入集合 == module 集合；每份 module 的类 == 导入方取过的键", () => {
    const led = ledger();
    const mods = moduleFiles(led);
    const bad: string[] = [];
    const importers = new Map<string, { code: string; id: string }[]>();
    for (const rel of led.codeFiles) {
      // 每份都过一遍 AST（不做子串预筛：`scanning-guard-registry` 的裸 `.includes` 棘轮不许再多一处）
      const r = moduleImportsOf(rel, readFileSync(resolve(REPO_ROOT, rel), "utf8"));
      bad.push(...r.bad);
      for (const imp of r.ok) {
        const list = importers.get(imp.css) ?? [];
        list.push({ code: rel, id: imp.id });
        importers.set(imp.css, list);
      }
    }
    expect(bad, "`.module.css` 只许 `import s from \"./x.module.css\"` 这一形（副作用导入 / 具名 / 命名空间 / 动态导入都绕开了逐文件类型）").toEqual([]);
    expect([...importers.keys()].sort(), "被导入的 module 集合 != 盘上的 module 集合（没人导入的 module 进不了产物；导入了不存在的 module 构建会红）").toEqual(mods);

    const drift: string[] = [];
    let judged = 0;
    for (const f of mods) {
      const defined = [...(led.moduleClasses.get(f)?.keys() ?? [])].sort();
      const used = new Set<string>();
      for (const { code, id } of importers.get(f) ?? []) {
        const u = usesOf(readFileSync(resolve(REPO_ROOT, code), "utf8"), id);
        if (u.loose !== 0) drift.push(`${code}：导入对象 \`${id}\` 有 ${u.loose} 处不是 \`${id}.x\` / \`${id}["x"]\` 形（整个传走或变量下标 ⇒ tsc 管不到类名）`);
        for (const k of u.keys) used.add(k);
      }
      const usedSorted = [...used].sort();
      if (JSON.stringify(usedSorted) !== JSON.stringify(defined))
        drift.push(`${f}：定义了 [${defined.join(", ")}]，导入方取了 [${usedSorted.join(", ")}]（定义了没人取 ＝ 死规则）`);
      judged += defined.length;
    }
    expect(drift, "module 的类名两个方向没对上").toEqual([]);
    expect(judged, "一个 module 类都没判到 —— 零命中地绿").toBeGreaterThan(0);
    console.log(`  ok   UC2-M③④  ${mods.length} 份 module · ${judged} 个类，两向对上、只经默认导入`);
  });

  it("死值验：导入形与取用形的量具认得出坏形状", () => {
    expect(moduleImportsOf("src/a.ts", 'import s from "./a.module.css";').bad).toEqual([]);
    expect(moduleImportsOf("src/a.ts", 'import "./a.module.css";').bad).toHaveLength(1);
    expect(moduleImportsOf("src/a.ts", 'import * as s from "./a.module.css";').bad).toHaveLength(1);
    expect(moduleImportsOf("src/a.ts", 'import { chip } from "./a.module.css";').bad).toHaveLength(1);
    expect(moduleImportsOf("src/a.ts", 'const m = await import("./a.module.css");').bad).toHaveLength(1);
    expect(moduleImportsOf("src/v/a.ts", 'import s from "../a.module.css";').ok).toEqual([{ id: "s", css: "src/a.module.css" }]);
    const ok = usesOf('import s from "./a.module.css";\nx.className = `b ${s.chip}`; y.toggle(s["high"]); z("s is fine");', "s");
    expect([...ok.keys].sort()).toEqual(["chip", "high"]);
    expect(ok.loose).toBe(0);
    expect(usesOf('import s from "./a.module.css";\nf(s);', "s").loose).toBe(1);
    expect(usesOf('import s from "./a.module.css";\nconst k = "chip"; g(s[k]);', "s").loose).toBe(1);
  });
});

describe("〔UC2〕M⑤ tsc 真吃到逐文件类型（仓里的 tsconfig）", () => {
  /** 在每份 module 旁边放一个虚拟探针：`s.<真类>` 与 `s.<拼错>` 各取一次，返回探针上的诊断码。 */
  function probe(override: ts.CompilerOptions): { file: string; codes: number[]; texts: string[] }[] {
    const cfgPath = resolve(REPO_ROOT, "tsconfig.json");
    const raw = ts.readConfigFile(cfgPath, ts.sys.readFile);
    const parsed = ts.parseJsonConfigFileContent(raw.config, ts.sys, REPO_ROOT);
    const options = { ...parsed.options, ...override, noEmit: true };
    const led = ledger();
    const probes = new Map<string, string>();
    for (const f of moduleFiles(led)) {
      const real = [...(led.moduleClasses.get(f)?.keys() ?? [])].sort()[0];
      // tsc 把喂进来的文件名一律规整成 `/` 再回调 host；键不同形（Windows 上 `join` 吐 `\`）⇒ 探针一份也认不出、零诊断地红。
      const abs = toPosix(join(dirname(resolve(REPO_ROOT, f)), `__uc2_probe_${probes.size}__.ts`));
      const key = IDENT.test(real) ? `.${real}` : `[${JSON.stringify(real)}]`;
      probes.set(
        abs,
        `import s from "./${posix.basename(f)}";\nexport const ok: string = s${key};\nexport const typo: string = s.uc2TypoProbe;\n`,
      );
    }
    const host = ts.createCompilerHost(options);
    const baseGet = host.getSourceFile.bind(host);
    host.getSourceFile = (name, lang, onErr, create) =>
      probes.has(toPosix(name)) ? ts.createSourceFile(name, probes.get(toPosix(name))!, lang) : baseGet(name, lang, onErr, create);
    const baseExists = host.fileExists.bind(host);
    host.fileExists = (name) => probes.has(toPosix(name)) || baseExists(name);
    const program = ts.createProgram([...probes.keys(), resolve(REPO_ROOT, "src/frontend/ui/vite-env.d.ts")], options, host);
    return [...probes.keys()].map((file) => {
      const diags = program.getSemanticDiagnostics(program.getSourceFile(file));
      return {
        file,
        codes: diags.map((d) => d.code),
        texts: diags.map((d) => ts.flattenDiagnosticMessageText(d.messageText, "\n")),
      };
    });
  }

  it("开着 allowArbitraryExtensions（仓里的真值）：真类零诊断、拼错恰一条 TS2339", () => {
    const res = probe({});
    expect(res.length, "没有探针 —— 没有 module，本条零命中地绿").toBeGreaterThan(0);
    for (const r of res) {
      expect(r.codes, `${r.file}：${r.texts.join(" | ")}`).toEqual([2339]);
      expect(r.texts[0]).toContain("uc2TypoProbe");
    }
    console.log(`  ok   UC2-M⑤  ${res.length} 份 module：tsc 对拼错的类名恰报 TS2339，对真类零诊断`);
  }, 60_000);

  it("反控：同一探针关掉 allowArbitraryExtensions ⇒ 零诊断（tsc 落回 vite 的通配类型 —— 红是那一项买的）", () => {
    const res = probe({ allowArbitraryExtensions: false });
    for (const r of res) expect(r.codes, `${r.file}：${r.texts.join(" | ")}`).toEqual([]);
  }, 60_000);
});

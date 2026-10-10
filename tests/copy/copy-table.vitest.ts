/**
 * CP2a · 抽表机制（四个决定）的判据。表住 `src/shared/copy/table.json`，
 * 取文口住 `src/frontend/ui/copy-table.ts::copyText`。
 *
 * # 判什么
 *
 * 1. **表 ↔ 引用两向相等**（本拍的主判据）：表里每个 key 在生产代码里至少被 `copyText("…")`
 *    引用一次；生产代码里每个 `copyText("…")` 的 key 都在表里。两侧异源：一侧是 JSON，
 *    一侧是用 TypeScript 编译器把 `src/**.ts` 真解析出来的调用点（不是 grep）。
 * 2. **调用点的参数 == 表里的 args**（决定 3）：第二个实参必须是对象字面量，
 *    属性名集合与表里 `args` 相等。
 * 3. **绕不过去**：key 不是字面量、参数对象里有展开、把 `copyText` 改名导入或当值传走 ⇒ 红
 *    —— 否则那一处引用本判据看不见，两向相等就是在一个缺角的集合上成立的。
 * 4. **表的形状**：key 三段式 · kind 六档 · 占位符只许具名 · `args` 与 `zh` 里的占位符两向相等。
 * 5. **`kind == "title"` 的条数不为 0**（决定 4 的反空真：全填成 body 会让 R5 静默空转）。
 *
 * 标题以 `[C-xx]` 开头的两条是 `src/shared/copy/rules.json` 里那两条「机检」规矩的实现
 * （`copy-rules.vitest.ts` 按这个前缀做规矩 ↔ 实现两向对拍，改标题要连规矩表一起改）。
 *
 * # 不判什么（诚实段）
 *
 * - **Rust 读口已落地**（`src/frontend/shell/src/copy_table.rs::copy_text`，与 TS 读同一份 JSON）⇒
 *   `src/frontend/shell/src/**.rs` 的 `copy_text("…", &[("名", 值), …])` 调用点也收进「引用」一侧（[`rustRefsIn`]）。
 *   〔墓碑 —— 原话「Rust 一侧没有调用点，本文件也不扫 `.rs`；Rust 读口落地那一拍要把 `.rs` 的调用点加进『引用』那一侧」。〕
 *   ⚠ Rust 那一侧不是编译器解析：按调用形状读（剥掉 `//` 注释后找 `copy_text(`），key 必须是紧跟的字符串字面量、
 *   参数必须是 `&[…]` 数组字面量、每一项 `("名", …)` 的名是字符串字面量 —— 其余写法一律报「绕过」，不放过。
 * - **不判表里的文案写得好不好**：规矩住 `src/shared/copy/rules.json`，判据住 `copy-rules.vitest.ts`。
 * - **不判全集**：没抽进表的文案今天仍是散在源码里的字面量，归普查（K-T68）与 CP1 台账管。
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import ts from "typescript";
import { describe, expect, it } from "vitest";

import { copyText, type CopyKey } from "../../src/frontend/ui/copy-table.ts";
import { productionTsFiles } from "../test-support/production-sources.ts";
import { REPO_ROOT } from "../test-support/repo-root.ts";
import { loadTable, NAMED_PH, type Ref, type Table } from "./copy-support.ts";
import { KT_HOME, kotlinRefsIn, kotlinRefsOfTree } from "./kotlin-refs.ts";
import { RS_DEFINITIONS, RS_HOME, rustRefsIn, rustRefsOfTree } from "./rust-refs.ts";

// 第六档 aria：只进 aria-label 的无障碍名。
const KINDS = new Set(["title", "control", "action", "body", "error", "aria"]);
/**
 * 界面角色（文案新写法 `规范.md` §2 按角色分节）：闭集 ＋ 每个角色许配的 kind。
 * 两向相等：表里出现的（role · kind）== 这张搭配表里的格（多一格 ⇒ 搭错了；少一格 ⇒ 这张表写了没人用的格，删掉）。
 */
export const ROLE_KINDS: Readonly<Record<string, readonly string[]>> = {
  按钮: ["action"],
  菜单项: ["action"],
  标题: ["title"],
  标签: ["control"],
  占位: ["control"],
  状态: ["body"],
  说明: ["body"],
  悬停: ["body"],
  toast: ["body"],
  报错: ["error"],
  原因格: ["error"],
  确认框: ["title", "body", "action"],
  空态: ["body"],
  进度: ["body"],
  命令行: ["body", "error"],
  读屏: ["aria"],
  片段: ["body"],
  图标: ["body"],
};

/** role 的问题：没写 · 不在闭集 · 与 kind 搭不上；以及搭配表里有、表里一条都没用到的格（`unused`）。 */
export function roleProblems(table: Table): { bad: string[]; unused: string[] } {
  const bad: string[] = [];
  const seen = new Set<string>();
  for (const [key, e] of Object.entries(table)) {
    const allowed = e.role === undefined ? undefined : ROLE_KINDS[e.role];
    if (typeof e.role !== "string" || e.role === "") bad.push(`${key}：没写 role`);
    else if (!allowed) bad.push(`${key}：role「${e.role}」不在闭集里`);
    else if (!allowed.includes(e.kind)) bad.push(`${key}：role「${e.role}」不配 kind「${e.kind}」（许配 ${allowed.join(" / ")}）`);
    else seen.add(`${e.role} · ${e.kind}`);
  }
  const unused = Object.entries(ROLE_KINDS)
    .flatMap(([r, ks]) => ks.map((k) => `${r} · ${k}`))
    .filter((cell) => !seen.has(cell));
  return { bad, unused };
}
const KEY_RE = /^[a-z][A-Za-z0-9]*\.[a-z][A-Za-z0-9]*\.[a-z][A-Za-z0-9]*$/;
/** 取文口自己住的文件：它里头的 `copyText` 是定义，不是引用。 */
const HOME = "src/frontend/ui/copy-table.ts";
const FN = "copyText";

/** 表自己的形状问题。 */
export function tableProblems(table: Table): string[] {
  const p: string[] = [];
  for (const [key, e] of Object.entries(table)) {
    if (!KEY_RE.test(key)) p.push(`${key}：key 不是 <面>.<场景>.<变体> 三段式`);
    if (!KINDS.has(e.kind)) p.push(`${key}：kind「${e.kind}」不在六档里`);
    if (typeof e.zh !== "string" || e.zh.trim() === "") p.push(`${key}：zh 是空的`);
    if (!Array.isArray(e.args)) {
      p.push(`${key}：没写 args（没有参数也要写 []）`);
      continue;
    }
    const zh = e.zh ?? "";
    const named = [...zh.matchAll(NAMED_PH)].map((m) => m[1]);
    // 具名占位符剥掉之后还剩花括号 ⇒ 位置式 `{}` / `{0}` 或者半截括号。
    if (/[{}]/.test(zh.replace(NAMED_PH, ""))) p.push(`${key}：有不具名的占位符或落单的花括号`);
    const a = new Set(e.args);
    const n = new Set(named);
    if (a.size !== e.args.length) p.push(`${key}：args 有重复`);
    for (const x of n) if (!a.has(x)) p.push(`${key}：zh 里有 {${x}}，args 没登记`);
    for (const x of a) if (!n.has(x)) p.push(`${key}：args 登记了 ${x}，zh 里没有这个占位符`);
  }
  return p;
}

/** 用 TypeScript 编译器读一份源码里的全部 `copyText` 调用点。 */
export function refsIn(file: string, text: string): { refs: Ref[]; problems: string[] } {
  const refs: Ref[] = [];
  const problems: string[] = [];
  const sf = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS);
  const at = (n: ts.Node): string => `${file}:${sf.getLineAndCharacterOfPosition(n.getStart()).line + 1}`;
  const visit = (n: ts.Node): void => {
    if (ts.isImportSpecifier(n) && (n.propertyName ?? n.name).text === FN && n.propertyName)
      problems.push(`${at(n)}：把 ${FN} 改名导入 —— 改了名的调用本判据看不见`);
    if (ts.isIdentifier(n) && n.text === FN && file !== HOME) {
      const parent = n.parent;
      const isCallee = ts.isCallExpression(parent) && parent.expression === n;
      const isImport = ts.isImportSpecifier(parent);
      if (!isCallee && !isImport) problems.push(`${at(n)}：${FN} 被当值用了（不是直接调用）—— 本判据看不见它会被拿去取哪条`);
    }
    if (ts.isCallExpression(n) && ts.isIdentifier(n.expression) && n.expression.text === FN && file !== HOME) {
      const [k, a] = n.arguments;
      if (!k || !(ts.isStringLiteral(k) || ts.isNoSubstitutionTemplateLiteral(k))) {
        problems.push(`${at(n)}：${FN} 的 key 不是字面量`);
      } else {
        const args: string[] = [];
        if (a !== undefined) {
          if (!ts.isObjectLiteralExpression(a)) problems.push(`${at(n)}：${FN} 的参数不是对象字面量`);
          else
            for (const prop of a.properties) {
              if ((ts.isPropertyAssignment(prop) || ts.isShorthandPropertyAssignment(prop)) && !ts.isComputedPropertyName(prop.name))
                args.push(prop.name.getText(sf).replace(/^["']|["']$/g, ""));
              else problems.push(`${at(n)}：${FN} 的参数对象里有展开或计算属性名`);
            }
        }
        refs.push({ file: at(n), key: k.text, args });
      }
    }
    ts.forEachChild(n, visit);
  };
  visit(sf);
  return { refs, problems };
}

/** 表 ↔ 引用的两向差，加上参数对不上的调用点。 */
export function crossCheck(table: Table, refs: Ref[]): { unreferenced: string[]; unknown: string[]; badArgs: string[] } {
  const used = new Set(refs.map((r) => r.key));
  const unreferenced = Object.keys(table).filter((k) => !used.has(k)).sort();
  const unknown = refs.filter((r) => !(r.key in table)).map((r) => `${r.file} ${r.key}`);
  const badArgs = refs
    .filter((r) => r.key in table)
    .filter((r) => [...r.args].sort().join(",") !== [...table[r.key].args].sort().join(","))
    .map((r) => `${r.file} ${r.key}：给了 [${r.args.join(",")}]，表里要 [${table[r.key].args.join(",")}]`);
  return { unreferenced, unknown, badArgs };
}

describe("CP2a · 文案表：形状", () => {
  const table = loadTable();

  it("表读得出条目（反空真：0 条时下面每一条都是空转）", () => {
    expect(Object.keys(table).length).toBeGreaterThan(0);
  });

  it("[C-K1] key 三段式 · kind 六档 · 占位符只许具名 · args 与占位符两向相等", () => {
    expect(tableProblems(table)).toEqual([]);
  });

  it("[C-K2] kind == title 的条数不为 0（全填成 body 会让 R5 静默空转）", () => {
    expect(Object.values(table).filter((e) => e.kind === "title").length).toBeGreaterThan(0);
  });

  it("[C-K4] 每条带 role，取自闭集；role 与 kind 的搭配取自搭配表，搭配表与表里出现的格两向相等", () => {
    const { bad, unused } = roleProblems(table);
    expect(bad, "role 写错或与 kind 搭不上").toEqual([]);
    expect(unused, "搭配表里这几格表里一条都没有 —— 删掉那一格（或那个角色）").toEqual([]);
  });
});

describe("CP2a · 文案表 ↔ 生产代码引用", () => {
  const table = loadTable();
  const files = productionTsFiles("src");
  const all = files
    .filter((f) => new RegExp(`\\b${FN}\\b`).test(f.text))
    .map((f) => refsIn(f.file, f.text));
  // Rust 读口的调用点（射程与读法住 `rust-refs.ts`：常驻后端 · 两个前端 · 子 crate；取文实现本身是定义，不进人群）。
  const { files: rsFiles, refs: rsRefs, problems: rsProblems } = rustRefsOfTree();
  // 手机端 Kotlin 读口的调用点（射程与读法住 `kotlin-refs.ts`：`src/mobile/*/src/main/**.kt`）。
  const { files: ktFiles, refs: ktRefs, problems: ktProblems } = kotlinRefsOfTree();
  const refs = [...all.flatMap((x) => x.refs), ...rsRefs, ...ktRefs];
  const problems = [...all.flatMap((x) => x.problems), ...rsProblems, ...ktProblems];

  it("正控（Kotlin）：扫到了手机端生产源码、Kotlin 取文口自己，也读到了 Kotlin 调用点", () => {
    expect(ktFiles.length, "一个手机端生产 .kt 都没扫到").toBeGreaterThan(100);
    expect(ktFiles.map((f) => f.file)).toContain(KT_HOME);
    expect(ktFiles.some((f) => f.file.includes("/src/test/")), "测试源混进了生产人群").toBe(false);
    expect(ktRefs.length, "一个 copyText 调用点都没找到 —— 读口没接上，或者读法坏了").toBeGreaterThan(0);
  });

  it("正控（Rust）：扫到了 .rs 生产源码、Rust 取文口自己，也读到了 Rust 调用点", () => {
    expect(rsFiles.length, "一个 .rs 都没扫到").toBeGreaterThan(100);
    expect(rsFiles.map((f) => f.file)).toContain(RS_HOME);
    expect(rsRefs.length, "一个 copy_text 调用点都没找到 —— 读口没接上，或者读法坏了").toBeGreaterThan(0);
     // 三棵树各自真的扫到了、也各自读到了调用点（扩根那一步没接上时，下面的两向相等会在缺角的集合上成立）。
    // `src/common/` 10 → 5：`search-core` 删、`deploy-core` 的判定进后端（契约 `deploy-contract` 只剩一份 lib.rs），盘上现 10 份。
    for (const [root, n] of [["src/backend/", 100], ["src/common/", 5]] as const) {
      expect(rsFiles.filter((f) => f.file.startsWith(root)).length, `${root} 下一个 .rs 都没扫到`).toBeGreaterThan(n);
    }
    expect(rsRefs.some((r) => r.file.startsWith("src/backend/")), "常驻后端一个 copy_text 调用点都没读到").toBe(true);
    for (const d of RS_DEFINITIONS) expect(rsFiles.map((f) => f.file)).toContain(d);
  });

  it("正控：扫到了生产源码，也扫到了取文口自己", () => {
    expect(files.length, "一个生产 .ts 都没扫到 —— 下面的相等是空集对空集").toBeGreaterThan(100);
    expect(files.map((f) => f.file)).toContain(HOME);
    expect(refs.length, "一个 copyText 调用点都没找到 —— 样板区没接上，或者解析器坏了").toBeGreaterThan(0);
  });

  it("没有绕过静态读的写法（非字面 key · 改名导入 · 当值传 · 展开参数）", () => {
    expect(problems).toEqual([]);
  });

  it("★ 表里的 key == 代码里引用的 key（两向）", () => {
    const { unreferenced, unknown } = crossCheck(table, refs);
    expect(unknown, "代码引用了表里没有的 key").toEqual([]);
    expect(unreferenced, "表里有、代码里没人引用的 key（死文案）").toEqual([]);
  });

  it("★ 每个调用点给的参数 == 表里登记的 args", () => {
    expect(crossCheck(table, refs).badArgs).toEqual([]);
  });
});

describe("CP2a · 文案表判据自己会不会死（正控）", () => {
  const t: Table = {
    "a.b.c": { kind: "body", role: "说明", zh: "你好 {name}", args: ["name"] },
  };

  it("形状：两段 key / 未知 kind / 位置式占位符 / args 与占位符不相等 —— 各红一次", () => {
    expect(tableProblems({ "a.b": t["a.b.c"] }).join()).toMatch(/三段式/);
    expect(tableProblems({ "a.b.c": { ...t["a.b.c"], kind: "label" } }).join()).toMatch(/六档/);
    expect(tableProblems({ "a.b.c": { kind: "body", role: "说明", zh: "第 {} 条", args: [] } }).join()).toMatch(/不具名/);
    expect(tableProblems({ "a.b.c": { kind: "body", role: "说明", zh: "你好 {name}", args: [] } }).join()).toMatch(/没登记/);
    expect(tableProblems({ "a.b.c": { kind: "body", role: "说明", zh: "你好", args: ["name"] } }).join()).toMatch(/没有这个占位符/);
  });

  it("[C-K4] role：没写 / 不在闭集 / 与 kind 搭不上 各红一次；搭配表里没人用的格逮得住", () => {
    expect(roleProblems({ "a.b.c": { ...t["a.b.c"], role: "" } }).bad.join()).toMatch(/没写 role/);
    expect(roleProblems({ "a.b.c": { ...t["a.b.c"], role: "旁白" } }).bad.join()).toMatch(/不在闭集/);
    expect(roleProblems({ "a.b.c": { ...t["a.b.c"], role: "菜单项" } }).bad.join()).toMatch(/不配 kind/);
    const one = roleProblems(t);
    expect(one.bad).toEqual([]);
    expect(one.unused).toContain("按钮 · action");
    expect(one.unused).not.toContain("说明 · body");
  });

  it("引用：真调用读得出 key 与参数；四种绕法各被逮住", () => {
    const ok = refsIn("src/x.ts", 'import { copyText } from "./copy-table";\nf(copyText("a.b.c", { name }));\n');
    expect(ok.problems).toEqual([]);
    expect(ok.refs.map((r) => [r.key, r.args])).toEqual([["a.b.c", ["name"]]]);
    expect(refsIn("src/x.ts", "copyText(k);").problems.join()).toMatch(/不是字面量/);
    expect(refsIn("src/x.ts", 'import { copyText as t } from "./copy-table";').problems.join()).toMatch(/改名导入/);
    expect(refsIn("src/x.ts", "const f = copyText;").problems.join()).toMatch(/当值用了/);
    expect(refsIn("src/x.ts", 'copyText("a.b.c", { ...o });').problems.join()).toMatch(/展开/);
  });

  it("〔DP1〕Rust 引用：真调用读得出 key 与参数；非字面 key / 非数组参数 / 当值用 各被逮住；注释里的不算", () => {
    const ok = rustRefsIn(
      "src/frontend/shell/src/x.rs",
      'fn f() -> String {\n    copy_text(\n        "a.b.c",\n        &[("name", n), ("os", &o.to_string())],\n    )\n}\n// copy_text("z.z.z", &[])\n',
    );
    expect(ok.problems).toEqual([]);
    expect(ok.refs.map((r) => [r.key, r.args])).toEqual([["a.b.c", ["name", "os"]]]);
    expect(rustRefsIn("x.rs", "copy_text(KEY, &[]);").problems.join()).toMatch(/不是字面量/);
    expect(rustRefsIn("x.rs", 'copy_text("a.b.c", args);').problems.join()).toMatch(/数组字面量/);
    expect(rustRefsIn("x.rs", "let f = copy_text;").problems.join()).toMatch(/当值用了/);
    expect(rustRefsIn("x.rs", 'copy_text("a.b.c", &[(name, n)]);').problems.join()).toMatch(/参数项/);
    expect(rustRefsIn("x.rs", "use crate::copy_table::copy_text;").problems).toEqual([]);
    expect(rustRefsIn("x.rs", "ui.ctx().copy_text(s.to_string());").refs).toEqual([]);
    // 行里前面有个以 use 结尾的标识符（Misuse）不是 `use` 导入 —— 这个调用点照样得认出来。
    expect(rustRefsIn("x.rs", 'Fault::Misuse => copy_text("a.b.c", &[]),').refs.map((r) => r.key)).toEqual(["a.b.c"]);
    // `&'static str` 形：字面量 key 认得出；非字面 key / 带参数 各被逮住。
    expect(rustRefsIn("x.rs", 'let w = copy_static!("a.b.c");').refs.map((r) => [r.key, r.args])).toEqual([["a.b.c", []]]);
    expect(rustRefsIn("x.rs", "copy_static!(KEY);").problems.join()).toMatch(/不是字面量/);
    expect(rustRefsIn("x.rs", 'copy_static!("a.b.c", x);').problems.join()).toMatch(/不是字面量/);
  });

  it("Kotlin 引用：真调用读得出 key 与参数；非字面 key / 非 \"名\" to 值 / 当值用 / 改名导入 各被逮住；注释里的不算", () => {
    const ok = kotlinRefsIn(
      "src/mobile/app/src/main/kotlin/X.kt",
      'fun f() =\n    copyText(\n        "a.b.c",\n        "name" to n,\n        "os" to listOf(1, 2).size,\n    )\n// copyText("z.z.z")\n/** copyText("y.y.y") */\n',
    );
    expect(ok.problems).toEqual([]);
    expect(ok.refs.map((r) => [r.key, r.args])).toEqual([["a.b.c", ["name", "os"]]]);
    expect(kotlinRefsIn("x.kt", 'copyText("a.b.c")').refs.map((r) => [r.key, r.args])).toEqual([["a.b.c", []]]);
    expect(kotlinRefsIn("x.kt", "copyText(key)").problems.join()).toMatch(/不是字面量/);
    expect(kotlinRefsIn("x.kt", 'copyText("a.$k.c")').problems.join()).toMatch(/不是字面量/);
    expect(kotlinRefsIn("x.kt", 'copyText("a.b.c", name to n)').problems.join()).toMatch(/参数项/);
    expect(kotlinRefsIn("x.kt", "val f = ::copyText").problems.join()).toMatch(/当值用了/);
    expect(kotlinRefsIn("x.kt", "import com.x.copyText as t").problems.join()).toMatch(/改名导入/);
    expect(kotlinRefsIn("x.kt", "import com.x.copyText").problems).toEqual([]);
  });

  it("对拍：表里多一条 / 代码多引一条 / 参数给错 —— 各红一次", () => {
    const r: Ref[] = [{ file: "x", key: "a.b.c", args: ["name"] }];
    expect(crossCheck({ ...t, "a.b.d": t["a.b.c"] }, r).unreferenced).toEqual(["a.b.d"]);
    expect(crossCheck(t, [...r, { file: "y", key: "z.z.z", args: [] }]).unknown).toEqual(["y z.z.z"]);
    expect(crossCheck(t, [{ file: "x", key: "a.b.c", args: [] }]).badArgs.length).toBe(1);
  });
});

// `rules.json` C-L5 逐字「插进来的值与相邻汉字之间的空格随值定」（WF2 报备：「lx上报出了这个会话」）。
describe("[C-L5] 值与汉字之间的空格随值定", () => {
  const table = loadTable();
  const HAN = "[\\u3400-\\u4dbf\\u4e00-\\u9fff\\uf900-\\ufaff]";
  const ASCII = "Q7"; // 表里不会出现的两种值
  const HANV = "龘"; // 单字：两个值之间只隔空格的那一段（设计上照留）靠「另一边也是它」认出来、不算
  /** 这条里有没有一道「值 ↔ 汉字」接缝（中间至多一个空格）—— 没有的条目这一格不看。 */
  const seamed = (zh: string): boolean => new RegExp(`${HAN} ?\\{[A-Za-z][A-Za-z0-9]*\\}|\\{[A-Za-z][A-Za-z0-9]*\\} ?${HAN}`).test(zh);
  const fill = (e: { args: string[] }, v: string): Record<string, string> => Object.fromEntries(e.args.map((a) => [a, v]));

  it("[C-L5] 全表逐条：ASCII 值挨着汉字 ⇒ 恰一个空格；汉字值挨着汉字 ⇒ 没有空格", () => {
    const keys = Object.keys(table).filter((k) => seamed(table[k].zh));
    expect(keys.length, "一条有接缝的都没找到 —— 下面零命中地绿").toBeGreaterThan(900);
    const tightA = new RegExp(`${HAN}${ASCII}|${ASCII}${HAN}`);
    const spacedH = new RegExp(`(?!${HANV})${HAN} ${HANV}|${HANV} (?!${HANV})${HAN}`);
    const bad: string[] = [];
    for (const k of keys) {
      const a = copyText(k as CopyKey, fill(table[k], ASCII));
      if (tightA.test(a)) bad.push(`${k}（ASCII 值紧贴汉字）：${a}`);
      const h = copyText(k as CopyKey, fill(table[k], HANV));
      if (spacedH.test(h)) bad.push(`${k}（汉字值与汉字之间多了空格）：${h}`);
    }
    expect(bad).toEqual([]);
  });

  it("[C-L5] 正控：同一套检查对着不经取文口的原样替换逮得住两种错", () => {
    const raw = (zh: string, v: string): string => zh.replace(/\{[A-Za-z][A-Za-z0-9]*\}/g, v);
    expect(new RegExp(`${HAN}${ASCII}|${ASCII}${HAN}`).test(raw("在{machine}上", ASCII))).toBe(true);
    expect(new RegExp(`(?!${HANV})${HAN} ${HANV}|${HANV} (?!${HANV})${HAN}`).test(raw("{machine} 上没有", HANV))).toBe(true);
    expect(copyText("launchArrival.arrived.body", { machine: ASCII })).toBe(`${ASCII} 已报出此会话`);
    expect(copyText("extPage.card.slot", { field: HANV, key: "b" })).toBe(`${HANV}里的 b`);
  });
});

/**
 * 〔「前端读口 `copy-table.ts::copyText`；Rust 读口只有一份实现 `copy-core::copy_text`」〕
 * **两个读口的插值对拍（TS 这一侧）**：共用金样 `tests/__fixtures__/copy-interpolation.golden.json` 逐条喂给 `copyText`，
 * 期望是金样里手写的；Rust 那一侧 `tests/common/copy-core/lib_tests.rs::the_shared_interpolation_golden_agrees_with_this_reader`
 * 读同一份。两侧有意不同的几形（缺键 · 参数对不上）登记在金样 `_differences`，不在这里（「值里含别的占位符」那一形 Rust 改成单趟之后两侧一致，挪进了 `cases`）。
 */
describe("DUP2 · 两个读口的插值对拍（金样 copy-interpolation.golden.json）", () => {
  const golden = JSON.parse(readFileSync(resolve(REPO_ROOT, "tests/__fixtures__/copy-interpolation.golden.json"), "utf8")) as {
    cases: { key: string; zh: string; args: Record<string, string>; want: string }[];
  };
  const table = loadTable();

  it("金样读得出（反空真）", () => {
    expect(golden.cases.length).toBeGreaterThanOrEqual(5);
  });

  it("每条：表里还是那一句，copyText 插出来 == 手写的 want", () => {
    const wrong: string[] = [];
    for (const c of golden.cases) {
      expect(table[c.key]?.zh, `${c.key} 在表里的原文变了 —— 照新句子改金样的 zh 与 want`).toBe(c.zh);
      const got = copyText(c.key as CopyKey, c.args);
      if (got !== c.want) wrong.push(`${c.key}: ${JSON.stringify(got)}`);
    }
    expect(wrong).toEqual([]);
  });
});

/**
 * 出口扫描判据的两句（`exit-judgment-scan.vitest.ts` 用；单列一份是为了让判据与它的对照样例读同一份判法）。
 *
 * 「出口零判定零格式化」（`src/doc/ARCHITECTURE.md` §2.9）：出口只取自己骨架上的字，描述数据与状态的字由核心写。
 * 按意思判成两句，三门语言各照自己的语法判同样的两句：
 * - `key`：出口取的文案键只许骨架那几档（文案表 `kind` 的 title · control · action · aria · body），且那一条不带插值（没登 `args`）。
 *   不带插值的 body（空态 · 提示）是界面固定的字，不随数据变；error 是结局，由后端交成品，不算骨架。
 *   键不是字面量、表里没有这把键，也算一处。
 * - `branch`：条件读了线上值的分支里不许取文。「读了线上值」＝ 条件拿一个值去比字面的码：`x.code === "dup"` 一类的比较、
 *   `case "dup":`（连同那个 `switch` 的 `default:`）、`CODES.has(x)` / `CODES.includes(x)`（`CODES` 是本文件里一组字面码）。
 *   管到的分支：`if` 的两头 · 三元的两头 · `&&` / `||` / `??` 的右侧 · `case` / `default` 底下；进到里面再定义的具名函数 · 方法 · 类就不算，
 *   箭头函数跟着外面的分支算（按码挂上去的回调取的字，也是按码挑的字）。
 *   分支里调本文件里取文的函数（具名函数 · 绑在 `const` 上的箭头函数 · 函数表达式，体里取文或再调这样的函数）也算一处：
 *   取文挪进小函数，取的仍是按码挑的字。
 * TS 这一半在这里（typescript 的语法树）；Kotlin 那一半是 `src/mobile/detekt-rules` 里的 detekt 规则，
 * 壳与文件窗口的 Rust 那一半是 `tests/frontend/filewin/exit_scan_tests.rs`。
 *
 * 买不到：分支里调的是别的文件里的函数或类的方法，取文在那边，认不出；比的码不是字面量（从别处拿来的常量、枚举）就认不出是在按码判；先 `if (…) return` 再在后面取文，后面那一句不在分支里，也认不出；
 * 不经文案表、直接写在代码里的字归文案表那几条判据管，不在这里。
 */
import ts from "typescript";

/** 两句的名字（欠账名单的 `rule` 只许这两个）。 */
export const RULE_IDS = ["key", "branch"] as const;
export type RuleId = (typeof RULE_IDS)[number];

/** 骨架那几档：出口自己的字只许是这几档。 */
export const SKELETON_KINDS: ReadonlySet<string> = new Set(["title", "control", "action", "aria", "body"]);

/** 文案表里判这两句要用的那两格。 */
export interface CopyEntry {
  kind: string;
  args?: string[];
}

export type CopyTable = Readonly<Record<string, CopyEntry>>;

/** 这把键出口取得了：表里有、骨架那几档、不带插值。 */
export function skeletonKey(key: string | null, table: CopyTable): boolean {
  if (key === null) return false;
  const e = table[key];
  return e !== undefined && SKELETON_KINDS.has(e.kind) && (e.args ?? []).length === 0;
}

const EQ = new Set([
  ts.SyntaxKind.EqualsEqualsEqualsToken,
  ts.SyntaxKind.ExclamationEqualsEqualsToken,
  ts.SyntaxKind.EqualsEqualsToken,
  ts.SyntaxKind.ExclamationEqualsToken,
]);

function isStr(n: ts.Node): boolean {
  return ts.isStringLiteral(n) || ts.isNoSubstitutionTemplateLiteral(n);
}

function unwrap(n: ts.Expression): ts.Expression {
  let e = n;
  while (ts.isParenthesizedExpression(e) || ts.isAsExpression(e) || ts.isSatisfiesExpression(e)) e = e.expression;
  return e;
}

/** 一组字面码：`["a", "b"]` 或 `new Set([...])`。 */
function isCodeList(n: ts.Expression): boolean {
  const e = unwrap(n);
  if (ts.isArrayLiteralExpression(e)) return e.elements.length > 0 && e.elements.every(isStr);
  if (ts.isNewExpression(e) && ts.isIdentifier(e.expression) && e.expression.text === "Set") {
    const a = e.arguments?.[0];
    return a !== undefined && isCodeList(a);
  }
  return false;
}

/** 往外不再找分支的边界：具名的函数 · 方法 · 类（箭头函数与函数表达式跟着外面的分支算，同 Kotlin 的 lambda）。 */
function isFunctionLike(n: ts.Node): boolean {
  return (
    ts.isFunctionDeclaration(n) ||
    ts.isMethodDeclaration(n) ||
    ts.isConstructorDeclaration(n) ||
    ts.isGetAccessorDeclaration(n) ||
    ts.isSetAccessorDeclaration(n) ||
    ts.isClassLike(n)
  );
}

/** 本文件里绑着一组字面码的名字。 */
function codeLists(sf: ts.SourceFile): Set<string> {
  const out = new Set<string>();
  const walk = (n: ts.Node): void => {
    if (ts.isVariableDeclaration(n) && ts.isIdentifier(n.name) && n.initializer && isCodeList(n.initializer)) out.add(n.name.text);
    ts.forEachChild(n, walk);
  };
  walk(sf);
  return out;
}

/** 条件里拿一个值去比字面的码（不进里面再定义的函数）。 */
function readsCode(cond: ts.Node, lists: Set<string>): boolean {
  let found = false;
  const walk = (n: ts.Node): void => {
    if (found || isFunctionLike(n)) return;
    if (ts.isBinaryExpression(n) && EQ.has(n.operatorToken.kind) && isStr(unwrap(n.left)) !== isStr(unwrap(n.right))) found = true;
    else if (
      ts.isCallExpression(n) &&
      ts.isPropertyAccessExpression(n.expression) &&
      ["has", "includes"].includes(n.expression.name.text) &&
      n.arguments.length === 1 &&
      !isStr(n.arguments[0]!)
    ) {
      const recv = unwrap(n.expression.expression);
      if ((ts.isIdentifier(recv) && lists.has(recv.text)) || isCodeList(recv)) found = true;
    }
    if (!found) ts.forEachChild(n, walk);
  };
  walk(cond);
  return found;
}

function caseOnCode(c: ts.CaseClause, lists: Set<string>): boolean {
  return isStr(unwrap(c.expression)) || readsCode(c.expression, lists);
}

/** `child` 是不是 `parent` 那个按码判的分支底下（`child` 是 `parent` 的直接子节点）。 */
function branchOnCode(parent: ts.Node, child: ts.Node, lists: Set<string>): boolean {
  if (ts.isIfStatement(parent)) return child !== parent.expression && readsCode(parent.expression, lists);
  if (ts.isConditionalExpression(parent)) return child !== parent.condition && readsCode(parent.condition, lists);
  if (ts.isBinaryExpression(parent)) {
    const op = parent.operatorToken.kind;
    const logical = op === ts.SyntaxKind.AmpersandAmpersandToken || op === ts.SyntaxKind.BarBarToken || op === ts.SyntaxKind.QuestionQuestionToken;
    return logical && child === parent.right && readsCode(parent.left, lists);
  }
  if (ts.isCaseClause(parent)) return child !== parent.expression && caseOnCode(parent, lists);
  if (ts.isDefaultClause(parent)) return parent.parent.clauses.some((c) => ts.isCaseClause(c) && caseOnCode(c, lists));
  return false;
}

/** 调的是哪个名字（`f(…)` ⇒ `f`；别的形状 ⇒ null）。 */
function calleeName(n: ts.Node): string | null {
  return ts.isCallExpression(n) && ts.isIdentifier(n.expression) ? n.expression.text : null;
}

/**
 * 本文件里取文的函数：具名函数与绑在 `const` 上的箭头函数 · 函数表达式，体里调了 `copyText`，
 * 或调了本文件里另一个取文的函数（一直传到不再变）。
 */
function textFns(sf: ts.SourceFile): Set<string> {
  const bodies = new Map<string, ts.Node>();
  const collect = (n: ts.Node): void => {
    if (ts.isFunctionDeclaration(n) && n.name && n.body) bodies.set(n.name.text, n.body);
    else if (ts.isVariableDeclaration(n) && ts.isIdentifier(n.name) && n.initializer) {
      const init = unwrap(n.initializer);
      if (ts.isArrowFunction(init) || ts.isFunctionExpression(init)) bodies.set(n.name.text, init.body);
    }
    ts.forEachChild(n, collect);
  };
  collect(sf);
  const calls = new Map<string, Set<string>>();
  for (const [name, body] of bodies) {
    const out = new Set<string>();
    const walk = (n: ts.Node): void => {
      const c = calleeName(n);
      if (c !== null) out.add(c);
      ts.forEachChild(n, walk);
    };
    walk(body);
    calls.set(name, out);
  }
  const takes = new Set<string>();
  for (let grew = true; grew; ) {
    grew = false;
    for (const [name, cs] of calls) {
      if (!takes.has(name) && [...cs].some((c) => c === "copyText" || takes.has(c))) {
        takes.add(name);
        grew = true;
      }
    }
  }
  return takes;
}

/** 一份 TS 生产代码里两句各命中几处。 */
export function scanTs(code: string, table: CopyTable, file = "x.ts"): Record<RuleId, number> {
  const sf = ts.createSourceFile(file, code, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS);
  const lists = codeLists(sf);
  const takes = textFns(sf);
  const got: Record<RuleId, number> = { key: 0, branch: 0 };
  const inCodeBranch = (n: ts.Node): boolean => {
    let child: ts.Node = n;
    for (let p = n.parent; p && !ts.isSourceFile(p) && !isFunctionLike(child); child = p, p = p.parent) {
      if (branchOnCode(p, child, lists)) return true;
    }
    return false;
  };
  const visit = (n: ts.Node): void => {
    const callee = calleeName(n);
    if (callee === "copyText") {
      const a = (n as ts.CallExpression).arguments[0];
      const key = a !== undefined && (ts.isStringLiteral(a) || ts.isNoSubstitutionTemplateLiteral(a)) ? a.text : null;
      if (!skeletonKey(key, table)) got.key++;
      if (inCodeBranch(n)) got.branch++;
    } else if (callee !== null && takes.has(callee) && inCodeBranch(n)) got.branch++;
    ts.forEachChild(n, visit);
  };
  visit(sf);
  return got;
}

/** 对照样例用的表：`s.k.x` · `p.k.x` 是骨架那几档、不带插值；`b.k.x` 是报错；`a.k.x` 带插值。 */
export const SAMPLE_TABLE: CopyTable = {
  "s.k.x": { kind: "action" },
  "p.k.x": { kind: "body" },
  "b.k.x": { kind: "error" },
  "a.k.x": { kind: "title", args: ["n"] },
};

/** 对照样例：每一段两句各命中几处（探测器坏了 ⇒ 先红）。第一段是照写法正则扫时漏掉的那一形（`case` 里套一层再取文）。 */
export const TS_SAMPLES: { code: string; want: Record<RuleId, number> }[] = [
  { code: 'function f(x: X) { switch (x.code) { case "x": return v(copyText("s.k.x")); } }', want: { key: 0, branch: 1 } },
  { code: 'function f(x: X) { switch (x.code) { case "x": return 1; default: return copyText("s.k.x"); } }', want: { key: 0, branch: 1 } },
  { code: 'const RETRYABLE = new Set(["screen_changed", "ambiguous"]);\nconst t = RETRYABLE.has(x.why) ? copyText("s.k.x") : null;', want: { key: 0, branch: 1 } },
  { code: 'const t = x.code === "dup" ? copyText("s.k.x") : copyText("s.k.x");', want: { key: 0, branch: 2 } },
  { code: 'if (q.state === "refused") el.textContent = copyText("s.k.x");', want: { key: 0, branch: 1 } },
  { code: 'const t = x.state === "on" && copyText("s.k.x");', want: { key: 0, branch: 1 } },
  { code: 'el.textContent = copyText("b.k.x");', want: { key: 1, branch: 0 } },
  { code: 'el.textContent = copyText("a.k.x", { n });', want: { key: 1, branch: 0 } },
  { code: "el.textContent = copyText(k);", want: { key: 1, branch: 0 } },
  { code: 'el.textContent = copyText("no.such.key");', want: { key: 1, branch: 0 } },
  { code: 'if (x.code === "dup") btn.onclick = () => say(copyText("s.k.x"));', want: { key: 0, branch: 1 } },
  { code: 'const t = rows.some((r) => r.kind === "x") ? copyText("s.k.x") : null;', want: { key: 0, branch: 1 } },
  // 取文挪进同文件的小函数：按码判的分支里调它，算一处（连它再调的同文件函数一起看）。
  { code: 'function none() { return span(copyText("s.k.x")); }\nif (b.takeover === "no_line") box.append(none());', want: { key: 0, branch: 1 } },
  { code: 'const word = () => copyText("s.k.x");\nfunction none() { return word(); }\nconst t = x.code === "dup" ? none() : null;', want: { key: 0, branch: 1 } },
  // 阴性：骨架字照摆；条件不比码；分支里再定义的具名函数不算；比码的那一侧在条件里不算；两侧都是字面量不算。
  { code: 'el.textContent = copyText("s.k.x");', want: { key: 0, branch: 0 } },
  { code: 'empty.textContent = copyText("p.k.x");', want: { key: 0, branch: 0 } },
  { code: 'const t = open ? copyText("s.k.x") : copyText("s.k.x");', want: { key: 0, branch: 0 } },
  { code: 'if (x.code === "dup") { function g() { return copyText("s.k.x"); } }', want: { key: 0, branch: 0 } },
  { code: 'if (copyText("s.k.x") === "y") go();', want: { key: 0, branch: 0 } },
  { code: 'const t = "a" === "b" ? copyText("s.k.x") : null;', want: { key: 0, branch: 0 } },
  // 阴性：同一个取文的小函数在分支外调不算；分支里调的同文件函数不取文不算。
  { code: 'function none() { return copyText("s.k.x"); }\nbox.append(none());', want: { key: 0, branch: 0 } },
  { code: 'function plain() { return 1; }\nif (x.code === "dup") go(plain());', want: { key: 0, branch: 0 } },
];

/**
 * **`onLine` 上不许有旁路记账员** 的机检。
 *
 * ## 守的要求（住址，逐字）
 *
 * -：「3. 前端不许从流上攒全会话事实 —— `onLine` 上的旁路记账员只剩『真事件』那两个。」
 * -：「**『这个会话到目前为止是什么样』＝ 全会话事实 ＝ 读 json** / **『刚刚发生了什么』＝ 事件 ＝ 留在流上**」；
 *   那张表里标「**事件，留在流上**」的恰好两行：`turnEndNotifier.observe`（轮次结束 → 系统通知）·
 *   `compactWaiters`（compact 完成 → 唤醒换号重启编排）。标「读 json」的四行（`trackUsage` · `trackAgents` ·
 *   `noteTouchedFiles` · `applyForkedFrom`）由 STC 改成问后端（`history-facts`，阶段 C）。
 *
 * ## 两条判据
 *
 * **L1 · `onLine` 的调用人群 == 登记表（两向）**
 * - 人群：`src/frontend/ui/tabs.ts` 里 `TabManager.onLine` 方法体中，**实参子树提到记录**的每一个调用，按被调者文本归并（走 TS 的 AST，
 *   注释天然不在）。「提到记录」= 出现形参 `payload`，或出现由它**只经取属性 / 断言 / 括号**得来的局部常量
 *   （如 `const uuid = (payload.message as …).uuid`；函数调用的返回值不算 —— `ensureTab(…)` 返回的是 tab，不是记录）。
 * - 登记表每行写类：`入口`（建 tab · 两道去重 · 线上表示法换算）· `真事件` · `活卡定稿`（TAP 那一格：jsonl 那一轮到了 ⇒
 *   撤掉同 `message.id` 的活卡，）· `渲染管线`。**没有「记账员」这一类。**
 * - 另两条：登记表里 `真事件` 类的事件名集合 == `{轮次结束, compact 完成}`（异源：上面原文那两行）；
 *   各类恰好这些行（`渲染管线` 只有 `this.view.ingest` 一行 —— 管线内部的 sink 不在那张表的射程里，见「买不到」）。
 * - 正控：同一个抽取器对一段内嵌样本抽得出 `noteAgents(tab, payload.message)` 与经别名的 `f(m)`，抽不出 `g(tab)`。
 *
 * **L2 · 四个事实字段只有登记的写者**（`touchedFiles` · `latestPromptTokens` · `latestModel` · `forkedFromSessionId`）
 * - 人群：`src/**\/*.ts` 生产段里对这几个名字的**写**（`x.<名> = …` 的左边 · `x.<名>.set / add / delete / clear(…)`），
 *   按 `(文件, 所在声明)` 归并 == 登记表（两向）。L1 只看 `onLine` 自己的方法体；L2 管的是「记账员换个地方长回来」
 *   （例如塞进渲染管线的某个回调里）—— 那样它必然要写这几个字段，就会在这里多出一行。
 * - 按**名字**认（不跑类型检查器）⇒ 别的对象上同名的字段也在人群里，逐条登记为「同名不同物」、写清是谁。
 * - 正控：内嵌样本里的 `tab.touchedFiles.add(x)` 与 `t.latestModel = m` 被抽出来。
 *
 * ## 同波别路 / 买不到
 *
 * - 渲染管线内部（`view.ingest` → `renderContentRecord` 的 sink：标题 · 分支 · 队列 · 真用户输入）不在 L1 的人群里：
 * 那张表列的是「挂在 `onLine` 上的旁路」，管线本体不在表里。它若长出记账员，L2 兜「写事实字段」那一形。
 * - 别名只认一层取属性链；把记录塞进数组再取出来、或经函数返回，L1 看不见（L2 仍在）。
 */
import ts from "typescript";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

import { productionTsFiles, SCAN_TIMEOUT_MS } from "../../test-support/production-sources.ts";
import { REPO_ROOT } from "../../test-support/repo-root.ts";

// ─────────────────────────────── L1 ───────────────────────────────

type OnLineClass = "入口" | "真事件" | "活卡定稿" | "渲染管线";

/** `(被调者, 类, 真事件的事件名 | null, 理由)` —— `onLine` 上今天每一个吃记录的调用。 */
const ONLINE_CALLS: ReadonlyArray<readonly [string, OnLineClass, string | null, string]> = [
  ["this.ensureTab", "入口", null, "按 sid 找 / 建 tab（远端「收到行 ⇒ 复活」也在这里）"],
  ["originFromWire", "入口", null, "线上 origin 的唯一一处表示法换算"],
  ["tab.seenSeqs.has", "入口", null, "按 (sid, seq) 去重（快照与实时的重叠区，`INVARIANTS §25a`）"],
  ["tab.seenSeqs.add", "入口", null, "同上"],
  ["tab.seenSeqs.addRange", "入口", null, "monitor 连着见过、都不可显示的那一段一起记（`skipped_from`）"],
  ["this.actions.settleCompact", "真事件", "compact 完成", "「compactWaiters：compact 完成 → 唤醒换号重启编排」"],
  ["isCompactRecord", "真事件", "compact 完成", "同一个事件：「这一行是不是 compact 摘要」的判法（交给 settleCompact 的闭包里）"],
  ["turnEndNotifier.observe", "真事件", "轮次结束", "「turnEndNotifier.observe：轮次结束 → 系统通知」"],
  [
    "this.live.onRecord",
    "活卡定稿",
    null,
    "jsonl 那一轮到了 ⇒ 同 `message.id` 的活卡整轮撤掉（「jsonl 到了就整轮覆盖」）；不攒任何全会话事实",
  ],
  ["this.view.ingest", "渲染管线", null, "渲染管线本体（按 seq 门控建卡 / 收纳）"],
];

/** 表里标「事件，留在流上」的恰好这两行（异源：设计原文，不是上面那张表）。 */
const DESIGN_TRUE_EVENTS = ["compact 完成", "轮次结束"];

/** 一个表达式（去括号 / `as` / 非空断言）是不是以 `roots` 里某个名字为根、只经取属性 / 取下标得来。 */
function rootedIn(e: ts.Expression, roots: ReadonlySet<string>): boolean {
  let cur: ts.Expression = e;
  for (;;) {
    if (ts.isParenthesizedExpression(cur) || ts.isAsExpression(cur) || ts.isNonNullExpression(cur)) cur = cur.expression;
    else if (ts.isPropertyAccessExpression(cur) || ts.isElementAccessExpression(cur)) cur = cur.expression;
    else break;
  }
  return ts.isIdentifier(cur) && roots.has(cur.text);
}

/** 子树里有没有出现 `roots` 里的某个名字（作标识符）。 */
function mentions(n: ts.Node, roots: ReadonlySet<string>): boolean {
  let hit = false;
  const walk = (x: ts.Node): void => {
    if (hit) return;
    if (ts.isIdentifier(x) && roots.has(x.text)) {
      // 取属性的**属性名**那一格（`a.payload`）不算提到。
      if (!(ts.isPropertyAccessExpression(x.parent) && x.parent.name === x)) hit = true;
      return;
    }
    ts.forEachChild(x, walk);
  };
  walk(n);
  return hit;
}

/**
 * 一段源码里 `<cls>.<method>` 方法体中「实参提到记录」的调用，按被调者文本归并（去空白）。
 * 记录 = 第一个形参 ＋ 由它只经取属性得来的局部常量（按出现顺序传递）。
 */
export function recordConsumers(source: string, cls: string, method: string): string[] {
  const sf = ts.createSourceFile("x.ts", source, ts.ScriptTarget.Latest, true);
  let body: ts.Block | undefined;
  let param: string | undefined;
  const find = (n: ts.Node): void => {
    if (ts.isClassDeclaration(n) && n.name?.text === cls) {
      for (const m of n.members) {
        if (ts.isMethodDeclaration(m) && m.name.getText(sf) === method && m.body) {
          body = m.body;
          const p = m.parameters[0]?.name;
          if (p && ts.isIdentifier(p)) param = p.text;
        }
      }
    }
    ts.forEachChild(n, find);
  };
  find(sf);
  if (!body || !param) return [];
  const roots = new Set<string>([param]);
  const out = new Set<string>();
  const walk = (n: ts.Node): void => {
    if (ts.isVariableDeclaration(n) && n.initializer && ts.isIdentifier(n.name) && rootedIn(n.initializer, roots)) {
      roots.add(n.name.text);
    }
    if (ts.isCallExpression(n) && n.arguments.some((a) => mentions(a, roots))) {
      out.add(n.expression.getText(sf).replace(/\s+/g, ""));
    }
    ts.forEachChild(n, walk);
  };
  walk(body);
  return [...out].sort();
}

describe("〔STC〕L1 · onLine 上只剩真事件", () => {
  const tabsSrc = readFileSync(resolve(REPO_ROOT, "src/frontend/ui/tabs.ts"), "utf8");

  it("★ 正控：抽取器认得出直呼的记账员、经别名的记账员，不认只拿 tab 的调用", () => {
    const sample = `
      class K {
        onLine(payload: P): void {
          const tab = this.ensureTab(payload.session_id);
          const m = (payload.message as { x?: unknown });
          noteAgents(tab, payload.message);
          f(m);
          g(tab);
          this.view.noteGrew(tab);
        }
      }`;
    expect(recordConsumers(sample, "K", "onLine")).toEqual(["f", "noteAgents", "this.ensureTab"]);
  });

  it("★ 人群 == 登记表（两向）", () => {
    const got = recordConsumers(tabsSrc, "TabManager", "onLine");
    expect(got.length, "一个调用都没抽到 —— 抽取器或方法名坏了，下面会拿空集比").toBeGreaterThan(0);
    expect(got, "`onLine` 上吃记录的调用变了 —— 新的一条回登记表写清它是哪一类（没有「记账员」这一类）").toEqual(
      ONLINE_CALLS.map(([c]) => c).sort(),
    );
  });

  it("★ 真事件恰好是那两个；渲染管线 / 活卡定稿各只有登记那一行", () => {
    const events = [...new Set(ONLINE_CALLS.filter(([, k]) => k === "真事件").map(([, , e]) => e))].sort();
    expect(events).toEqual([...DESIGN_TRUE_EVENTS].sort());
    for (const [c, k, e, why] of ONLINE_CALLS) {
      expect(why.trim().length, `${c} 没写理由`).toBeGreaterThan(0);
      expect(k === "真事件", `${c}：只有真事件那一类带事件名`).toBe(e !== null);
    }
    expect(ONLINE_CALLS.filter(([, k]) => k === "渲染管线").map(([c]) => c)).toEqual(["this.view.ingest"]);
    expect(ONLINE_CALLS.filter(([, k]) => k === "活卡定稿").map(([c]) => c)).toEqual(["this.live.onRecord"]);
  });
});

// ─────────────────────────────── L2 ───────────────────────────────

const FACT_FIELDS = [
  "touchedFiles",
  "latestPromptTokens",
  "latestModel",
  "forkedFromSessionId",
] as const;
const MUTATORS = new Set(["set", "add", "delete", "clear"]);

/** `(文件, 所在声明)` —— 今天对这几个名字的全部写者，逐条写理由。 */
const FACT_WRITERS: ReadonlyArray<readonly [string, string, string]> = [
  ["src/frontend/ui/tab-session-facts.ts", "applyFacts", "后端成品的投影：三样整份替换（唯一的正门）"],
];

/** 一个节点所在的「顶层声明 / 类方法」名（类成员记成 `类.成员`）。 */
function ownerOf(n: ts.Node, sf: ts.SourceFile): string {
  let member: string | null = null;
  let cur: ts.Node | undefined = n;
  let top = "<模块顶层>";
  while (cur && !ts.isSourceFile(cur)) {
    const p: ts.Node | undefined = cur.parent;
    if (p && ts.isClassDeclaration(p) && member === null) {
      const m = cur as ts.ClassElement;
      member = m.name ? m.name.getText(sf) : "constructor";
    }
    if (p && ts.isSourceFile(p)) {
      if (ts.isFunctionDeclaration(cur) || ts.isClassDeclaration(cur)) top = cur.name?.text ?? "<匿名>";
      else if (ts.isVariableStatement(cur)) top = cur.declarationList.declarations.map((d) => d.name.getText(sf)).join(",");
      if (ts.isClassDeclaration(cur) && member !== null) top = `${top}.${member}`;
    }
    cur = p;
  }
  return top;
}

/** 一份源码里对事实字段的写者：`所在声明` 的集合。 */
export function factWriters(source: string): string[] {
  const sf = ts.createSourceFile("x.ts", source, ts.ScriptTarget.Latest, true);
  const fields = new Set<string>(FACT_FIELDS);
  const out = new Set<string>();
  const walk = (n: ts.Node): void => {
    if (
      ts.isBinaryExpression(n) &&
      n.operatorToken.kind >= ts.SyntaxKind.FirstAssignment &&
      n.operatorToken.kind <= ts.SyntaxKind.LastAssignment &&
      ts.isPropertyAccessExpression(n.left) &&
      fields.has(n.left.name.text)
    ) {
      out.add(ownerOf(n, sf));
    }
    if (
      ts.isCallExpression(n) &&
      ts.isPropertyAccessExpression(n.expression) &&
      MUTATORS.has(n.expression.name.text) &&
      ts.isPropertyAccessExpression(n.expression.expression) &&
      fields.has(n.expression.expression.name.text)
    ) {
      out.add(ownerOf(n, sf));
    }
    ts.forEachChild(n, walk);
  };
  walk(sf);
  return [...out];
}

describe("〔STC〕L2 · 会话事实字段只有登记的写者（记账员换个地方也长不回来）", () => {
  it("★ 正控：抽得出集合增删与赋值两种写，不认读", () => {
    const sample = `
      function feed(tab: T, x: string): void { tab.touchedFiles.add(x); }
      class H { bump(t: T, m: string): void { t.latestModel = m; } read(t: T): number { return t.agents.size; } }`;
    expect(factWriters(sample).sort()).toEqual(["H.bump", "feed"]);
  });

  it("★ 人群 == 登记表（两向，按文件 ＋ 所在声明）", () => {
    const files = productionTsFiles("src");
    expect(files.length, "一个 TS 生产文件都没扫到").toBeGreaterThan(0);
    const got: string[] = [];
    for (const { file, text } of files) for (const o of factWriters(text)) got.push(`${file} :: ${o}`);
    expect(got.sort(), "事实字段多了 / 少了写者 —— 从流上攒全会话事实回来了？").toEqual(
      FACT_WRITERS.map(([f, o]) => `${f} :: ${o}`).sort(),
    );
    for (const [f, o, why] of FACT_WRITERS) expect(why.trim().length, `${f} :: ${o} 没写理由`).toBeGreaterThan(0);
  }, SCAN_TIMEOUT_MS);
});

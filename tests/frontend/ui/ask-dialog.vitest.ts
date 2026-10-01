/**
 * 应用内对话框收口的判据。
 *
 * # 要求住址
 *
 * - 「全仓 `window.prompt` / `window.confirm` 收口 …… 独立一件，牵扯 `plugin:dialog` 被 ACL 拒」。
 * - 「全仓 `window.prompt/confirm` 的收口 | 独立一件（牵扯 `plugin:dialog` 的 ACL）」。
 * - `INVARIANTS §13`「任何用 `position: fixed` 实现的浮层 …… **必须**挂到 `document.body`」。
 *
 * # 为什么这不是洁癖（`src/frontend/ui/ask-dialog.ts` 头注）
 *
 * `tauri-plugin-dialog` 往 webview 注入的 `window.confirm` 是 `async` 的 ⇒ 返回 Promise、永远真值 ⇒
 * 真 app 里每一处 `if (!window.confirm(m)) return;` 都不拦。jsdom 的 `window.confirm` 是同步原生实现，
 * 所以这件事在单测里从来看不见。
 *
 * | 格 | 判什么 | 形态 |
 * |---|---|---|
 * | D1 | 生产 TS 里引用原生 `confirm` / `prompt`（裸调用，或 `window.` / `globalThis.` / `self.` 成员，调用或取值） | TS AST，**零命中**；正控：合成样本逐形各恰 1 处、近似形 0 处 |
 * | D1b | 生产 TS 里每一处**调用** `askConfirm` / `askText` 都是 `await` 的操作数（没 `await` 的 Promise 恒真值 —— 正是原生替身那个病） | TS AST，未 await 的调用 **零命中**；正控同上；另钉「被调用的地方恰好是改过的那几份文件」两向相等 |
 * | D2 | `askConfirm` / `askText` 的结算语义 | 确定 / 取消 / 遮罩 / Esc（经真 `dispatcher`）/ Enter / 顶掉上一个 / 挂 body / 正文不解释 HTML |
 *
 * `alert` 不在 D1：它归 `INVARIANTS §12`，由 `tests/frontend/ui/invariants-frontend-guard.vitest.ts` ① 守。
 */
import { describe, it, expect, beforeAll, afterEach } from "vitest";
import ts from "typescript";

import { askConfirm, askText } from "../../../src/frontend/ui/ask-dialog";
import { dispatcher } from "../../../src/frontend/ui/keybindings/registry";
import { productionTsFiles, SCAN_TIMEOUT_MS } from "../../test-support/production-sources.ts";

// ─────────────────────────────── D1 ───────────────────────────────

const NATIVE = new Set(["confirm", "prompt"]);
const GLOBALS = new Set(["window", "globalThis", "self"]);

interface Hit {
  file: string;
  line: number;
  text: string;
}

/** 一份源码里引用原生 `confirm` / `prompt` 的地方（AST：注释天然不算）。 */
function nativeDialogRefs(file: string, text: string): Hit[] {
  const sf = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS);
  const out: Hit[] = [];
  const hit = (n: ts.Node): void => {
    const { line } = sf.getLineAndCharacterOfPosition(n.getStart(sf));
    out.push({ file, line: line + 1, text: n.getText(sf) });
  };
  const visit = (n: ts.Node): void => {
    // `window.confirm` / `globalThis.prompt` / `self.confirm` —— 调用也好、取值也好（`opts.confirm ?? window.confirm`）。
    if (
      ts.isPropertyAccessExpression(n) &&
      ts.isIdentifier(n.expression) &&
      GLOBALS.has(n.expression.text) &&
      NATIVE.has(n.name.text)
    ) {
      hit(n);
    }
    // 裸调用 `confirm(…)` / `prompt(…)`。
    if (ts.isCallExpression(n) && ts.isIdentifier(n.expression) && NATIVE.has(n.expression.text)) {
      hit(n.expression);
    }
    ts.forEachChild(n, visit);
  };
  visit(sf);
  return out;
}

describe("D1 · 生产 TS 零处原生 confirm / prompt", () => {
  it("🔴 正控：量具对合成样本逐形各认出恰 1 处，近似形一处不认", () => {
    const positives = [
      `if (!window.confirm("a")) return;`,
      `const ok = confirm("b");`,
      `const n = globalThis.prompt("c", "");`,
      `self.confirm("d");`,
      `const f = opts.confirm ?? ((m: string) => window.confirm(m));`,
      `const g = window.prompt;`,
      `const h = prompt("e");`,
    ];
    for (const src of positives) {
      expect(nativeDialogRefs("probe.ts", src).length, src).toBe(1);
    }
    const negatives = [
      `await askConfirm("x");`,
      `opts.confirm("y");`,
      `const confirmFn = opts?.confirm;`,
      `tab.prompt("z");`,
      `// window.confirm("注释里的不算")`,
      `const s = "window.confirm(不是代码)";`,
      `function confirm2() {}`,
    ];
    for (const src of negatives) {
      expect(nativeDialogRefs("probe.ts", src), src).toEqual([]);
    }
  });

  it("★ 生产 TS（src/**，生成物除外）引用原生 confirm / prompt 的地方 == ∅", () => {
    const files = productionTsFiles("src");
    expect(files.length, "人群为空 —— 下面的零命中是空真").toBeGreaterThan(100);
    expect(
      files.some((f) => f.file === "src/frontend/ui/ask-dialog.ts"),
      "人群里没有对话框模块本身 —— 遍历口径变了",
    ).toBe(true);
    const hits = files.flatMap((f) => nativeDialogRefs(f.file, f.text));
    expect(
      hits.map((h) => `${h.file}:${h.line}  ${h.text}`),
      "这些地方还在用原生弹窗 —— 真 app 里 window.confirm 返回 Promise（永远真值），等于没问；改用 src/frontend/ui/ask-dialog.ts 的 askConfirm / askText",
    ).toEqual([]);
  }, SCAN_TIMEOUT_MS); // 全仓生产 TS 逐份建 AST：整套并跑时 5 s 默认期限不够（现打 7.8 s）
});

// ─────────────────────────────── D1b ───────────────────────────────

const ASKS = new Set(["askConfirm", "askText"]);

/** 一份源码里调用 `askConfirm` / `askText` 的地方：`[全部调用, 其中没被 await 的]`。 */
function askCalls(file: string, text: string): { all: Hit[]; unawaited: Hit[] } {
  const sf = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS);
  const all: Hit[] = [];
  const unawaited: Hit[] = [];
  const visit = (n: ts.Node): void => {
    if (ts.isCallExpression(n) && ts.isIdentifier(n.expression) && ASKS.has(n.expression.text)) {
      const { line } = sf.getLineAndCharacterOfPosition(n.getStart(sf));
      const h = { file, line: line + 1, text: n.getText(sf).slice(0, 80) };
      all.push(h);
      let p: ts.Node = n.parent;
      while (ts.isParenthesizedExpression(p)) p = p.parent;
      if (!ts.isAwaitExpression(p)) unawaited.push(h);
    }
    ts.forEachChild(n, visit);
  };
  visit(sf);
  return { all, unawaited };
}

/** 本路改过、今天调用对话框的文件（期望手写，不从扫描结果派生）。 */
const ASK_CALLERS = [
  "src/frontend/ui/keybindings/editor.ts",
  "src/frontend/ui/settings/accounts-mcp-block.ts",
  "src/frontend/ui/settings/accounts-section.ts",
  "src/frontend/ui/settings/cc-bus-section.ts",
  "src/frontend/ui/settings/machine-card.ts",
  "src/frontend/ui/settings/panel.ts",
  "src/frontend/ui/tab-menu.ts",
  "src/frontend/ui/views/history.ts",
];

describe("D1b · 对话框的答案一律 await", () => {
  it("🔴 正控：没 await 的调用认得出，await 的（含括号、`!await`）不认", () => {
    const bad = [`if (!askConfirm("a")) return;`, `const ok = askText("b");`, `void askConfirm("c");`];
    for (const src of bad) expect(askCalls("p.ts", src).unawaited.length, src).toBe(1);
    const good = [
      `if (!(await askConfirm("a"))) return;`,
      `const n = await askText("b", { initial: "x" });`,
      `if (!await askConfirm("c")) return;`,
      `const f = opts.confirm ?? askConfirm;`,
    ];
    for (const src of good) expect(askCalls("p.ts", src).unawaited, src).toEqual([]);
    expect(askCalls("p.ts", `const f = opts.confirm ?? askConfirm;`).all, "取值不是调用").toEqual([]);
  });

  it("★ 生产 TS 里没 await 的对话框调用 == ∅；调用它的文件 == 手写清单（两向）", () => {
    const files = productionTsFiles("src").filter((f) => f.file !== "src/frontend/ui/ask-dialog.ts");
    const per = files.map((f) => ({ file: f.file, ...askCalls(f.file, f.text) }));
    expect(per.flatMap((p) => p.unawaited).map((h) => `${h.file}:${h.line}  ${h.text}`)).toEqual([]);
    expect(
      per.filter((p) => p.all.length > 0).map((p) => p.file).sort(),
      "调用对话框的文件变了：新长的调用点要进这张清单（并确认它 await 了）",
    ).toEqual([...ASK_CALLERS].sort());
  }, SCAN_TIMEOUT_MS); // 全仓生产 TS 逐份建 AST：整套并跑时 5 s 默认期限不够（现打 7.8 s）
});

// ─────────────────────────────── D2 ───────────────────────────────

const dialog = (): HTMLElement | null => document.querySelector<HTMLElement>('[role="dialog"]');
const buttons = (): HTMLButtonElement[] => [...(dialog()?.querySelectorAll("button") ?? [])];
const okBtn = (): HTMLButtonElement => buttons()[1];
const cancelBtn = (): HTMLButtonElement => buttons()[0];
const escape = (): void => {
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", code: "Escape", bubbles: true }));
};

describe("D2 · askConfirm / askText 的结算语义", () => {
  beforeAll(() => {
    // 与主窗 / 设置窗启动时同一条路（INVARIANTS「复用 dispatcher 的独立窗口必须自调 start ＋ applyOverrides」）：
    // Esc 经 overlay 栈到栈顶。
    dispatcher.applyOverrides({});
    dispatcher.start();
  });
  afterEach(() => {
    // 每条结束时不许留下没结算的对话框（留下 = 永挂的 Promise）
    expect(dialog(), "有一个对话框没结算就被留在了页面上").toBeNull();
  });

  it("确定 ⇒ true；遮罩挂在 document.body 下（INVARIANTS §13）；正文是纯文本、换行原样", async () => {
    const p = askConfirm("删掉 <b>x</b>？\n\n不可恢复。");
    const d = dialog()!;
    expect(d.parentElement?.parentElement, "遮罩没挂在 body 上").toBe(document.body);
    expect(d.querySelector("b"), "正文被当成 HTML 解释了").toBeNull();
    expect(d.textContent).toContain("删掉 <b>x</b>？\n\n不可恢复。");
    expect(document.activeElement, "确认框打开时焦点不在「确定」上").toBe(okBtn());
    okBtn().click();
    await expect(p).resolves.toBe(true);
  });

  it("取消按钮 · 点遮罩 · Esc ⇒ false", async () => {
    const a = askConfirm("a");
    cancelBtn().click();
    await expect(a).resolves.toBe(false);

    const b = askConfirm("b");
    dialog()!.parentElement!.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    await expect(b).resolves.toBe(false);

    const c = askConfirm("c");
    escape();
    await expect(c).resolves.toBe(false);
  });

  it("点面板本身（不是遮罩）不结算", async () => {
    const p = askConfirm("p");
    dialog()!.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(dialog(), "点了面板就关了").not.toBeNull();
    okBtn().click();
    await expect(p).resolves.toBe(true);
  });

  it("Esc 只关对话框，不连带关下面那层 overlay", async () => {
    let under = 0;
    const below = { handleEsc: () => ((under += 1), true) };
    dispatcher.pushOverlay(below);
    const p = askConfirm("上面那层");
    escape();
    await expect(p).resolves.toBe(false);
    expect(under, "Esc 连带关了下面那层").toBe(0);
    escape();
    expect(under, "对话框走了之后，下面那层没拿回栈顶").toBe(1);
    dispatcher.popOverlay(below);
  });

  it("新开一个会把上一个按取消结算（不留永挂的 Promise）", async () => {
    const first = askConfirm("第一个");
    const second = askConfirm("第二个");
    await expect(first).resolves.toBe(false);
    expect(document.querySelectorAll('[role="dialog"]').length).toBe(1);
    expect(dialog()!.textContent).toContain("第二个");
    okBtn().click();
    await expect(second).resolves.toBe(true);
  });

  it("只认第一次结算：确定之后再 Esc 不改答案", async () => {
    const p = askConfirm("x");
    okBtn().click();
    escape();
    await expect(p).resolves.toBe(true);
  });

  it("askText：初值全选；Enter / 确定 ⇒ 原值（不 trim）；取消一类 ⇒ null（与空串分开）", async () => {
    const a = askText("名字", { initial: "旧名" });
    const inp = dialog()!.querySelector("input")!;
    expect(inp.value).toBe("旧名");
    expect(document.activeElement).toBe(inp);
    expect([inp.selectionStart, inp.selectionEnd]).toEqual([0, 2]);
    inp.value = "  新名 ";
    inp.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    await expect(a).resolves.toBe("  新名 ");

    const b = askText("名字");
    dialog()!.querySelector("input")!.value = "";
    okBtn().click();
    await expect(b).resolves.toBe("");

    const c = askText("名字", { initial: "x" });
    escape();
    await expect(c).resolves.toBeNull();
  });

  it("确定按钮的字可以换；默认两颗按钮的字取自文案表", async () => {
    const p = askConfirm("x", { okLabel: "删除" });
    expect(buttons().map((b) => b.textContent)).toEqual(["取消", "删除"]);
    cancelBtn().click();
    await p;
    const q = askConfirm("y");
    expect(buttons().map((b) => b.textContent)).toEqual(["取消", "确定"]);
    okBtn().click();
    await q;
  });

  it("结算后焦点回到打开之前的那个元素", async () => {
    const before = document.createElement("button");
    document.body.appendChild(before);
    before.focus();
    const p = askConfirm("x");
    expect(document.activeElement).not.toBe(before);
    cancelBtn().click();
    await p;
    expect(document.activeElement).toBe(before);
    before.remove();
  });
});

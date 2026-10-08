/**
 * 应用内对话框收口的判据。
 *
 * # 要求住址
 *
 * - 「全仓 `window.prompt` / `window.confirm` 收口 …… 独立一件，牵扯 `plugin:dialog` 被 ACL 拒」。
 * - 「全仓 `window.prompt/confirm` 的收口 | 独立一件（牵扯 `plugin:dialog` 的 ACL）」。
 * - `INVARIANTS §13`「任何用 `position: fixed` 实现的浮层 …… **必须**挂到 `document.body`」。
 *
 * # 为什么这不是洁癖（`src/frontend/ui/kit/dialog.ts` 头注）
 *
 * `tauri-plugin-dialog` 往 webview 注入的 `window.confirm` 是 `async` 的 ⇒ 返回 Promise、永远真值 ⇒
 * 真 app 里每一处 `if (!window.confirm(m)) return;` 都不拦。jsdom 的 `window.confirm` 是同步原生实现，
 * 所以这件事在单测里从来看不见。
 *
 * | 格 | 判什么 | 形态 |
 * |---|---|---|
 * | D1 | 生产 TS 里引用原生 `confirm` / `prompt`（裸调用，或 `window.` / `globalThis.` / `self.` 成员，调用或取值） | TS AST，**零命中**；正控：合成样本逐形各恰 1 处、近似形 0 处 |
 * | D1b | 生产 TS 里每一处**调用** `confirmDialog` / `askText` / `confirmInterrupts` 都是 `await` 的操作数（没 `await` 的 Promise 恒真值 —— 正是原生替身那个病） | TS AST，未 await 的调用 **零命中**；正控同上；另钉「被调用的地方恰好是改过的那几份文件」两向相等 |
 * | D2 | `confirmDialog` / `askText` 的结算语义 | 确定 / 取消 / 遮罩 / Esc（经真 `dispatcher`）/ Enter / 顶掉上一个 / 挂 body / 正文不解释 HTML · 标题与动作名 · 逐项清单 · 填了东西点遮罩不关 · 框里报错 · Tab 不出框 |
 *
 * `alert` 不在 D1：它归 `INVARIANTS §12`，由 `tests/frontend/ui/invariants-frontend-guard.vitest.ts` ① 守。
 */
import { describe, it, expect, beforeAll, afterEach } from "vitest";
import ts from "typescript";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { REPO_ROOT } from "../../../test-support/repo-root";

import { confirmDialog, askText, formDialog, LIST_MAX, type ConfirmSpec } from "../../../../src/frontend/ui/kit/dialog";
import { dispatcher } from "../../../../src/frontend/ui/keybindings/registry";
import { productionTsFiles, SCAN_TIMEOUT_MS } from "../../../test-support/production-sources.ts";
import { copyText } from "../../../../src/frontend/ui/copy-table";

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
      `await confirmDialog({ title: "x", action: "y" });`,
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
      files.some((f) => f.file === "src/frontend/ui/kit/dialog.ts"),
      "人群里没有对话框模块本身 —— 遍历口径变了",
    ).toBe(true);
    const hits = files.flatMap((f) => nativeDialogRefs(f.file, f.text));
    expect(
      hits.map((h) => `${h.file}:${h.line}  ${h.text}`),
      "这些地方还在用原生弹窗 —— 真 app 里 window.confirm 返回 Promise（永远真值），等于没问；改用 src/frontend/ui/kit/dialog.ts 的 confirmDialog / askText",
    ).toEqual([]);
  }, SCAN_TIMEOUT_MS); // 全仓生产 TS 逐份建 AST：整套并跑时 5 s 默认期限不够（现打 7.8 s）
});

// ─────────────────────────────── D1b ───────────────────────────────

const ASKS = new Set(["confirmDialog", "askText", "confirmInterrupts"]);

/** 一份源码里调用对话框的地方：`[全部调用, 其中没被 await 的]`。 */
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
  "src/frontend/ui/acct-panel.ts", // 账号面板的重启切换：「会打断什么」那一问
  "src/frontend/ui/backend-deploy.ts", // 换上这一版之前「会打断什么」那一问（机器卡与 ↗ 浮层共用）
  "src/frontend/ui/keybindings/editor.ts",
  "src/frontend/ui/settings/cc-bus-section.ts",
  "src/frontend/ui/settings/config-page.ts",
  "src/frontend/ui/settings/machine-card.ts",
  "src/frontend/ui/settings/panel.ts",
  "src/frontend/ui/tab-batch-menu.ts",
  "src/frontend/ui/tab-menu.ts",
  "src/frontend/ui/views/history.ts",
];

describe("D1b · 对话框的答案一律 await", () => {
  it("🔴 正控：没 await 的调用认得出，await 的（含括号、`!await`）不认", () => {
    const bad = [`if (!confirmDialog(s)) return;`, `const ok = askText(t);`, `void confirmDialog(s);`];
    for (const src of bad) expect(askCalls("p.ts", src).unawaited.length, src).toBe(1);
    const good = [
      `if (!(await confirmDialog(s))) return;`,
      `const n = await askText(t);`,
      `if (!await confirmDialog(s)) return;`,
      `const f = opts.confirm ?? confirmDialog;`,
    ];
    for (const src of good) expect(askCalls("p.ts", src).unawaited, src).toEqual([]);
    expect(askCalls("p.ts", `const f = opts.confirm ?? confirmDialog;`).all, "取值不是调用").toEqual([]);
  });

  it("★ 生产 TS 里没 await 的对话框调用 == ∅；调用它的文件 == 手写清单（两向）", () => {
    const files = productionTsFiles("src").filter((f) => !f.file.startsWith("src/frontend/ui/kit/"));
    const per = files.map((f) => ({ file: f.file, ...askCalls(f.file, f.text) }));
    expect(per.flatMap((p) => p.unawaited).map((h) => `${h.file}:${h.line}  ${h.text}`)).toEqual([]);
    expect(
      per.filter((p) => p.all.length > 0).map((p) => p.file).sort(),
      "调用对话框的文件变了：新长的调用点要进这张清单（并确认它 await 了）",
    ).toEqual([...ASK_CALLERS].sort());
  }, SCAN_TIMEOUT_MS); // 全仓生产 TS 逐份建 AST：整套并跑时 5 s 默认期限不够（现打 7.8 s）
});

// ─────────────────────────────── D2 ───────────────────────────────

const dialog = (): HTMLElement | null => document.querySelector<HTMLElement>('[aria-modal="true"]');
const buttons = (): HTMLButtonElement[] => [...(dialog()?.querySelectorAll("button") ?? [])];
const okBtn = (): HTMLButtonElement => buttons()[1];
const cancelBtn = (): HTMLButtonElement => buttons()[0];
const escape = (): void => {
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", code: "Escape", bubbles: true }));
};
const backdropDown = (): void => {
  dialog()!.parentElement!.dispatchEvent(new MouseEvent("mousedown", { bubbles: true }));
};
const ask = (over: Partial<ConfirmSpec> = {}): Promise<boolean> => confirmDialog({ title: "结束会话 orders", action: "结束", ...over });

describe("D2 · confirmDialog / askText 的结算语义（C10）", () => {
  beforeAll(() => {
    // 与主窗 / 设置窗启动时同一条路：Esc 经弹层栈到栈顶。
    dispatcher.applyOverrides({});
    dispatcher.start();
  });
  afterEach(() => {
    expect(dialog(), "有一个对话框没结算就被留在了页面上").toBeNull();
  });

  it("标题 ＋ 正文 ＋ 取消在左 · 动作在右；确认 ⇒ true；遮罩挂 body；正文是纯文本", async () => {
    const p = ask({ body: "删掉 <b>x</b>\n\n撤不回" });
    const d = dialog()!;
    expect(d.parentElement?.parentElement, "遮罩没挂在 body 上").toBe(document.body);
    expect(d.querySelector("h2")?.textContent).toBe("结束会话 orders");
    expect(d.querySelector("b"), "正文被当成 HTML 解释了").toBeNull();
    expect(d.textContent).toContain("删掉 <b>x</b>\n\n撤不回");
    expect(buttons().map((b) => b.textContent)).toEqual([copyText("kit.dialog.cancel"), "结束"]);
    expect(document.activeElement, "一般确认打开时焦点在动作键上").toBe(okBtn());
    okBtn().click();
    await expect(p).resolves.toBe(true);
  });

  it("危险确认：确认键红（data-kind=danger）、默认焦点在「取消」", async () => {
    const p = ask({ danger: true });
    expect(okBtn().dataset.kind).toBe("danger");
    expect(document.activeElement).toBe(cancelBtn());
    cancelBtn().click();
    await expect(p).resolves.toBe(false);
  });

  it("列出会断 / 会改什么的框（不危险也算）：默认焦点在「取消」；什么都不列的普通确认：焦点在动作键", async () => {
    const a = ask({ rows: [{ label: copyText("interrupts.row.brief"), items: [copyText("interrupts.item.relayed", { n: "2" })] }] });
    expect(okBtn().dataset.kind).not.toBe("danger");
    expect(document.activeElement).toBe(cancelBtn());
    cancelBtn().click();
    await a;
    const b = ask({ rows: [{ label: copyText("kit.interrupts.cut"), items: [] }] });
    expect(document.activeElement).toBe(okBtn());
    cancelBtn().click();
    await b;
  });

  it("逐项那几行排成一张两列表：各行的标签与值同在一个两列网格里（标签列同宽、值列左对齐）", async () => {
    const p = ask({
      rows: [
        { label: copyText("interrupts.row.untilStart"), items: ["a"] },
        { label: "保留", items: ["b"] },
      ],
    });
    const labels = [...dialog()!.querySelectorAll("span")].filter((e) => e.textContent === copyText("interrupts.row.untilStart") || e.textContent === "保留");
    expect(labels).toHaveLength(2);
    const grid = labels[0]!.parentElement!.parentElement!;
    expect(labels[1]!.parentElement!.parentElement).toBe(grid);
    const css = readFileSync(resolve(REPO_ROOT, "src/frontend/ui/kit/dialog.module.css"), "utf8");
    const rule = (name: string): string => css.match(new RegExp(`\\.${name} \\{([^}]*)\\}`))?.[1] ?? "";
    const gridClass = [...grid.classList].find((c) => /dialogRows/.test(c)) ?? "";
    expect(gridClass).not.toBe("");
    expect(rule("dialogRows")).toMatch(/display: grid;[\s\S]*grid-template-columns: max-content minmax\(0, 1fr\)/);
    expect(rule("dialogRow")).toMatch(/display: contents/);
    cancelBtn().click();
    await p;
  });

  it("逐项 `中断` / `保留`：空的那段不画；清单超过 8 项只列前 8 ＋「另外 n 个」", async () => {
    const p = ask({
      rows: [
        { label: copyText("kit.interrupts.cut"), items: [copyText("kit.interrupts.turn")] },
        { label: "保留", items: [] },
      ],
      list: Array.from({ length: LIST_MAX + 3 }, (_, i) => `s${i}`),
    });
    const t = dialog()!.textContent ?? "";
    expect(t).toContain(copyText("kit.interrupts.cut"));
    expect(t).not.toContain("保留");
    expect(t).toContain(`s${LIST_MAX - 1}`);
    expect(t).not.toContain(`s${LIST_MAX}`);
    expect(t).toContain(copyText("kit.dialog.more", { n: "3" }));
    cancelBtn().click();
    await p;
  });

  it("取消按钮 · 点遮罩 · Esc ⇒ false；点面板本身不结算", async () => {
    const a = ask();
    cancelBtn().click();
    await expect(a).resolves.toBe(false);
    const b = ask();
    backdropDown();
    await expect(b).resolves.toBe(false);
    const c = ask();
    escape();
    await expect(c).resolves.toBe(false);
    const d = ask();
    dialog()!.dispatchEvent(new MouseEvent("mousedown", { bubbles: true }));
    expect(dialog(), "点了面板就关了").not.toBeNull();
    okBtn().click();
    await expect(d).resolves.toBe(true);
  });

  it("Esc 只关对话框，不连带关下面那层", async () => {
    let under = 0;
    const below = { handleEsc: () => ((under += 1), true) };
    dispatcher.pushOverlay(below);
    const p = ask();
    escape();
    await expect(p).resolves.toBe(false);
    expect(under, "Esc 连带关了下面那层").toBe(0);
    escape();
    expect(under, "对话框走了之后，下面那层没拿回栈顶").toBe(1);
    dispatcher.popOverlay(below);
  });

  it("新开一个会把上一个按取消结算；只认第一次结算", async () => {
    const first = ask({ title: "第一个" });
    const second = ask({ title: "第二个" });
    await expect(first).resolves.toBe(false);
    expect(document.querySelectorAll('[aria-modal="true"]').length).toBe(1);
    expect(dialog()!.textContent).toContain("第二个");
    okBtn().click();
    escape();
    await expect(second).resolves.toBe(true);
  });

  it("Tab 只在框内循环（末尾 → 第一个，Shift+Tab 反过来）", async () => {
    const p = ask();
    okBtn().focus();
    dialog()!.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true }));
    expect(document.activeElement).toBe(cancelBtn());
    dialog()!.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", shiftKey: true, bubbles: true }));
    expect(document.activeElement).toBe(okBtn());
    cancelBtn().click();
    await p;
  });

  it("askText：初值全选；Enter / 动作键 ⇒ 原值（不 trim）；取消 ⇒ null（与空串分开）", async () => {
    const a = askText({ title: "新建集合", action: "新建", initial: "旧名" });
    const inp = dialog()!.querySelector("input")!;
    expect(inp.value).toBe("旧名");
    expect(document.activeElement).toBe(inp);
    expect([inp.selectionStart, inp.selectionEnd]).toEqual([0, 2]);
    inp.value = "  新名 ";
    inp.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    await expect(a).resolves.toBe("  新名 ");

    const b = askText({ title: "t", action: "a" });
    okBtn().click();
    await expect(b).resolves.toBe("");

    const c = askText({ title: "t", action: "a", initial: "x" });
    escape();
    await expect(c).resolves.toBeNull();
  });

  it("askText：填了东西点遮罩不关（防丢输入）；没改动点遮罩 ＝ 取消", async () => {
    const a = askText({ title: "t", action: "a", initial: "x" });
    dialog()!.querySelector("input")!.value = "改过";
    backdropDown();
    expect(dialog(), "填了东西被遮罩关掉了").not.toBeNull();
    okBtn().click();
    await expect(a).resolves.toBe("改过");
    const b = askText({ title: "t", action: "a", initial: "x" });
    backdropDown();
    await expect(b).resolves.toBeNull();
  });

  it("askText 校验不过：错误写在框里、不关；改了再交就过", async () => {
    const p = askText({ title: "t", action: "a", validate: (v) => (v.trim() === "" ? "名字为空" : null) });
    okBtn().click();
    const d = dialog()!;
    expect(d, "校验不过就关了").not.toBeNull();
    expect(d.textContent).toContain("名字为空");
    expect(d.querySelector("input")!.getAttribute("aria-invalid")).toBe("true");
    d.querySelector("input")!.value = "ok";
    okBtn().click();
    await expect(p).resolves.toBe("ok");
  });

  it("结算后焦点回到打开之前的那个元素", async () => {
    const before = document.createElement("button");
    document.body.appendChild(before);
    before.focus();
    const p = ask();
    expect(document.activeElement).not.toBe(before);
    cancelBtn().click();
    await p;
    expect(document.activeElement).toBe(before);
    before.remove();
  });

  it("formDialog：拦着时主按钮禁用并说为什么；交了没成 ⇒ 框顶一条错、不关；成了 ⇒ 关、答 true", async () => {
    const body = document.createElement("div");
    const input = document.createElement("input");
    body.appendChild(input);
    let reply: string | null = "没存上";
    const h = formDialog({
      title: "t",
      action: "加",
      body,
      blocked: () => (input.value === "" ? "需填" : null),
      submit: async () => reply,
    });
    expect(okBtn().getAttribute("aria-disabled")).toBe("true");
    expect(okBtn().title).toBe("需填");
    okBtn().click();
    await Promise.resolve();
    expect(dialog(), "拦着还交了").not.toBeNull();
    input.value = "x";
    h.refresh();
    h.setAction("加一台");
    expect(okBtn().textContent).toBe("加一台");
    okBtn().click();
    for (let i = 0; i < 3; i++) await Promise.resolve();
    expect(dialog(), "没成就关了").not.toBeNull();
    expect(dialog()!.textContent).toContain("没存上");
    reply = null;
    okBtn().click();
    await expect(h.done).resolves.toBe(true);
    expect(dialog()).toBeNull();
  });

  it("formDialog 的宽度刻度：缺省不标（560）· wide 580 · narrow 520", () => {
    const sizes = [{}, { wide: true }, { narrow: true }].map((extra) => {
      formDialog({ title: "t", action: "a", body: document.createElement("div"), submit: async () => null, ...extra });
      const size = dialog()!.dataset.size ?? "";
      cancelBtn().click();
      return size;
    });
    expect(sizes).toEqual(["", "wide", "narrow"]);
  });
});

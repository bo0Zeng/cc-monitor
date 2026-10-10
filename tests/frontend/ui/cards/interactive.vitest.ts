// **「Claude 在等你决定」这两张卡不许被折叠 —— 这条承诺此前零覆盖。**
//
// # 怎么发现的
//
// 按「测试从未引用过的生产模块」筛（前端 116 个生产 .ts 里有 5 个），
// `cards/interactive.ts` 排在里面，而它导出的 `isInteractiveTool` 是个**纯谓词、有判断**。
// 先核缺口：把 `agent-profile.ts` 的 `interactiveTools` **清空**（等于这两个工具重新被折进
// 通用 🔧 工具组）—— `npm test` / `npx vitest run`（1277）/ `tsc` **三个门禁全绿**。
//
// 它的后果不是「样式变了」：`interactive.ts` 头注逐字写着，折叠会让
// **「用户看不出有问题在等他，误以为 LLM 还在输出」**。
//
// # 为什么这里**可以**把两个工具名写死，而上一条（bash 折叠阈值）不行
//
// 上一条钉的是**调参值**（30 行折叠 / 头部 20 行）——写死会让判据变成那个数的第二份副本，
// 合法微调就误红。**本条钉的是需求本身**：这两个工具之所以要特殊对待，是因为它们
// 「在等用户决定」，那不是可调的，是这条功能存在的理由。
// ⇒ 判据里写 `AskUserQuestion` / `ExitPlanMode` **就是需求的落点**。
//
// 〔判定只在后端〕「哪个工具算交互工具」今天住那台后端的适配层（`agents/claudecode/cards.rs`，
// 判据 `tests/backend/agents/claudecode/cards_tests.rs` 逐名钉着这两个），随 assistant 记录的 `toolCards` 带来；
// 界面这一半只剩「**照卡型办**」：卡型是 `interactive` ⇒ 整条消息走 `kind: "card"`（不进工具组折叠），没有卡型 ⇒ 工具组。
import { describe, it, expect } from "vitest";
import { buildInteractiveCard, settleInteractive } from "../../../../src/frontend/ui/cards/interactive";
import { renderMessage } from "../../../../src/frontend/ui/cards/index";
import { LOCAL_ORIGIN } from "../../../../src/frontend/ui/ipc/origin";
import type { LineRecord } from "../../../../src/frontend/ui/generated/LineRecord";
import { copyText } from "../../../../src/frontend/ui/copy-table";
import type { StepAsk } from "../../../../src/frontend/ui/generated/StepAsk";

describe("交互等待类工具：不许被折进通用工具组", () => {
  const ctx = () => ({
    parentPath: "/p/s.jsonl",
    origin: LOCAL_ORIGIN,
    toolUseNames: new Map(),
    toolUseElements: new Map(),
    pendingToolResults: new Map(),
    lazy: false,
  });
  const ask = (cards: Record<string, string> | undefined): LineRecord =>
    ({
      agent: "claude",
      t: "reply",
      id: "a",
      at: "2026-01-01T00:00:00.000Z",
      blocks: [{ type: "tool_use", id: "t1", name: "AskUserQuestion", input: { questions: [{ question: "q?", options: [{ label: "x" }] }] } }],
      autoReply: false,
      endsTurn: false,
      ...(cards ? { cards } : {}),
    }) as never;

  it("★ 后端说这是交互卡（`toolCards` 里是 `interactive`）⇒ 默认可见的一张卡，不进工具组", () => {
    expect(renderMessage(ask({ t1: "interactive" }), ctx() as never).kind, "问题卡被折叠 = 用户看不出有问题在等他").toBe("card");
  });

  it("★ 反向：没有卡型 ⇒ 普通工具组（界面不按工具名自己判 —— 否则上一条恒真、等于没判据）", () => {
    expect(renderMessage(ask(undefined), ctx() as never).kind).toBe("tool-group");
  });
});

describe("交互卡：按核心那一格（`steps[id].ask`）建，不认入参；没有那一格一律 throw，由调用方回退通用折叠卡", () => {
  // **双层防御的外层是 throw** —— 若它改成「静默渲染一张空卡」，用户会看到一张什么都没有的卡而不是通用工具卡。
  // 入参形状不对时核心不出那一格（`tests/backend/agents/claudecode/steps_tests.rs::questions_and_plans_come_out_as_their_own_cell`）。
  const opts = { lazy: false };
  const asked = (question: string, labels: string[]): StepAsk => ({
    kind: "questions",
    questions: [{ question, multi: false, options: labels.map((label) => ({ label })) }],
  });

  it("没有那一格 → throw（调用方据此走通用路径）", () => {
    expect(() => buildInteractiveCard(undefined, opts)).toThrow(/no ask cell/);
  });

  it("★ 正路要真渲染出来（否则上面全是 throw，等于只测了失败路径）", () => {
    const el = buildInteractiveCard(asked("选哪个方案", ["甲案", "乙案"]), opts);
    const text = el.textContent ?? "";
    expect(text).toContain("选哪个方案");
    expect(text).toContain("甲案");
    expect(text).toContain("乙案");
    // 「在等你」这件事必须是**看得见的**，不只是结构上分了一类。
    expect(text).toContain(copyText("interactive.ask.title"));
  });

  it("★ 出口省掉 `tool_use.input` 之后（`omit` 声明）：交互卡照样建成提问卡，不退成通用工具卡", () => {
    const rec = {
      agent: "claude",
      t: "reply",
      id: "a",
      at: "2026-01-01T00:00:00.000Z",
      blocks: [{ type: "tool_use", id: "t1", name: "AskUserQuestion" }],
      cards: { t1: "interactive" },
      steps: { t1: { tool: "AskUserQuestion", known: true, ask: asked("改哪一处", ["甲"]) } },
      autoReply: false,
      endsTurn: false,
    } as unknown as LineRecord;
    const ctx = { parentPath: "/p/s.jsonl", origin: LOCAL_ORIGIN, toolUseNames: new Map(), toolUseElements: new Map(), pendingToolResults: new Map(), lazy: false };
    const r = renderMessage(rec, ctx as never);
    expect(r.kind).toBe("card");
    const el = (r as { element: HTMLElement }).element;
    expect(el.querySelector(".block-ask")?.textContent).toContain("改哪一处");
  });

  it("★ 答了之后（B7）：标题换「提问」/「计划」、底行写后端读出的结果（已选「…」· 已批准 · 未批准），被选项高亮；不印英文原句", () => {
    const ask = buildInteractiveCard(asked("选哪个方案", ["甲案", "乙案"]), opts);
    settleInteractive(ask, { ok: true, answer: { kind: "picked", options: ["乙案"] }, text: copyText("interactive.done.picked", { option: "乙案" }) });
    expect(ask.querySelector(".block-ask-title")?.textContent).toBe(copyText("interactive.ask.done"));
    expect(ask.querySelector(".block-interactive-done")?.textContent).toBe(copyText("interactive.done.picked", { option: "乙案" }));
    expect([...ask.querySelectorAll(".ask-option.is-chosen")].map((li) => (li as HTMLElement).dataset.optionLabel)).toEqual(["乙案"]);
    const plan = buildInteractiveCard({ kind: "plan", text: "甲" }, opts);
    settleInteractive(plan, { ok: true, answer: { kind: "approved" }, text: copyText("interactive.done.approved") });
    expect([plan.querySelector(".block-plan-title")?.textContent, plan.querySelector(".block-interactive-done")?.textContent]).toEqual([copyText("interactive.plan.done"), copyText("interactive.done.approved")]);
    settleInteractive(plan, { ok: false, rejected: true, text: "" });
    expect(plan.querySelector(".block-interactive-done")?.textContent).toBe(copyText("interactive.done.rejected"));
    expect(plan.textContent).not.toMatch(/User has/);
  });
});

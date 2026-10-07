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
import type { JsonlRecord } from "../../../../src/frontend/ui/generated/JsonlRecord";
import { copyText } from "../../../../src/frontend/ui/copy-table";

describe("交互等待类工具：不许被折进通用工具组", () => {
  const ctx = () => ({
    parentPath: "/p/s.jsonl",
    origin: LOCAL_ORIGIN,
    toolUseNames: new Map(),
    toolUseElements: new Map(),
    pendingToolResults: new Map(),
    lazy: false,
  });
  const ask = (cards: Record<string, string> | undefined): JsonlRecord =>
    ({
      type: "assistant",
      uuid: "a",
      timestamp: "2026-01-01T00:00:00.000Z",
      message: {
        role: "assistant",
        content: [
          { type: "tool_use", id: "t1", name: "AskUserQuestion", input: { questions: [{ question: "q?", options: [{ label: "x" }] }] } },
        ],
      },
      ...(cards ? { toolCards: cards } : {}),
    }) as never;

  it("★ 后端说这是交互卡（`toolCards` 里是 `interactive`）⇒ 默认可见的一张卡，不进工具组", () => {
    expect(renderMessage(ask({ t1: "interactive" }), ctx() as never).kind, "问题卡被折叠 = 用户看不出有问题在等他").toBe("card");
  });

  it("★ 反向：没有卡型 ⇒ 普通工具组（界面不按工具名自己判 —— 否则上一条恒真、等于没判据）", () => {
    expect(renderMessage(ask(undefined), ctx() as never).kind).toBe("tool-group");
  });
});

describe("交互卡：畸形输入一律 throw，由调用方回退通用折叠卡", () => {
  // 头注逐字：「输入畸形一律 throw，由调用方（renderBlock）回退通用折叠卡
  // （INVARIANT § 17a/18 双层防御惯例）」。**双层防御的外层是 throw** ——
  // 若它改成「静默渲染一张空卡」，用户会看到一张什么都没有的卡而不是通用工具卡。
  const opts = { lazy: false };

  it("不是交互工具的名字 → throw（调用方据此走通用路径）", () => {
    expect(() => buildInteractiveCard("Bash", {}, opts)).toThrow(/not an interactive tool/);
  });

  it("AskUserQuestion：questions 缺失 / 空数组 / 非数组 → throw", () => {
    for (const bad of [{}, { questions: [] }, { questions: "nope" }, null, undefined]) {
      expect(() => buildInteractiveCard("AskUserQuestion", bad, opts)).toThrow(
        /malformed input\.questions/,
      );
    }
  });

  it("ExitPlanMode：plan 缺失 → throw", () => {
    expect(() => buildInteractiveCard("ExitPlanMode", {}, opts)).toThrow(/malformed input\.plan/);
  });

  it("★ 正路要真渲染出来（否则上面全是 throw，等于只测了失败路径）", () => {
    const el = buildInteractiveCard(
      "AskUserQuestion",
      { questions: [{ question: "选哪个方案", options: [{ label: "甲案" }, { label: "乙案" }] }] },
      opts,
    );
    const text = el.textContent ?? "";
    expect(text).toContain("选哪个方案");
    expect(text).toContain("甲案");
    expect(text).toContain("乙案");
    // 「在等你」这件事必须是**看得见的**，不只是结构上分了一类。
    expect(text).toContain(copyText("interactive.ask.title"));
  });

  it("★ 答了之后（B7）：标题换「提问」/「计划」、底行写后端读出的结果（已选「…」· 已批准 · 未批准），被选项高亮；不印英文原句", () => {
    const ask = buildInteractiveCard("AskUserQuestion", { questions: [{ question: "选哪个方案", options: [{ label: "甲案" }, { label: "乙案" }] }] }, opts);
    settleInteractive(ask, { ok: true, answer: { kind: "picked", options: ["乙案"] } });
    expect(ask.querySelector(".block-ask-title")?.textContent).toBe(copyText("interactive.ask.done"));
    expect(ask.querySelector(".block-interactive-done")?.textContent).toBe(copyText("interactive.done.picked", { option: "乙案" }));
    expect([...ask.querySelectorAll(".ask-option.is-chosen")].map((li) => (li as HTMLElement).dataset.optionLabel)).toEqual(["乙案"]);
    const plan = buildInteractiveCard("ExitPlanMode", { plan: "甲" }, opts);
    settleInteractive(plan, { ok: true, answer: { kind: "approved" } });
    expect([plan.querySelector(".block-plan-title")?.textContent, plan.querySelector(".block-interactive-done")?.textContent]).toEqual([copyText("interactive.plan.done"), copyText("interactive.done.approved")]);
    settleInteractive(plan, { ok: false, rejected: true });
    expect(plan.querySelector(".block-interactive-done")?.textContent).toBe(copyText("interactive.done.rejected"));
    expect(plan.textContent).not.toMatch(/User has/);
  });
});

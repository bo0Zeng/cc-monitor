// 〔audit-0805 08-06〕**「Claude 在等你决定」这两张卡不许被折叠 —— 这条承诺此前零覆盖。**
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
// ⇒ 判据里写 `AskUserQuestion` / `ExitPlanMode` **就是需求的落点**；
// `agent-profile.ts` 里那个集合是实现。从集合里删掉一个，就该红。
import { describe, it, expect } from "vitest";
import { isInteractiveTool, buildInteractiveCard } from "./interactive";

describe("交互等待类工具：不许被折进通用工具组", () => {
  it("★ AskUserQuestion 与 ExitPlanMode 必须被判成交互工具（从 profile 里删掉任一个，这条红）", () => {
    expect(isInteractiveTool("AskUserQuestion"), "问题卡被折叠 = 用户看不出有问题在等他").toBe(
      true,
    );
    expect(isInteractiveTool("ExitPlanMode"), "计划卡被折叠 = 用户看不出要他拍板").toBe(true);
  });

  it("★ 反向：普通工具不许被判成交互工具（否则本谓词恒真、等于没判据）", () => {
    for (const name of ["Bash", "Read", "Edit", "Task", ""]) {
      expect(isInteractiveTool(name), `${name} 不该走交互卡`).toBe(false);
    }
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
    expect(text).toContain("等待你的选择");
  });
});

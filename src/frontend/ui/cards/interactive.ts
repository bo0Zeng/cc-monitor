/**
 * issue #21：交互等待类 tool_use 的**默认展开**渲染。
 *
 * `AskUserQuestion` / `ExitPlanMode` 是"Claude 在等用户决定"——折叠在通用
 * 🔧 工具条/工具组里时，用户看不出有问题在等他，误以为 LLM 还在输出。
 * 这两个工具走本模块：问题+选项 / plan 正文直接可见，且 renderMessage 把含
 * 它们的消息按 `kind:"card"` 处理（不进 card-tool-group 折叠，见 index.ts）。
 *
 * 题目 / 选项 / 计划正文是核心从入参里读好的一格（`ToolStep.ask`，各家的格式知识在 `agents/<名>/steps.rs`）：
 * 出口省掉 `tool_use.input` 之后照样建得出来。DOM 惯例同 diff.ts：纯 createElement + textContent，
 * 唯计划正文是 markdown → renderMarkdown（内建 DOMPurify）。没有那一格 throw，由调用方（renderBlock）回退通用折叠卡。
 */
import { renderMarkdown } from "../render";
import { copyText } from "../copy-table";
import type { StepResult } from "../generated/StepResult";
import type { StepAsk } from "../generated/StepAsk";
import type { AskQuestion } from "../generated/AskQuestion";

/**
 * 分发入口：按核心给的那一格（`ToolStep.ask`，从入参里读出来的提问 / 计划）建卡，不认入参结构。
 * 返回的元素内含 `.block-body-wrap`（tool_result 注入靶点，结构同 buildToolUseCard），调用方负责 `ctx.toolUseElements.set(block.id, el)`。
 * 没有那一格 → throw，调用方回退通用折叠卡。
 */
export function buildInteractiveCard(ask: StepAsk | undefined, opts: { lazy?: boolean }): HTMLElement {
  if (ask?.kind === "questions") return buildAskCard(ask.questions);
  if (ask?.kind === "plan") return buildPlanCard(ask.text, opts);
  throw new Error("interactive card: no ask cell");
}

function buildAskCard(questions: AskQuestion[]): HTMLElement {
  const root = document.createElement("div");
  root.className = "block-ask";

  const title = document.createElement("div");
  title.className = "block-ask-title";
  title.textContent = copyText("interactive.ask.title");
  root.appendChild(title);

  for (const q of questions) {
    const qEl = document.createElement("div");
    qEl.className = "ask-question";

    const qLine = document.createElement("div");
    qLine.className = "ask-q-line";
    if (q.header) {
      const chip = document.createElement("span");
      chip.className = "ask-header-chip";
      chip.textContent = q.header;
      qLine.appendChild(chip);
    }
    const qText = document.createElement("span");
    qText.className = "ask-q-text";
    qText.textContent = q.multi ? copyText("interactive.ask.multi", { question: q.question }) : q.question;
    qLine.appendChild(qText);
    qEl.appendChild(qLine);

    const ul = document.createElement("ul");
    ul.className = "ask-options";
    for (const opt of q.options) {
      const li = document.createElement("li");
      li.className = "ask-option";
      li.dataset.optionLabel = opt.label;
      const label = document.createElement("span");
      label.className = "ask-option-label";
      label.textContent = opt.label;
      li.appendChild(label);
      if (opt.description) {
        const desc = document.createElement("span");
        desc.className = "ask-option-desc";
        desc.textContent = opt.description;
        li.appendChild(desc);
      }
      ul.appendChild(li);
    }
    qEl.appendChild(ul);
    root.appendChild(qEl);
  }

  // tool_result 注入靶点（injectOrBuildToolResult 找 .block-body-wrap）
  const wrap = document.createElement("div");
  wrap.className = "block-body-wrap";
  root.appendChild(wrap);
  return root;
}

function buildPlanCard(plan: string, opts: { lazy?: boolean }): HTMLElement {
  const root = document.createElement("div");
  root.className = "block-plan";

  const title = document.createElement("div");
  title.className = "block-plan-title";
  title.textContent = copyText("interactive.plan.title");
  root.appendChild(title);

  const body = document.createElement("div");
  body.className = "block-plan-body block-body-md";
  body.innerHTML = renderMarkdown(plan, { lazy: opts.lazy });
  root.appendChild(body);

  const wrap = document.createElement("div");
  wrap.className = "block-body-wrap";
  root.appendChild(wrap);
  return root;
}

/**
 * 答了之后：左条去掉、标题换成「提问」/「计划」、底行写答了什么（已批准 · 已选「…」· 未批准）；
 * 提问卡里被选的那几项高亮（按后端给的选项原文比对选项标签，排版）。答了什么由后端从结果里读出（`toolResults[].answer` / `.rejected`），
 * 界面不读 Claude Code 的英文原句。
 */
export function settleInteractive(host: HTMLElement, res: StepResult): void {
  const ask = host.classList.contains("block-ask");
  if (!ask && !host.classList.contains("block-plan")) return;
  host.classList.add("is-answered");
  const title = host.querySelector<HTMLElement>(ask ? ".block-ask-title" : ".block-plan-title");
  if (title) title.textContent = ask ? copyText("interactive.ask.done") : copyText("interactive.plan.done");
  const picked = res.answer?.kind === "picked" ? res.answer.options : [];
  if (picked.length > 0) {
    const chosen = new Set(picked.flatMap((o) => [o, ...o.split(", ")]));
    host.querySelectorAll<HTMLElement>(".ask-option").forEach((li) => {
      if (chosen.has(li.dataset.optionLabel ?? "")) li.classList.add("is-chosen");
    });
  }
  const done =
    res.rejected || !res.ok
      ? copyText("interactive.done.rejected")
      : picked.length > 0
        ? copyText("interactive.done.picked", { option: picked.join(" / ") })
        : res.answer?.kind === "approved"
          ? copyText("interactive.done.approved")
          : "";
  let line = host.querySelector<HTMLElement>(":scope > .block-interactive-done");
  if (!done) {
    line?.remove();
    return;
  }
  if (!line) {
    line = document.createElement("div");
    line.className = "block-interactive-done";
    host.appendChild(line);
  }
  line.dataset.rejected = String(res.rejected || !res.ok);
  line.textContent = done;
}

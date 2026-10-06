/**
 * issue #21：交互等待类 tool_use 的**默认展开**渲染。
 *
 * `AskUserQuestion` / `ExitPlanMode` 是"Claude 在等用户决定"——折叠在通用
 * 🔧 工具条/工具组里时，用户看不出有问题在等他，误以为 LLM 还在输出。
 * 这两个工具走本模块：问题+选项 / plan 正文直接可见，且 renderMessage 把含
 * 它们的消息按 `kind:"card"` 处理（不进 card-tool-group 折叠，见 index.ts）。
 *
 * DOM 惯例同 diff.ts：纯 createElement + textContent（label/description 任意
 * 字符按字面安全），唯 ExitPlanMode 的 plan 是 markdown → renderMarkdown
 * （内建 DOMPurify）。输入畸形一律 throw，由调用方（renderBlock）回退通用折叠卡
 * （INVARIANT § 17a/18 双层防御惯例）。
 */
import { renderMarkdown } from "../render";
import { copyText } from "../copy-table";
import type { StepResult } from "../generated/StepResult";

// `isInteractiveTool`〔散文墓碑〕删：哪个 tool_use 在等用户决定由那台后端判（卡型 `interactive`，随记录成品带来）。

/** AskUserQuestion 的 input.questions[]（Claude Code 端 schema，实测 2026-06）。 */
interface AskOption {
  label: string;
  description?: string;
}
interface AskQuestion {
  question: string;
  header?: string;
  multiSelect?: boolean;
  options: AskOption[];
}

/**
 * 分发入口。返回的元素内含 `.block-body-wrap`（tool_result 注入靶点，结构同
 * buildToolUseCard），调用方负责 `ctx.toolUseElements.set(block.id, el)`。
 * 畸形 input → throw，调用方回退。
 */
export function buildInteractiveCard(
  name: string,
  input: unknown,
  opts: { lazy?: boolean },
): HTMLElement {
  if (name === "AskUserQuestion") return buildAskCard(input);
  if (name === "ExitPlanMode") return buildPlanCard(input, opts);
  throw new Error(`not an interactive tool: ${name}`);
}

function buildAskCard(input: unknown): HTMLElement {
  const questions = (input as { questions?: unknown })?.questions;
  if (!Array.isArray(questions) || questions.length === 0) {
    throw new Error("AskUserQuestion: malformed input.questions");
  }

  const root = document.createElement("div");
  root.className = "block-ask";

  const title = document.createElement("div");
  title.className = "block-ask-title";
  title.textContent = copyText("interactive.ask.title");
  root.appendChild(title);

  for (const raw of questions) {
    const q = raw as AskQuestion;
    if (typeof q?.question !== "string" || !Array.isArray(q?.options)) {
      throw new Error("AskUserQuestion: malformed question entry");
    }
    const qEl = document.createElement("div");
    qEl.className = "ask-question";

    const qLine = document.createElement("div");
    qLine.className = "ask-q-line";
    if (typeof q.header === "string" && q.header) {
      const chip = document.createElement("span");
      chip.className = "ask-header-chip";
      chip.textContent = q.header;
      qLine.appendChild(chip);
    }
    const qText = document.createElement("span");
    qText.className = "ask-q-text";
    qText.textContent = q.multiSelect ? copyText("interactive.ask.multi", { question: q.question }) : q.question;
    qLine.appendChild(qText);
    qEl.appendChild(qLine);

    const ul = document.createElement("ul");
    ul.className = "ask-options";
    for (const rawOpt of q.options) {
      const opt = rawOpt as AskOption;
      if (typeof opt?.label !== "string") {
        throw new Error("AskUserQuestion: malformed option");
      }
      const li = document.createElement("li");
      li.className = "ask-option";
      li.dataset.optionLabel = opt.label;
      const label = document.createElement("span");
      label.className = "ask-option-label";
      label.textContent = opt.label;
      li.appendChild(label);
      if (typeof opt.description === "string" && opt.description) {
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

function buildPlanCard(input: unknown, opts: { lazy?: boolean }): HTMLElement {
  const plan = (input as { plan?: unknown })?.plan;
  if (typeof plan !== "string" || plan.trim().length === 0) {
    throw new Error("ExitPlanMode: malformed input.plan");
  }

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
 * 答了之后（主窗口稿 §5.2.7 · B7）：左条去掉、标题换成「提问」/「计划」、底行写答了什么（已批准 · 已选「…」· 未批准）；
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

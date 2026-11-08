/**
 * `!` bash 模式的输入 / 输出卡（Batch4-F16）。认它们、拆出命令与 stdout/stderr（含实体反转义）都在后端
 * （记录成品的 `userText.speaker`），这里只画。渲染全部 createElement + textContent，零 innerHTML。
 */

import { copyText } from "../copy-table";
import { icon } from "../kit/icon";
import type { TextFold } from "../generated/TextFold";

export interface BashInput {
  command: string;
}

/** 输出与各自的折法（后端 `face_core::record::output_fold` 定折不折、露几行、展开那一句；界面不留阈值）。 */
export interface BashOutput {
  stdout: string;
  stderr: string;
  stdoutFold?: TextFold;
  stderrFold?: TextFold;
}

/** 终端风格命令卡：提示符箭头 ＋ npm install && npm run build */
export function buildBashInputCard(
  input: BashInput,
  /** 记录的钟面（后端写好的 `timeText`）。 */
  time: string,
): HTMLElement {
  const card = document.createElement("div");
  card.className = "card card-bash-input";

  const prompt = document.createElement("span");
  prompt.className = "bash-prompt";
  prompt.appendChild(icon("caretRight", "compact"));
  card.appendChild(prompt);

  const cmd = document.createElement("code");
  cmd.className = "bash-cmd";
  cmd.textContent = input.command;
  card.appendChild(cmd);

  const ts = document.createElement("span");
  ts.className = "bash-ts";
  ts.textContent = time;
  card.appendChild(ts);

  return card;
}

/** stdout/stderr 输出卡：stderr 红色调标注，超长折叠（沿 block-body-show-full 按钮惯例）。 */
export function buildBashOutputCard(
  output: BashOutput,
  /** 记录的钟面（后端写好的 `timeText`）。 */
  time: string,
): HTMLElement {
  const card = document.createElement("div");
  card.className = "card card-bash-output";

  const header = document.createElement("div");
  header.className = "bash-output-header";
  const mark = document.createElement("span");
  mark.className = "bash-prompt";
  mark.appendChild(icon("terminal", "compact"));
  const label = document.createElement("span");
  label.className = "bash-output-label";
  label.textContent = copyText("bash.output.title");
  const ts = document.createElement("span");
  ts.className = "bash-ts";
  ts.textContent = time;
  header.append(mark, label, ts);
  card.appendChild(header);

  const stdout = output.stdout.trim();
  const stderr = output.stderr.trim();
  if (stdout) {
    card.appendChild(buildOutputPre(stdout, "bash-stdout-body", output.stdoutFold));
  }
  if (stderr) {
    const errLabel = document.createElement("div");
    errLabel.className = "bash-stderr-label";
    errLabel.textContent = "✖ stderr";
    card.appendChild(errLabel);
    card.appendChild(buildOutputPre(stderr, "bash-stderr-body", output.stderrFold));
  }
  if (!stdout && !stderr) {
    const empty = document.createElement("div");
    empty.className = "bash-output-empty";
    empty.textContent = copyText("bash.output.empty");
    card.appendChild(empty);
  }
  return card;
}

/** 一份输出：后端给了折法 ⇒ 先露它说的那几行、加展开那一颗（字照后端那一句）；否则整份照排。 */
function buildOutputPre(text: string, cls: string, fold: TextFold | undefined): HTMLElement {
  const pre = document.createElement("pre");
  pre.className = `bash-output-body ${cls}`;
  if (!fold) {
    pre.textContent = text;
    return pre;
  }
  pre.textContent = text.split("\n").slice(0, fold.shown).join("\n");
  const wrap = document.createElement("div");
  wrap.className = "block-body-truncated-wrap";
  wrap.appendChild(pre);
  const expand = document.createElement("button");
  expand.type = "button";
  expand.className = "block-body-show-full";
  expand.textContent = fold.more;
  expand.addEventListener(
    "click",
    () => {
      pre.textContent = text;
      expand.remove();
    },
    { once: true },
  );
  wrap.appendChild(expand);
  return wrap;
}

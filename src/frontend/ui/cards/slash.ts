/**
 * 斜杠命令卡（`/compact`、`/loop` …）。认它、拆出命令名与参数（含实体反转义、标签顺序随版本漂）都在后端
 * （记录成品的 `userText.speaker`），这里只画。
 */

import { icon } from "../kit/icon";

export interface SlashCommand {
  name: string;
  args: string;
}

/** 紧凑渲染：⌘ /compact arg1 arg2 */
export function buildSlashCommandCard(
  cmd: SlashCommand,
  /** 记录的钟面（后端写好的 `timeText`）。 */
  time: string,
): HTMLElement {
  const card = document.createElement("div");
  card.className = "card card-slash";

  const mark = document.createElement("span");
  mark.className = "slash-icon";
  mark.appendChild(icon("command", "compact"));
  card.appendChild(mark);

  const name = document.createElement("span");
  name.className = "slash-name";
  name.textContent = cmd.name;
  card.appendChild(name);

  if (cmd.args) {
    const args = document.createElement("span");
    args.className = "slash-args";
    args.textContent = cmd.args;
    card.appendChild(args);
  }

  const ts = document.createElement("span");
  ts.className = "slash-ts";
  ts.textContent = time;
  card.appendChild(ts);

  return card;
}

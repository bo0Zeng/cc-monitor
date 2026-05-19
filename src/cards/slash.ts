/**
 * 解析 user 消息里的斜杠命令标记。
 *
 * Claude Code 把 `/compact`、`/loop` 这类命令写到 JSONL 时，content 形如：
 *
 *   <command-name>/compact</command-name>
 *               <command-message>compact</command-message>
 *               <command-args></command-args>
 *
 * 我们把它识别出来，渲染成一行紧凑卡片，省得用户看一坨 XML。
 *
 * 注：`/clear`、`/help`、`/model` 等 CLI-only 命令不会写 JSONL，物理上识别不到。
 */

export interface SlashCommand {
  name: string;
  args: string;
}

const RE = /^\s*<command-name>([^<]+)<\/command-name>\s*<command-message>[^<]*<\/command-message>\s*<command-args>([^<]*)<\/command-args>\s*$/s;

export function parseSlashCommand(text: string): SlashCommand | null {
  const m = RE.exec(text);
  if (!m) return null;
  return {
    name: m[1].trim(),
    args: m[2].trim(),
  };
}

/** 紧凑渲染：⌘ /compact arg1 arg2 */
export function buildSlashCommandCard(
  cmd: SlashCommand,
  timestamp: string,
  formatTime: (iso: string) => string,
): HTMLElement {
  const card = document.createElement("div");
  card.className = "card card-slash";

  const icon = document.createElement("span");
  icon.className = "slash-icon";
  icon.textContent = "⌘";
  card.appendChild(icon);

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
  ts.textContent = formatTime(timestamp);
  card.appendChild(ts);

  return card;
}

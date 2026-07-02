/**
 * 解析 user 消息里的斜杠命令标记。
 *
 * Claude Code 把 `/compact`、`/loop` 这类命令写到 JSONL 时，content 由三个
 * 标签组成——但**顺序随 CLI 版本漂移**（Batch4-F16 实测本机数据）：
 *
 *   旧版：<command-name>/compact</command-name><command-message>…</command-message><command-args></command-args>
 *   新版：<command-message>full-audit</command-message>
 *         <command-name>/full-audit</command-name>
 *         <command-args>全面理解这个项目</command-args>
 *
 * 旧实现的正则钉死旧序，新版会话的 `/xxx` 整段 XML 裸露在 user 气泡里。
 * 现在三标签各自独立提取（name 必须、message/args 可选），剥除后剩余非空白
 * → 判非命令走兜底（防误伤恰好含这些标签的正文）。
 *
 * 注：`/clear`、`/help`、`/model` 等 CLI-only 命令不会写 JSONL，物理上识别不到。
 */

// 显式 .ts 扩展：slash.ts 被 bash.test.ts（node 直跑）间接 import，
// node type-stripping 不做扩展名推断（tsconfig allowImportingTsExtensions 已开）。
import { unescapeEntities } from "./bash.ts";

export interface SlashCommand {
  name: string;
  args: string;
}

const NAME_RE = /<command-name>([\s\S]*?)<\/command-name>/;
const MSG_RE = /<command-message>[\s\S]*?<\/command-message>/;
const ARGS_RE = /<command-args>([\s\S]*?)<\/command-args>/;

export function parseSlashCommand(text: string): SlashCommand | null {
  const name = NAME_RE.exec(text);
  if (!name) return null;
  const args = ARGS_RE.exec(text);
  // 三类标签各剥一次（非全局——出现第二份会留残余），剩余必须是纯空白
  const leftover = text.replace(NAME_RE, "").replace(MSG_RE, "").replace(ARGS_RE, "").trim();
  if (leftover.length > 0) return null;
  return {
    name: unescapeEntities(name[1]).trim(),
    args: unescapeEntities(args?.[1] ?? "").trim(),
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

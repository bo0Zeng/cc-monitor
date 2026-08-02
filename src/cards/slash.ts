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
 * 注（U-CC1 订正，2026-08-02 全量语料实测）：原文写的是「`/clear`、`/help`、`/model` 等
 * CLI-only 命令不会写 JSONL，物理上识别不到」——**`/model` 那个例子是错的**。
 * 本机 1,904 个 jsonl 里 `<command-name>` 共 **56 种**，其中 `/model` **74 条**、`/context` 11、
 * `/login` 4、`/doctor` 3、`/ide` 3、`/exit` 3 都在。真正 0 条的只有 `/clear` 与 `/help`。
 *
 * 所以这一层**不需要白名单**：本渲染器已经完全数据驱动（任何 `/xxx` 都渲染成 ⌘ 卡，
 * 三标签独立提取、顺序无关），56 种命令名零改动跑通。**加白名单是负收益。**
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
  const cmdName = unescapeEntities(name[1]).trim();
  if (!cmdName) return null; // 空 name → 回退原样（与 bash.ts 空命令回退对称）
  return {
    name: cmdName,
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

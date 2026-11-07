/**
 * **把后端给的会话事实落到 tab 上** —— 纯函数，零 DOM、零 IPC。
 *
 * 分叉血缘 · 改动文件集 · 最新 usage · 项目目录 · 「机器 · 目录」那一句：判定与累加都在后端
 * （`observe/recognize/facts.rs`，帧命令 `history-facts`），界面不从 JSONL 记录里抽事实，本文件只做**投影**：成品里的数组落成 tab 上的
 * `Map` / `Set`、`usage` 拆成两格，并回「这次哪几样变了」—— 变了之后刷哪块界面是调用方（`TabManager`）的事。
 *
 * 子 agent 的列表与状态不在这里：那是运行表（后端 `session_runs`，`runs.ts`），界面不自己判。
 */
import type { SessionFacts } from "./session-reads";
import type { Tab } from "./tab-model";
import { lineageOf, sameLineage } from "./session-hidden";

/** 一份成品落下来之后，哪几样真变了（宿主据此只刷变了的那几块）。 */
export interface FactsChange {
  /** 「分叉自」那一句变了。 */
  forkedLine: boolean;
  touchedFiles: boolean;
  usage: boolean;
  /** 项目目录或它的显示形（`cwdShown`）变了。 */
  projectDir: boolean;
  /** 「机器 · 目录」那一句变了。 */
  where: boolean;
  /** 需手动那一格变了。 */
  needs: boolean;
  /** 悬停卡与状态一句那几样（没结果的调用 · 最后一句 · 状态一句）变了。 */
  peek: boolean;
  /** 血缘（由谁启动 · 子会话）变了。 */
  lineage: boolean;
  /** 会话头上那两颗（模型 · 权限档）的字或那一档变了。 */
  switchable: boolean;
}

/** 后端的一份成品 ⇒ tab 上的几样（整份替换，不合并；项目目录后端还没读到 ⇒ 不动 tab 上那一份）。 */
export function applyFacts(tab: Tab, f: SessionFacts): FactsChange {
  const forkedLine = f.forkedLine ?? null;
  const forkedChanged = tab.forkedLine !== forkedLine;
  tab.forkedLine = forkedLine;

  const files = [...tab.touchedFiles];
  const touchedFiles = files.length !== f.touchedFiles.length || files.some((p, i) => p !== f.touchedFiles[i]);
  tab.touchedFiles = new Set(f.touchedFiles);

  const usage = JSON.stringify(tab.usage) !== JSON.stringify(f.usage);
  tab.usage = f.usage;

  const dirMoved = f.projectDir !== null && f.projectDir !== tab.projectDir;
  if (dirMoved) tab.projectDir = f.projectDir;
  const shownMoved = f.cwdShown !== undefined && f.cwdShown !== tab.cwdShown;
  if (shownMoved) tab.cwdShown = f.cwdShown ?? null;
  const projectDir = dirMoved || shownMoved;

  if (f.agent !== null) tab.agent = f.agent;

  const whereLine = f.whereLine ?? null;
  const where = tab.whereLine !== whereLine;
  tab.whereLine = whereLine;

  const needs =
    JSON.stringify(tab.needs) !== JSON.stringify(f.needs) ||
    tab.autoAnswer !== f.autoAnswer ||
    JSON.stringify(tab.autoAnswerChoices) !== JSON.stringify(f.autoAnswerChoices) ||
    JSON.stringify(tab.askEnd) !== JSON.stringify(f.askEnd);
  tab.needs = f.needs;
  tab.autoAnswer = f.autoAnswer;
  tab.autoAnswerChoices = f.autoAnswerChoices;
  tab.askEnd = f.askEnd;
  const peek = JSON.stringify([tab.pending, tab.lastSay, tab.backgroundWork, tab.line]) !== JSON.stringify([f.pending, f.lastSay, f.background, f.line ?? null]);
  tab.line = f.line ?? null;
  tab.backgroundWork = f.background;
  tab.pending = f.pending;
  tab.lastSay = f.lastSay;
  tab.retries = new Map(f.retries.map((r) => [r.id, r]));

  const lin = lineageOf(f);
  const lineage = tab.lineage === undefined || !sameLineage(tab.lineage, lin);
  tab.lineage = lin;

  const switchable =
    tab.modelText !== f.modelText || tab.modelPick !== f.modelPick || tab.permissionMode !== f.permissionMode || tab.permissionModeText !== f.permissionModeText;
  tab.modelText = f.modelText;
  tab.modelPick = f.modelPick;
  tab.permissionMode = f.permissionMode;
  tab.permissionModeText = f.permissionModeText;

  return { forkedLine: forkedChanged, touchedFiles, usage, projectDir, where, needs, peek, lineage, switchable };
}

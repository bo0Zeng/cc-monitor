/**
 * **把后端给的会话事实落到 tab 上** —— 纯函数，零 DOM、零 IPC。
 *
 * 分叉血缘 · 改动文件集 · 最新 usage：本文件从前是「从一条 JSONL 记录里抽事实」的几个抽取器，
 * 挂在 `onLine` 旁路上一条一条攒（表里标「读 json」的那四行）。今天判定与累加都在后端
 * （`observe/facts_query.rs`，帧命令 `history-facts`），本文件只剩**投影**：成品里的数组落成 tab 上的
 * `Map` / `Set`、`usage` 拆成两格，并回「这次哪几样变了」—— 变了之后刷哪块界面是调用方（`TabManager`）的事。
 *
 * 子 agent 的列表与状态不在这里：那是运行表（后端 `session_runs`，`runs.ts`），界面不自己判。
 */
import type { SessionFacts } from "./session-reads";
import type { Tab } from "./tab-model";

/** 一份成品落下来之后，哪几样真变了（宿主据此只刷变了的那几块）。 */
export interface FactsChange {
  forkedFrom: boolean;
  touchedFiles: boolean;
  usage: boolean;
}

/** 后端的一份成品 ⇒ tab 上的三样（整份替换，不合并）。 */
export function applyFacts(tab: Tab, f: SessionFacts): FactsChange {
  const forkedFrom = tab.forkedFromSessionId !== f.forkedFrom;
  tab.forkedFromSessionId = f.forkedFrom;

  const files = [...tab.touchedFiles];
  const touchedFiles = files.length !== f.touchedFiles.length || files.some((p, i) => p !== f.touchedFiles[i]);
  tab.touchedFiles = new Set(f.touchedFiles);

  const tokens = f.usage?.promptTokens ?? null;
  const model = f.usage?.model ?? null;
  const usage = tab.latestPromptTokens !== tokens || tab.latestModel !== model;
  tab.latestPromptTokens = tokens;
  tab.latestModel = model;

  return { forkedFrom, touchedFiles, usage };
}

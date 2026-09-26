/**
 * 〔STC · `设计/90 §4` 阶段 C · `设计/10 §2.2`〕**把后端给的会话事实落到 tab 上** —— 纯函数，零 DOM、零 IPC。
 *
 * 分叉血缘 · 改动文件集 · agent 列表 · 最新 usage：本文件从前是「从一条 JSONL 记录里抽事实」的四个抽取器，
 * 挂在 `onLine` 旁路上一条一条攒（`10 §2.2` 表里标「读 json」的那四行）。今天判定与累加都在后端
 * （`observe/facts_query.rs`，帧命令 `history-facts`），本文件只剩**投影**：成品里的数组落成 tab 上的
 * `Map` / `Set`、`usage` 拆成两格，并回「这次哪几样变了」—— 变了之后刷哪块界面是调用方（`TabManager`）的事。
 *
 * 唯一不是投影的一件：**中止**（`abortRunningAgents`）。它不从 json 读出来 —— 它是「会话落到不忙
 * （idle / shell / 已结束）那一刻」这个**事件**的反应（`10 §2.2`「刚刚发生了什么留在流上」）：
 * 那一刻还在跑的 agent 不会再有结果（ESC 打断 / 崩溃不写 `tool_result`）。被判中止的 id 记在 `tab.agentsAborted`，
 * 之后的成品里它仍是 `running` 也照样显示中止。
 */
import type { AgentEntry } from "./agents-panel";
import type { SessionFacts } from "./session-reads";
import type { Tab } from "./tab-model";

/** 一份成品落下来之后，哪几样真变了（宿主据此只刷变了的那几块）。 */
export interface FactsChange {
  forkedFrom: boolean;
  agents: boolean;
  touchedFiles: boolean;
  usage: boolean;
}

const sameAgent = (a: AgentEntry, b: AgentEntry): boolean =>
  a.id === b.id &&
  a.label === b.label &&
  a.agentType === b.agentType &&
  a.status === b.status &&
  a.timestamp === b.timestamp &&
  a.desc === b.desc;

/** 后端的一份成品 ⇒ tab 上的四样（整份替换，不合并）。 */
export function applyFacts(tab: Tab, f: SessionFacts): FactsChange {
  const forkedFrom = tab.forkedFromSessionId !== f.forkedFrom;
  tab.forkedFromSessionId = f.forkedFrom;

  const agents = new Map<string, AgentEntry>();
  for (const a of f.agents) {
    agents.set(a.id, {
      id: a.id,
      label: a.label,
      agentType: a.agentType,
      status: a.status === "running" && tab.agentsAborted.has(a.id) ? "aborted" : a.status,
      timestamp: a.timestamp,
      desc: a.desc,
    });
  }
  const before = [...tab.agents.values()];
  const after = [...agents.values()];
  const agentsChanged = before.length !== after.length || before.some((a, i) => !sameAgent(a, after[i]));
  tab.agents = agents;

  const files = [...tab.touchedFiles];
  const touchedFiles = files.length !== f.touchedFiles.length || files.some((p, i) => p !== f.touchedFiles[i]);
  tab.touchedFiles = new Set(f.touchedFiles);

  const tokens = f.usage?.promptTokens ?? null;
  const model = f.usage?.model ?? null;
  const usage = tab.latestPromptTokens !== tokens || tab.latestModel !== model;
  tab.latestPromptTokens = tokens;
  tab.latestModel = model;

  return { forkedFrom, agents: agentsChanged, touchedFiles, usage };
}

/** 会话落到不忙（idle / shell / 已结束）⇒ 仍在跑的 agent 标中止，并记住是哪几个（之后的成品照样显示中止）。 */
export function abortRunningAgents(tab: Tab): boolean {
  let changed = false;
  for (const a of tab.agents.values()) {
    if (a.status === "running") {
      a.status = "aborted";
      tab.agentsAborted.add(a.id);
      changed = true;
    }
  }
  return changed;
}

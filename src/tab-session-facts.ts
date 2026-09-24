/**
 * 〔U2 · 拆 `tabs.ts` ①〕**从一条 JSONL 记录里抽会话事实，记到 tab 上** —— 纯函数，零 DOM、零 IPC。
 *
 * usage（HUD 的 context%）· subagent 配对（agents 面板）· fork 血缘 · 改动过的文件（全景高亮）。
 * 每个函数只改 `tab` 的字段并返回「这次变了没有」；变了之后刷哪块界面、喂哪个回调，
 * 是调用方（`TabManager`）的事 —— 那一半与「现在是不是重放批 / 是不是当前 tab」有关，不归这里。
 * 函数体逐字从 `tabs.ts` 搬来，唯一的改写是把末尾那一句「通知界面」换成 `return` 一个布尔。
 */
import { isAgentTool } from "./cards/subagent";
import { collectEditedFiles } from "./panorama/session-files";
import type { Tab } from "./tab-model";

/**
 * F88b：从 assistant 记录抽 usage → 更新 tab.latestPromptTokens/latestModel（供 HUD context%）。
 * prompt token = input + cache_creation + cache_read（本轮喂进模型的总量，即 context 占用近似）。
 * 只认带 usage 的 assistant 记录（user/system/无 usage 的一律跳过，保留上一次值）。
 *
 * 审计加固：
 * - **seq 单调**：onLine 投递序不保证升序（重放/远端重投），故只在 `seq >= tab.latestUsageSeq`
 *   时覆盖——「最新」= 最大 seq 而非最后到达，防低 seq 历史记录盖掉高 seq 实时值（会误显低占用%）。
 * - **批期不刷 chip**：重放/大增量批（inBatch）里逐条 assistant 记录都触发 setActive 会视觉抖动
 *   （10%→20%→…），故批期只更 tab 字段、不喂回调；onBatchEnd 对活跃 tab 单次 flush。
 *   ⇒ 本函数只改 tab 字段、返回「这次更新了没有」；喂不喂回调是调用方（`TabManager.trackUsage`）的事。
 */
export function noteUsage(tab: Tab, message: unknown, seq: number): boolean {
  const rec = message as {
    type?: string;
    message?: {
      model?: unknown;
      usage?: {
        input_tokens?: unknown;
        cache_creation_input_tokens?: unknown;
        cache_read_input_tokens?: unknown;
      };
    };
  };
  if (rec?.type !== "assistant") return false;
  const usage = rec.message?.usage;
  if (!usage || typeof usage !== "object") return false;
  const num = (v: unknown): number => (typeof v === "number" && v >= 0 ? v : 0);
  const prompt =
    num(usage.input_tokens) +
    num(usage.cache_creation_input_tokens) +
    num(usage.cache_read_input_tokens);
  // 全 0（无任何 token 字段）→ 视为无效 usage，不覆盖上一次有效值。
  if (prompt <= 0) return false;
  // seq 回退（更老的记录晚到）→ 不覆盖更新的值。
  if (seq < tab.latestUsageSeq) return false;
  tab.latestUsageSeq = seq;
  tab.latestPromptTokens = prompt;
  tab.latestModel = typeof rec.message?.model === "string" ? rec.message.model : null;
  return true;
}

/**
 * issue #23（第二增量）：从 jsonl 流配对 agent 工具调用。
 * - assistant 的 Task/Agent tool_use → 注册 running（label 取 input.description，
 *   回退 prompt 首行 / 工具名）
 * - user 的 tool_result（按 tool_use_id 命中）→ done
 * 防 spam：只在真有变化时刷新面板。结构防御：message 形态全 unknown 窄化，
 * 任何不匹配静默跳过（§18 同源精神）。
 */
export function noteAgents(tab: Tab, message: unknown): boolean {
  const rec = message as {
    type?: string;
    timestamp?: unknown;
    message?: { content?: unknown };
  };
  const content = rec?.message?.content;
  if (!Array.isArray(content)) return false;
  // F77：这条 assistant 记录的 timestamp——存进 AgentEntry 供「点进 agent 看记录」的 load_subagent 定位。
  const recTimestamp = typeof rec.timestamp === "string" ? rec.timestamp : "";
  let changed = false;
  if (rec.type === "assistant") {
    for (const b of content) {
      const blk = b as {
        type?: string;
        id?: string;
        name?: string;
        input?: { description?: unknown; prompt?: unknown; subagent_type?: unknown };
      };
      if (
        blk?.type !== "tool_use" ||
        typeof blk.id !== "string" ||
        typeof blk.name !== "string" ||
        !isAgentTool(blk.name)
      ) {
        continue;
      }
      // F77：desc **trim 后**（镜像卡片 `input.description?.trim()`），供 load_subagent 精确匹配。
      const desc = (
        typeof blk.input?.description === "string" ? blk.input.description : ""
      ).trim();
      const prompt =
        typeof blk.input?.prompt === "string" ? blk.input.prompt : "";
      const label =
        desc || prompt.split("\n")[0]?.slice(0, 80) || blk.name;
      const agentType =
        typeof blk.input?.subagent_type === "string"
          ? blk.input.subagent_type
          : null;
      tab.agents.set(blk.id, {
        id: blk.id,
        label,
        agentType,
        status: "running",
        timestamp: recTimestamp, // F77：供 load_subagent 定位子 agent
        desc, // F77：load_subagent 精确匹配的 description（trim 后，非展示 label）
      });
      changed = true;
    }
    // 上限 30：超出删最老的非 running（Map 保持插入序）
    if (tab.agents.size > 30) {
      for (const [id, a] of tab.agents) {
        if (tab.agents.size <= 30) break;
        if (a.status !== "running") tab.agents.delete(id);
      }
    }
  } else if (rec.type === "user") {
    for (const b of content) {
      const blk = b as { type?: string; tool_use_id?: string };
      if (blk?.type !== "tool_result" || typeof blk.tool_use_id !== "string") {
        continue;
      }
      const a = tab.agents.get(blk.tool_use_id);
      if (a && a.status === "running") {
        a.status = "done";
        changed = true;
      }
    }
  }
  return changed;
}

/** issue #23：会话不再 busy（idle/shell/归档）→ 仍 running 的 agent 标 aborted
 *（ESC 打断/崩溃不会有 tool_result，防僵尸"运行中"）。 */
export function abortRunningAgents(tab: Tab): boolean {
  let changed = false;
  for (const a of tab.agents.values()) {
    if (a.status === "running") {
      a.status = "aborted";
      changed = true;
    }
  }
  return changed;
}

/**
* issue #63①：从记录里取 `forkedFrom.sessionId`，锁定血缘并给标题加 `↳` 徽标（同 aiTitle:出现一次
* 就锁,后续记录/重投不覆盖）。fork 会话的首条记录带 `forkedFrom`（Claude 原生 `/branch` 格式,
* 后端 history.rs 也读它）——但活 tab 层此前完全不看它,fork 与原会话是同名独立 tab、分不清。
*/
export function noteForkedFrom(tab: Tab, message: unknown): boolean {
  if (tab.forkedFromSessionId) return false; // 已锁定
  const fk = (message as { forkedFrom?: { sessionId?: unknown } }).forkedFrom;
  const sid = fk?.sessionId;
  if (typeof sid !== "string" || sid.length === 0) return false;
  tab.forkedFromSessionId = sid;
  return true;
}

/**
 * F70：累进本会话改动集（写类工具 file_path）。
 * F91b-fix(batch18 审计修)：re-touch 时 delete+add 把它移到末尾 = **近因序**，让 F91b peek 的
 * slice(-8) 显「最近改的 8 个」（原 Set 只记首触序，此刻正猛改的老文件被埋）。Set 成员/size 不变，
 * F70 全景高亮按成员判定、与序无关，安全。O(1)/文件。
 */
export function noteTouchedFiles(tab: Tab, message: unknown): void {
  for (const f of collectEditedFiles(message)) {
    tab.touchedFiles.delete(f);
    tab.touchedFiles.add(f);
  }
}

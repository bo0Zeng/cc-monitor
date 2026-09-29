/**
 * Agent/Task tool_use 的折叠卡。
 *
 * 主 session 看到这种 tool_use 时不渲染普通 `block-tool-use`，而是渲染一个
 * 折叠条；用户展开 → invoke `load_subagent` 异步拿到对应 `<session>/subagents/
 * agent-<id>.jsonl` 的全部记录 → 内嵌按主渲染逻辑展示。
 *
 * 解耦：通过回调拿主渲染函数（避免与 cards/index.ts 循环耦合的运行时风险），
 * 不直接调 Rust 命令以外的全局状态。
 */

import type { JsonlRecord, RenderContext, RenderResult } from "./index";

/** Rust subagent::load_subagent 的返回结构 */
// C04d 批 2：改用生成物。**它的 `records: Vec<JsonlRecord>` 传递依赖是 C04c 生成的**
// ——那一轮把 `JsonlRecord` 变成生成物的投资，在这里第一次收息（否则这一批还得先啃它）。
import { loadSubagent, type SubagentLoadResult } from "../record-reads";
import type { Origin } from "../ipc/origin";
import { copyText } from "../copy-table";

/** Agent tool_use 块的 input 形状（部分字段，按实测保留） */
interface AgentInput {
  description?: string;
  subagent_type?: string;
  prompt?: string;
}

// 〔THIN〕`isAgentTool`〔散文墓碑〕删：哪个 tool_use 是子 agent 由那台后端判（卡型 `agent`，随 assistant 记录的 `toolCards` 带来）。

/**
 * 构造 Agent 折叠卡。
 * @param input        block.input
 * @param timestamp    父消息 timestamp，作为 description 同名时的 tiebreaker
 * @param ctx          含 parentPath（父 JSONL 路径）
 * @param renderChild  主渲染入口，渲染 subagent JSONL 内的每一条记录
 */
export function buildAgentCard(
  input: AgentInput,
  timestamp: string,
  ctx: RenderContext,
  renderChild: (rec: JsonlRecord, ctx: RenderContext) => RenderResult,
): HTMLElement {
  const desc = input.description?.trim() ?? "";
  const subtype = input.subagent_type ?? "Agent";

  const d = document.createElement("details");
  d.className = "block-collapsible block-agent";

  const s = document.createElement("summary");
  s.className = "block-summary";
  s.textContent = `${subtype}  ·  ${desc || "(no description)"}`; // F80：去纯装饰 🤖
  d.appendChild(s);

  let loaded = false;
  let loading = false;
  let bodyEl: HTMLElement | null = null;

  d.addEventListener("toggle", () => {
    if (!d.open || loaded || loading) return;
    void loadAndRender();
  });

  async function loadAndRender(): Promise<void> {
    if (!bodyEl) {
      bodyEl = document.createElement("div");
      bodyEl.className = "block-body block-agent-body";
      d.appendChild(bodyEl);
    }
    bodyEl.replaceChildren();
    // ★ P7c-1（08-12）：**远端会话现在也能展开了**。
    //
    // 这里原来是一条降级：「远端会话（[origin]）暂不支持展开 subagent——其记录在远端机器上」，
    // 尾注还写着「真·远端拉取留 backlog（backend `--read-subagent` 协议扩容）」——那件事做了，
    // 〔MOD〕今天「找 ＋ 挑 ＋ 读 ＋ 解析」都在那台后端（`history-subagent`，挑的规则只一份 `history_query::pick_subagent`），
    //（定框 `C1`：别长第二套语义）。⇒ 这里只需把 origin 传下去。
    loading = true;
    bodyEl.textContent = copyText("subagent.loadAndRender.loading");

    try {
      // 〔MOD〕经通道直接问那台后端 `history-subagent`（列 ＋ 挑 ＋ 读 ＋ 解析都在后端，`src/frontend/ui/record-reads.ts`）；
      //   本机是 `LOCAL_ORIGIN`（`"<local>"`），与远端同一条路。
      const result: SubagentLoadResult = await loadSubagent(ctx.origin, ctx.parentPath, desc, timestamp);
      bodyEl.replaceChildren();
      renderSubagentBody(bodyEl, result, renderChild, ctx.origin);
      loaded = true;
    } catch (e) {
      bodyEl.replaceChildren();
      const errMsg = document.createElement("div");
      errMsg.className = "block-agent-error";
      errMsg.textContent = copyText("subagent.loadAndRender.failed", { e: String(e) });
      bodyEl.appendChild(errMsg);
      const retry = document.createElement("button");
      retry.type = "button";
      retry.className = "block-agent-retry";
      retry.textContent = copyText("subagent.loadAndRender.retry");
      retry.addEventListener("click", (ev) => {
        ev.preventDefault();
        ev.stopPropagation();
        if (!loading) void loadAndRender();
      });
      bodyEl.appendChild(retry);
    } finally {
      loading = false;
    }
  }

  return d;
}

function renderSubagentBody(
  body: HTMLElement,
  result: SubagentLoadResult,
  renderChild: (rec: JsonlRecord, ctx: RenderContext) => RenderResult,
  origin: Origin,
): void {
  const header = document.createElement("div");
  header.className = "block-agent-header";
  header.textContent = copyText("subagent.renderSubagentBody.header", { agentId: result.agent_id.slice(0, 12), n: result.records.length });
  body.appendChild(header);

  // 嵌套渲染时把 ctx.parentPath 切到 subagent 自己的 JSONL 路径，
  // 这样如果 subagent 内部又有 Agent tool_use，能找到 *它自己* 的 subagents 目录
  // toolUseNames 用独立 Map，避免与父 session 的 id 冲突
  //
  // P5.2 B 重构后 TabManager/SessionViewer 用 renderStreamRecord（基于 timeline +
  // seq），但 subagent 内部还是用 array 顺序 + renderMessage 直接调（renderChild
  // 由 caller 传入 = cards/index.ts::renderMessage）。subagent 一次性 load 没有
  // 增量场景，timeline 抽象没收益；保留简单 for-loop。
  const nestedCtx: RenderContext = {
    parentPath: result.path,
    // 〔C4a〕子 agent 的文件与父会话在同一台机器上（上一版这里不填 ⇒ 被当成本机）。
    origin,
    toolUseNames: new Map(),
    toolUseElements: new Map(),
    // P4：pendingToolResults 必填。subagent 内部一次性渲染，无 reconcile 路径，
    // 但提供空 Map 防止 cards/index.ts 的 fallback 路径走 undefined 导致漏注入。
    pendingToolResults: new Map(),
  };

  for (const rec of result.records) {
    const r = renderChild(rec, nestedCtx);
    if (r.kind === "card") {
      body.appendChild(r.element);
    } else if (r.kind === "tool-group") {
      const wrap = document.createElement("div");
      wrap.className = "block-agent-tool-group";
      for (const u of r.units) wrap.appendChild(u);
      body.appendChild(wrap);
    }
  }
}

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

import { invoke } from "@tauri-apps/api/core";
import type { JsonlRecord, RenderContext, RenderResult } from "./index";

/** Rust subagent::load_subagent 的返回结构 */
interface SubagentLoadResult {
  path: string;
  agent_id: string;
  records: JsonlRecord[];
}

/** Agent tool_use 块的 input 形状（部分字段，按实测保留） */
interface AgentInput {
  description?: string;
  subagent_type?: string;
  prompt?: string;
}

/** 判断一个工具名是否触发 subagent（v2.1 是 "Agent"，老版本曾是 "Task"） */
export function isAgentTool(name: string): boolean {
  return name === "Agent" || name === "Task";
}

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
  s.textContent = `🤖 ${subtype}  ·  ${desc || "(no description)"}`;
  d.appendChild(s);

  let loaded = false;
  let loading = false;
  d.addEventListener("toggle", () => {
    if (!d.open || loaded || loading) return;
    void loadAndRender();
  });

  async function loadAndRender(): Promise<void> {
    loading = true;
    const body = document.createElement("div");
    body.className = "block-body block-agent-body";
    body.textContent = "加载 subagent…";
    d.appendChild(body);

    try {
      const result = await invoke<SubagentLoadResult>("load_subagent", {
        parentJsonlPath: ctx.parentPath,
        description: desc,
        toolUseTimestamp: timestamp,
      });
      body.replaceChildren();
      renderSubagentBody(body, result, renderChild);
      loaded = true;
    } catch (e) {
      body.textContent = `加载失败：${String(e)}`;
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
): void {
  const header = document.createElement("div");
  header.className = "block-agent-header";
  header.textContent = `agent-${result.agent_id.slice(0, 12)}  ·  ${result.records.length} 条记录`;
  body.appendChild(header);

  // 嵌套渲染时把 ctx.parentPath 切到 subagent 自己的 JSONL 路径，
  // 这样如果 subagent 内部又有 Agent tool_use，能找到 *它自己* 的 subagents 目录
  const nestedCtx: RenderContext = { parentPath: result.path };

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

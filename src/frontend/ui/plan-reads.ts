/**
 * **计划读面**（pb 工作区 · 片 · 一格）的界面口：`plan-list` · `plan-read` · `plan-cell-view` 与代敲 · 认可 · 送话那几条（计划住哪台就问哪台）。
 *
 * 判定全在后端（`src/backend/plan/`）：状态、原因、接手对到哪个会话、边与文件倒过来的索引都是成品。
 * 入参 · 回包按生成类型问（`callTyped`，形状由握手指纹管）；没成原样抛（那一句 ＋ 详情是出错那一端写的）。
 */
import { budgetWithin, callTyped } from "./ipc/chan-caller";
import { ControlError } from "./control-said";
import type { Origin } from "./ipc/origin";
import type { PlanAckReply } from "./generated/PlanAckReply";
import type { PlanArchived } from "./generated/PlanArchived";
import type { PlanReturnReply } from "./generated/PlanReturnReply";
import type { PlanBlock } from "./generated/PlanBlock";
import type { PlanCell } from "./generated/PlanCell";
import type { PlanCheck } from "./generated/PlanCheck";
import type { PlanCmdReply } from "./generated/PlanCmdReply";
import type { PlanEdgeWords } from "./generated/PlanEdgeWords";
import type { PlanEdges } from "./generated/PlanEdges";
import type { PlanFile } from "./generated/PlanFile";
import type { PlanKind } from "./generated/PlanKind";
import type { PlanList } from "./generated/PlanList";
import type { PlanListSlice } from "./generated/PlanListSlice";
import type { PlanListWorkspace } from "./generated/PlanListWorkspace";
import type { PlanNeed } from "./generated/PlanNeed";
import type { PlanPb } from "./generated/PlanPb";
import type { PlanPhase } from "./generated/PlanPhase";
import type { PlanProgress } from "./generated/PlanProgress";
import type { PlanRead } from "./generated/PlanRead";
import type { PlanRed } from "./generated/PlanRed";
import type { PlanRef } from "./generated/PlanRef";
import type { PlanRefs } from "./generated/PlanRefs";
import type { PlanReturned } from "./generated/PlanReturned";
import type { PlanReturnedChild } from "./generated/PlanReturnedChild";
import type { PlanSessionBlock } from "./generated/PlanSessionBlock";
import type { PlanSign } from "./generated/PlanSign";
import type { PlanSlice } from "./generated/PlanSlice";
import type { PlanStale } from "./generated/PlanStale";
import type { PlanUndecidable } from "./generated/PlanUndecidable";
import type { PlanWho } from "./generated/PlanWho";
import type { PlanWhyCode } from "./generated/PlanWhyCode";

/** 起一次 pb（研究盘 252 格 1.1 秒；后端给 pb 的期限 20 秒）＋ 回程；远端再加一段。 */
const READ_BUDGET_MS = 25_000;

// 线上形状由后端 `plan/wire.rs` 经 ts-rs 生成（不手写），原样交给画的那几处。
export type {
  PlanAckReply,
  PlanReturnReply,
  PlanArchived,
  PlanBlock,
  PlanCell,
  PlanCheck,
  PlanCmdReply,
  PlanEdgeWords,
  PlanEdges,
  PlanFile,
  PlanKind,
  PlanList,
  PlanListSlice,
  PlanListWorkspace,
  PlanNeed,
  PlanPb,
  PlanPhase,
  PlanProgress,
  PlanRead,
  PlanRed,
  PlanRef,
  PlanRefs,
  PlanReturned,
  PlanReturnedChild,
  PlanSessionBlock,
  PlanSign,
  PlanSlice,
  PlanStale,
  PlanUndecidable,
  PlanWho,
  PlanWhyCode,
};

/** 四种边的名字（`PlanEdges` 的键，界面按这个次序排）。 */
export type PlanEdgeKind = keyof PlanEdges;
export const PLAN_EDGE_KINDS: readonly PlanEdgeKind[] = ["to", "with", "after", "replaces"];

/** 这台的工作区与片（`fresh` ＝ 认过的目录也重问 pb）。 */
export async function fetchPlanList(origin: Origin, fresh: boolean, dirs: readonly string[] = []): Promise<PlanList> {
  const req = dirs.length > 0 ? { fresh, dirs: [...dirs] } : { fresh };
  return callTyped(origin, "plan-list", req, budgetWithin(READ_BUDGET_MS));
}

/** 一个工作区的成品。 */
export async function fetchPlanRead(origin: Origin, workspace: string): Promise<PlanRead> {
  return callTyped(origin, "plan-read", { workspace }, budgetWithin(READ_BUDGET_MS));
}

/** 一格的 agent 视角（pb 原样那一段）。 */
export async function fetchCellView(origin: Origin, workspace: string, slice: string, id: string): Promise<string> {
  return (await callTyped(origin, "plan-cell-view", { workspace, slice, id }, budgetWithin(READ_BUDGET_MS))).view;
}

/** 以人的身份代敲的几条（`take` 另走 [`takeRepo`]：要带片名）。 */
export type PlanCmd = "continue" | "pause" | "view";

/** 代敲一条用户命令（pb 拒 ⇒ 后端回拒绝，那一句是 pb 那一句）。 */
export async function planCommand(origin: Origin, workspace: string, cmd: PlanCmd): Promise<PlanCmdReply> {
  return callTyped(origin, "plan-command", { workspace, cmd }, budgetWithin(READ_BUDGET_MS));
}

/** 接过一片的仓库（pb 的 `take`；不是此刻那一片后端先切过去）。回包的 `took` 说接到了哪一片；没说 ⇒ 不当成了。 */
export async function takeRepo(origin: Origin, workspace: string, slice: string): Promise<PlanCmdReply & { took: NonNullable<PlanCmdReply["took"]> }> {
  const r = await callTyped(origin, "plan-command", { workspace, cmd: "take", slice }, budgetWithin(READ_BUDGET_MS));
  if (r.took === null) throw new ControlError(r.said ?? "");
  return { ...r, took: r.took };
}

/** 认可一条 / 撤掉认可（只记在 cc-monitor，不写计划仓）。 */
export async function ackNeed(origin: Origin, workspace: string, slice: string, key: string, on: boolean): Promise<PlanAckReply> {
  const req = { workspace, slice, key };
  const budget = budgetWithin(READ_BUDGET_MS);
  return on ? callTyped(origin, "plan-ack", req, budget) : callTyped(origin, "plan-unack", req, budget);
}

/** 把人的话送给负责那一格的会话（后端拼那一行、判能不能送）。 */
export async function returnCell(
  origin: Origin,
  a: { workspace: string; slice: string; id: string; text: string; to: "owner" | "signer"; resume?: boolean },
): Promise<PlanReturnReply> {
  return callTyped(origin, "plan-return", a, budgetWithin(READ_BUDGET_MS));
}

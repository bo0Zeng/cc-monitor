/**
 * **接回那一行问那台后端**（`launch-render-cli`，本机远端同一条 `chan.call(origin, …)`）—— 回的永远是一行 `ccm …`；渲不出来是拒。
 *
 * 环境、中转地址、身份标记由那台机器上的 `ccm` 自己做。本文件只做调用方那一侧：发请求 · 失败说成一句（`control-said.ts::settleTyped`）。
 */
import { budgetWithin, callTyped } from "./ipc/chan-caller";
import { machineName, settleTyped, type Refusals } from "./control-said";
import type { Origin } from "./ipc/origin";
import type { CliRenderRequest, CliRendered } from "./launch-cli-wire";
import { copyText } from "./copy-table";

/** 渲染是纯函数（本机那条多问一次目录在不在）；这个期限只挡「那台后端没在答」。 */
const RENDER_BUDGET_MS = 10_000;

/** 那台后端拒了（码 `refused`）：说的就是那一句，前面带上是哪台。 */
function refusals(origin: Origin): Refusals {
  return {
    byCode: (_code, detail) => copyText("launchRender.refused.onMachine", { machine: machineName(origin), said: detail }),
    noReason: () => copyText("remoteLaunchRun.renderCli.noReason"),
  };
}

/** 那一行 `ccm …` ＋ 那台判出来实际用的号（typed 帧命令：回包类型从 Rust 生成，形状由握手指纹核过）。通道上的失败与那台后端的拒都抛
 *  （[`ControlError`]）。 */
export async function renderCli(origin: Origin, req: CliRenderRequest): Promise<CliRendered> {
  const budget = budgetWithin(RENDER_BUDGET_MS);
  return settleTyped(origin, "launch-render-cli", callTyped(origin, "launch-render-cli", req, budget), refusals(origin));
}

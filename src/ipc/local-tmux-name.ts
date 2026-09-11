/**
 * `K-R46`：**本机起会话时那个 tmux 会话名，从哪来** —— 一个函数，三条主路共用。
 *
 * # 为什么它必须单独存在
 *
 * 后端**故意**拒绝自己铸名（`history.rs` 的 `NO_TMUX_NAME`，注释逐字：「在这里补一个
 * 铸造口 = 第三次犯同一个错」）⇒ 名字只能由前端算好传下去。而「怎么算」这件事里
 * **有两格是纪律、不是代码**：
 *
 * 1. **名字只许过 `mintTmuxName`**（`remote-launch.ts`，全仓唯一带撞名避让的铸造口）。
 *    自己拼一个 `<sid8>-cc` 就是 F13 修掉的那个坑：另一处精心让出 `-cc-2`，你直接撞上去。
 *    这里用现成的 `pickFreshTmuxName`（= 基名 `<sid8>-cc` + `mintTmuxName` 的避让），
 *    **不在本文件里重写基名规则**。
 * 2. **`list_local_tmux()` 回 `null` 是「不知道」，不是「一个都没占」。**
 *    不知道的时候**不铸名、回 `null`** ⇒ 后端诚实降级回旧路（不进容器）。
 *    硬铸就是「不避让」，那正是 issue #76「静默接进第一个会话，而用户以为开了新的」。
 *
 * 这两格 `K-R46` 之前只写在 `tabs.ts` 那一处的注释里，而另外两条主路
 * （`views/history.ts` 的历史页 resume · `fork-flow.ts` 的分叉本机起）**一个字都没传**
 * —— 那就是本件治的病：架构上一条路、行为上两条。
 *
 * ⚠ **今天仍有一份内联副本**：`src/tabs.ts` 那条 tab 栏 resume 自己写着同样六行
 * （本件写区不含 `src/tabs.ts`，收不进来）。⇒ 现在是 **2 处调本函数 + 1 处内联**，
 * 收掉最后那一处要把 `src/tabs.ts` 划进写区，已随本件上报。
 *
 * ⚠ **它买不到「会话真的进了容器」**：POSIX 那侧后端还要过账号那一格
 * （`render_local_ccm_with`：只有显式「账号 0」渲染得出容器，具名账号与「没表态」
 * 都 §35 降级），Windows 那侧则连读都不读这个名字（`C12`）。本函数只负责
 * **把名字说出来**，别把它读成「容器一定建起来了」。
 */

import { commands } from "./commands";
import { pickFreshTmuxName } from "../remote-launch";

/**
 * 给本机这条会话铸一个**不撞现有 tmux 名**的会话名。
 *
 * @returns 铸出来的名字；**`null` = 说不出**（本机 tmux 快照不知道 / 读口抛了）
 *          ⇒ 调用方照原样传 `null`，后端降级回旧路。
 */
export async function mintLocalTmuxName(sessionId: string): Promise<string | null> {
  try {
    const sessions = await commands.list_local_tmux();
    // `null` = 本机 daemon 通道没起 / 还没推过帧 = **不知道**。绝不退化成空集。
    if (!sessions) return null;
    return pickFreshTmuxName(sessionId, new Set(sessions.map((s) => s.name)));
  } catch {
    // 读不到就当不知道 —— 与上面同一条纪律。
    return null;
  }
}

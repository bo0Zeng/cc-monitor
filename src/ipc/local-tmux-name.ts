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
 *    自己拼一个 `<项目名>-cc` 就是 F13 修掉的那个坑：另一处精心让出 `-cc-2`，你直接撞上去。
 *    这里用现成的 `mintSessionTmuxName`（= 基名 `<项目名>-cc` + `mintTmuxName` 的避让），
 *    **不在本文件里重写基名规则**。
 *    🔴 `K-R96`（用户 09-12 `R55`「**要是可读的名字 / 不要id**」）：基名从 `<sid8>-cc`
 *    换成从 **cwd** 派生的 `<项目名>-cc` ⇒ 本函数**多收一个 `cwd`**。
 *    sid 一格没丢 —— 它骑在 `@ccm_sid` 上（后端建会话时写），认会话从来只问那个。
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
import { mintSessionTmuxName } from "../remote-launch";

/**
 * 给本机这条会话铸一个**不撞现有 tmux 名**的会话名。
 *
 * 🔴 〔`K-R96` 09-12〕**`sessionId` 这个参数没了** —— 名字里不再有 sid，它没有用武之地。
 * 留一个用不上的参数就是下一处「看起来有关系、实际没有」的误导；sid 仍然由调用方
 * 直接传给 `resume_history_session`（后端拿它去 `set-option @ccm_sid`，那才是它的载体）。
 *
 * @param cwd 这条会话的工作目录 —— 名字就是从它派生的（`<项目名>-cc`）。
 *            🔴 **它不是可选的**：给空串会一律铸出 `session-cc`，而那正是
 *            「有名字的样子、没有可读性的事实」。少传一个参数由 `tsc` 挡着。
 * @returns 铸出来的名字；**`null` = 说不出**（本机 tmux 快照不知道 / 读口抛了）
 *          ⇒ 调用方照原样传 `null`，后端降级回旧路。
 */
export async function mintLocalTmuxName(cwd: string): Promise<string | null> {
  try {
    const sessions = await commands.list_local_tmux();
    // `null` = 本机 daemon 通道没起 / 还没推过帧 = **不知道**。绝不退化成空集。
    if (!sessions) return null;
    return mintSessionTmuxName(cwd, new Set(sessions.map((s) => s.name)));
  } catch {
    // 读不到就当不知道 —— 与上面同一条纪律。
    return null;
  }
}

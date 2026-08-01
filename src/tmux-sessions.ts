/**
 * tmux 会话 ↔ 会话 sid 的**判据**（F51/F74/F03/A5+，逐字从 `tabs.ts` 搬出）。
 *
 * # 为什么搬
 *
 * G6 起第三个模块要用它：`fork-flow.ts` 得知道「源会话此刻在不在 tmux 里、在哪个名字下」
 * 才能给分叉出来的新会话取一个**不撞车**的名字。而 `tabs.ts` 会 import `fork-flow.ts`
 * （分叉成功后起会话），反过来 import 就成环了。
 *
 * 搬的是**判据**，不是缓存策略 —— `TMUX_CACHE_TTL_MS` 与 `tmuxCache` 仍住在 `TabManager` 里，
 * 它们是那个类的取数策略，不是「哪个 tmux 跑着哪个会话」这个问题的答案。
 *
 * 契约见 doc/INVARIANTS.md §30（靠 `@ccm_sid` 精确匹配，不靠名字/目录反推）。
 * `tabs.ts` 原样 re-export 这几个符号，既有 import 面（含 `tabs.vitest.ts`）零改动。
 */

import { AGENT_PROFILE } from "./agent-profile";

// G6：类型改用**生成物**。这里原本手抄了一份 Rust `TmuxSession` 的 interface，
// 两份各写各的、漂了也没人知道；Rust 侧已加 ts-rs 导出，改成 re-export 生成物。
export type { TmuxSession } from "./generated/TmuxSession";
import type { TmuxSession } from "./generated/TmuxSession";
/** F51：tmux 前台命令是否算 claude 会话。真机 tmux 多报 `claude`(调研 03 §2c 实测),
 * 但视启动路径也可能报解释器 `node`(claude 是 Node CLI)——两者都认,叠加 cwd 精确匹配
 * 收窄误配(D-正确性 Sug2:只认 claude 会在报 node 的环境静默失效)。 */
export function isClaudeTmuxCommand(cmd: string): boolean {
  return AGENT_PROFILE.livenessProcessNames.has(cmd);
}

/**
 * F74：在 tmux 会话列表里定位「正跑目标 sid 的活 claude」。**优先 `@ccm_sid` 精确匹配**——
 * 同目录多个 claude tmux（原会话 + `/branch` 出来的分支…）只有 `@ccm_sid` 能分清哪个是目标
 * 会话，且不被漂移骗（`__ccm_rbind` 随 /branch 实时更新它）。
 *
 * 精确没命中时：**只有当整张列表没有任何会话带 `@ccm_sid`**（老 wrapper / 未装）才回退按
 * `path===cwd` 猜（向后兼容）。只要有会话带了 sid、却没一个等于目标 sid，就说明目标会话不在
 * 任何 tmux 里（已结束 / 已漂移到别的 sid）——此时**绝不**按 cwd 抓一个同目录的别的 claude
 * （那正是撞错会话的老 bug），宁可返 undefined（SS-5/SS-9：找不到就报「不在」，不静默换一个）。
 * 契约与铁律见 doc/INVARIANTS.md §30。
 */
/**
 * F04（R10 根治）：`@ccm_sid` 精确命中该 sid 的**全部**活 claude 会话（不折叠成第一个）。
 * `findClaudeTmux` 用它重实现——`.filter(pred)[0]` 与旧版 `.find(pred)` 同一遍历顺序、同一
 * 结果，故 `findClaudeTmux` 的既有调用点/断言零改动。多数调用方仍只关心"有没有、是哪一个"，
 * 三处真正需要"是否命中 ≥2 个"的调用点（resume-attach 警告 / restart 拒绝 / 菜单 kill 项禁用）
 * 才用本函数，见各自调用点注释。
 */
export function findClaudeTmuxMatches(
  sessions: TmuxSession[] | null | undefined,
  sid: string,
): TmuxSession[] {
  return sessions?.filter((s) => s.sid === sid && isClaudeTmuxCommand(s.command)) ?? [];
}

export function findClaudeTmux(
  sessions: TmuxSession[] | null | undefined,
  sid: string,
  cwd: string,
): TmuxSession | undefined {
  const matches = findClaudeTmuxMatches(sessions, sid);
  if (matches.length > 0) return matches[0];
  const anySidKnown = sessions?.some((s) => s.sid != null);
  if (anySidKnown) return undefined;
  return cwd
    ? sessions?.find((s) => s.path === cwd && isClaudeTmuxCommand(s.command))
    : undefined;
}

/**
 * audit-fixes F03（idle-tmux）：找目标 sid 的**空 tmux**——`@ccm_sid` 精确命中该 sid、但当前
 * 前台命令**不是** claude（交互 shell，claude 已退出）。即三态里的 idle-tmux：会话还在、可 attach/
 * 就地 resume，但没在跑 claude。**只按 @ccm_sid 精确命中**（绝不按 cwd 猜，免撞同目录别的会话）。
 * F03.1 的就地复用 resume 与 F03.3 的 attach-into-idle 共用此判据（与 `findClaudeTmux` 互斥：
 * 后者要 command=claude，本函数要 command≠claude）。纯函数（node/jsdom 可测）。
 */
export function findIdleTmux(
  sessions: TmuxSession[] | null | undefined,
  sid: string,
): TmuxSession | undefined {
  return sessions?.find((s) => s.sid === sid && !isClaudeTmuxCommand(s.command));
}

/**
 * F74c(#60-B)：`findClaudeTmux` 对给定 sid 是否会走 **cwd 回退**（= 无精确 `@ccm_sid` 命中
 * **且**整张列表都无任何会话带 sid）。回退命中的会话是「同目录里的某个 claude」，可能不是目标
 * 会话——未装 / 老 `ccm` wrapper 的向后兼容路径。用户 2026-07-17 拍板：保留回退但**命中时显式提示**
 * （attach 那一刻 toast，别静默串味）。纯函数（node/jsdom 可测），判据与 `findClaudeTmux` 回退分支对齐。
 */
export function isCwdFallbackMatch(
  sessions: TmuxSession[] | null | undefined,
  sid: string,
): boolean {
  const exact = sessions?.some((s) => s.sid === sid && isClaudeTmuxCommand(s.command));
  if (exact) return false;
  const anySidKnown = sessions?.some((s) => s.sid != null);
  return !anySidKnown; // 无精确命中 + 无任一 sid → findClaudeTmux 会走 cwd 回退
}

/**
 * A5+ 优雅退出检测：目标 sid 的 claude 是否已**不在**（本工具）tmux 里精确命中——前台回到 shell
 * （CC 退出）或会话已没。判据与破坏性重启的守卫 `!live || live.sid !== sid` 完全一致：不再精确命中
 * = 已退出。**注**：`sessions == null`（list 失败）时也返回 true，故轮询方（`awaitExitFor`）**只在
 * list 成功时**调用它，list 失败当「未知」继续轮询、不误判成已退出。纯函数（node/jsdom 可测）。
 */
export function claudeExited(
  sessions: TmuxSession[] | null | undefined,
  sid: string,
  cwd: string,
): boolean {
  const live = findClaudeTmux(sessions, sid, cwd);
  return !live || live.sid !== sid;
}

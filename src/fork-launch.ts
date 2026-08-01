/**
 * G3（branch-anywhere）：**分叉出来的新会话该用什么参数起**。
 *
 * 用户要的是「分叉之后两条都活着，新会话跟原会话同账号、同 cwd、同样在不在 tmux 里」。
 * 本模块只回答**上半句里的「同什么」**：逐维度给出「知道，值是 X」或「不知道」——
 * **绝不猜**。起会话本身走既有 launch IR。
 *
 * # ★ 账号在会话退出后是**还原不出来**的（实测，别再找了）
 *
 * 三条路全查过：
 *
 * | 路 | 结论 |
 * |---|---|
 * | pidfile `sessions/<PID>.json` | 进程退出即消失 ⇒ **只对活着的会话有效** |
 * | jsonl 所在路径 | 各账号的 `projects/` 是**软链到共享的 `~/.claude/projects`**（同 inode）⇒ 路径**不编码**账号。这是 cc-acct-iso「隔离又同步」的设计：凭据分家、会话历史共享 |
 * | jsonl 内容 | 44 个顶层键里**没有**任何账号 / 邮箱 / configDir 字段 |
 *
 * ⇒ **对已退出的会话，账号一律 `unknown`。**
 *
 * # 为什么不肯拿「当前账号」顶替
 *
 * 那会**静默地用错身份跑一条对话**：你从三个月前某条消息分叉，它拿今天的账号起会话，
 * 界面上看不出任何异样。宁可多问一次，也不要产生一个「看起来对、身份是错的」的会话
 * ——同 `readiness.ts` 那条「缺 ≠ 不知道」：不知道就说不知道。
 */

// tmux 名的净化器与 `deriveTmuxName` **共用一份**（`forkTmuxName` 头注写了为什么）。
import { tmuxNameSegment } from "./shell-quote";

/** 某个维度的取值：知道（带来源）或不知道（带原因）。 */
export type Slot<T> =
  | { kind: "known"; value: T; from: string }
  | { kind: "unknown"; why: string };

export interface ForkLaunchFacts {
  /** 工作目录。jsonl 每条记录都带 `cwd`，所以历史会话也答得出。 */
  cwd: Slot<string>;
  /** 账号；`null` = 账号 0（不注入 `CLAUDE_CONFIG_DIR`）。 */
  account: Slot<string | null>;
  /** 原会话是不是跑在 tmux 里。 */
  tmux: Slot<boolean>;
}

export interface ForkLaunchInput {
  /** 源会话**此刻还活着吗**（进程在跑）。决定 pidfile 那条路通不通。 */
  sourceIsLive: boolean;
  /** 源 jsonl 里的 `cwd`（任何会话都有，含历史会话）。 */
  sourceCwd?: string | null;
  /**
   * 源会话当前所属账号的 configDir —— **仅当 `sourceIsLive` 时可信**
   * （来自 pidfile 路线）。`null` 表示确认是账号 0；`undefined` 表示没查到。
   */
  liveConfigDir?: string | null;
  /** 源会话所在的 tmux 会话名 —— 仅当 `sourceIsLive` 时可信；空/缺省 = 不在 tmux 里。 */
  liveTmuxName?: string | null;
}

const EXITED_WHY =
  "源会话已退出：账号只记在 pidfile 里（进程一退就没了），" +
  "会话文件本身与它所在路径都不带账号信息";

/** 逐维度推断。**纯函数**，不碰 IO。 */
export function inferForkLaunch(input: ForkLaunchInput): ForkLaunchFacts {
  const cwd: Slot<string> =
    input.sourceCwd && input.sourceCwd.trim()
      ? { kind: "known", value: input.sourceCwd, from: "会话记录里的 cwd" }
      : { kind: "unknown", why: "源会话记录里没有 cwd" };

  // ★ 账号：活着才可能知道。**已退出一律 unknown，不看 liveConfigDir 传了什么**
  //   —— 调用方可能顺手把「当前账号」塞进来，这里必须挡住。
  const account: Slot<string | null> = !input.sourceIsLive
    ? { kind: "unknown", why: EXITED_WHY }
    : input.liveConfigDir === undefined
      ? { kind: "unknown", why: "源会话活着，但没查到它属于哪个账号" }
      : {
          kind: "known",
          value: input.liveConfigDir,
          from: "源会话进程的 pidfile",
        };

  const tmux: Slot<boolean> = !input.sourceIsLive
    ? { kind: "unknown", why: "源会话已退出：它当初在不在 tmux 里无从查起" }
    : {
        kind: "known",
        value: Boolean(input.liveTmuxName && input.liveTmuxName.trim()),
        from: "tmux 会话清单",
      };

  return { cwd, account, tmux };
}

/** 还需要问用户的维度（按呈现顺序）。空 = 一次都不用问，可以直接起。 */
export function slotsNeedingInput(f: ForkLaunchFacts): Array<keyof ForkLaunchFacts> {
  const order: Array<keyof ForkLaunchFacts> = ["account", "tmux", "cwd"];
  return order.filter((k) => f[k].kind === "unknown");
}

/** 给人读的一句话，说清这一格为什么要问。 */
export function describeSlot(k: keyof ForkLaunchFacts, f: ForkLaunchFacts): string {
  const label = { account: "账号", tmux: "是否在 tmux 里跑", cwd: "工作目录" }[k];
  const s = f[k];
  return s.kind === "known"
    ? `${label}：跟原会话一致（据${s.from}）`
    : `${label}：需要你选一次 —— ${s.why}`;
}

/**
 * 新会话的 tmux 名。**必须与原会话不同**，否则 `ccm` 会把新会话
 * attach 进原会话那个窗口 —— 那正好毁掉「两条都活着」。
 *
 * 命名跟 `shared/ccm` 的 `<X>-cc` 形状一致，加 `-fork` 段；撞名后缀**追加在最后**
 * （`<X>-fork-cc-2`）—— 与 `remote-launch.ts::pickFreshTmuxName` 写下的同一条规则对齐：
 * 「让『第几个』始终是名字的末段」。
 *
 * # ★ Phase G 审计抓出的一个阻塞：基名必须净化
 *
 * 调用方在「源会话已退出 ⇒ 没有 tmux 名可继承」时会拿 **cwd** 当基名
 * （`fork-start.ts`）。此前本函数直接把它拼成 `/home/pi/proj-fork-cc`，
 * 而 `launch-requests.ts::planResumeTmux` 的 `/^[A-Za-z0-9_][A-Za-z0-9_-]*$/` 当场拒掉 ⇒
 * **「分叉一条已退出的远端会话」这条主路径 100% 起不来**（而且失败还被吞成成功 toast）。
 *
 * ⇒ 基名一律过 `tmuxNameSegment`（与 `deriveTmuxName` **同一个**净化器，不是另写一份）。
 * 净化后为空（如 cwd 是 `/`）→ 退回 `session`，与 `deriveTmuxName` 的兜底一致。
 */
export function forkTmuxName(sourceName: string, taken: readonly string[] = []): string {
  const base = tmuxNameSegment(sourceName.replace(/-cc$/, "")) || "session";
  const used = new Set(taken);
  const first = `${base}-fork-cc`;
  if (!used.has(first)) return first;
  let n = 2;
  while (used.has(`${first}-${n}`)) n += 1;
  return `${first}-${n}`;
}

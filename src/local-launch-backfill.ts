/**
 * 〔FE1 · 第四波 4D〕**本机起新会话之后，拿身份 token 反查出它的 sid，再把账号 pin 写上**（`K-P5h`）。
 *
 * 起停域的一格：起会话方在拉起那一刻不知道 sid（token 在 exec 之前就铸好了），会话出生之后
 * （`main.ts` 的 `onSessionStarted`）再问一次本机后端的 `accounts-sessions`。
 *
 * 从 `accounts.ts` 拆出来（审计 B §6 必须拆 4；守的要求 `设计/01 §5` D1）。
 */
import { LOCAL_ORIGIN } from "./ipc/origin";
import type { SessionAccount } from "./accounts";
import { fetchSessionAccounts } from "./account-reads";
import { recordLocalLaunchAccount } from "./launch-account";

/**
 * 一批 `--session-accounts` 的行里，**带着这个身份 token 的那条会话的 sid**。
 *
 * # 它买的是 `K-P5` 立项时那条结构性事实的另一半
 *
 * `K-P5 §3 三` 记着：**5 处起会话方，没有一处在起新会话时知道 sid** ——
 * 那正是身份 token 存在的全部理由。写侧铸 token（`K-P5b`）· backend 从
 * `/proc/<pid>/environ` 读回来（`K-P5f`）· 铸法把 token 交给调用方（`K-P5h` `KP5HD1`）
 * ⇒ 本函数是最后一跳：**起会话方终于说得出「我刚起的那条是哪个会话」。**
 *
 * # 🔴 fail closed —— 查不到就说查不到，**不猜**
 *
 * `K-P5f` 已经把 `launchId` 为 `null` 的**四种原因刻意合并成一个 `null`**
 *（没设 / 形状不合格 / 同一 token 落在一条以上活会话 / 进程已死）。本函数照这个口径：
 *
 * | 输入 | 答案 | 为什么 |
 * |---|---|---|
 * | token 空 / 缺席 | `null` | 「没铸出来」不是「匹配任何人」——空串当通配符是这一族最贵的错 |
 * | 没有任何行带这个 token | `null` | 会话还没起来，或它压根不是我们起的 |
 * | 命中的行 `alive:false` | `null` | 死进程的 environ 不作数（口径同 `restartLocateFailureMessage`） |
 * | 命中的行没有 `sessionId` | `null` | 认得出进程、说不出会话 ⇒ 说不出就不说 |
 * | **命中一条以上** | `null` | 判不出谁是原主。backend 侧本来就会把这种全置 `null`，**这一格不靠上游守** |
 *
 * # ⚠ 它答不到的（照抄 `K-P5f §12 裁七`·1 已登记的那格残留洞，别在这里悄悄拓宽）
 *
 * `CCM_LAUNCH_ID` 是**继承型**环境变量：在一条本工具起的会话里手敲 `claude` 起出来的孩子
 * 带着同一个 token。backend 挡得住「同一个 token 同时落在一条以上**活**会话上」，
 * **挡不住父会话已经退出**的那一格 ⇒ 那种情形下本函数会把**孩子**的 sid 当成答案，
 * **指错**。本件买不到它（那洞另有登记，不在本件账上），但**必须写在这里**：
 * 读作「**这条活着的会话的进程环境里带着这个 token**」，不读作「它就是我刚起的那条」。
 */
export function sidOfLaunch(
  rows: readonly SessionAccount[] | null | undefined,
  token: string | null | undefined,
): string | null {
  // 空串不是「有」（空值 ≠ 未设，本仓一贯口径）——**尤其**这里：空 token 若被当成
  // 「匹配 launchId 为空的行」，第一条没设身份的会话就会被认成「我刚起的那条」。
  if (!token || !rows) return null;
  const hit = rows.filter((r) => r.alive && r.launchId === token && r.sessionId);
  // 一条以上 ⇒ 判不出谁是原主 ⇒ 不猜（见头注那张表最后一行）。
  return hit.length === 1 ? (hit[0].sessionId ?? null) : null;
}

/**
 * 一次**已经发出去、还不知道 sid** 的本机新会话。
 *
 * `deadlineMs` / `asksLeft` 两个上限的理由见 [`resolvePendingLocalLaunches`] 的头注
 *（`KP5HD3`：「等多久 · 问几次 · 问不到怎么办」）。
 */
interface PendingLocalLaunch {
  token: string;
  /** 这次拉起用的账号名 —— 反查到 sid 之后要往 pin 里写的就是它。 */
  accountName: string;
  /** 过了这个时刻就不再问（**惰性判定**，见下）。 */
  deadlineMs: number;
  /** 还能问几次。 */
  asksLeft: number;
}

/**
 * 🔴 **等多久**：120s。claude 起来之后要先把 `sessions/<PID>.json` 写出来，
 * 而那之前 `--session-accounts` 里根本没有这条会话。120s 是「慢机器 + 冷缓存」的
 * 宽上界，不是期望值 —— 正常情形下第一次事件到达就命中了。
 *
 * ⚠ **这个上限是惰性判定的**：没有任何定时器盯着它。过期的条目在**下一次有人来问**时
 * 被顺手扫掉（[`resolvePendingLocalLaunches`] 与 [`rememberLocalLaunch`] 各扫一次）。
 * ⇒ 一条过期又再没有事件的条目会**留在表里不动**，但它既不发 IPC 也不写 pin，
 * 而且下一次起会话就会被扫掉 —— **代价是一个对象，不是一个唤醒**。
 */
export const PENDING_LAUNCH_TTL_MS = 120_000;

/**
 * 🔴 **问几次**：每条待回填最多 8 次。
 *
 * 每一次「问」= 一次经通道的 `accounts-sessions`（〔C4a〕此前是一次 E79 那条本机 Tauri 命令，一次 exec local_backend）。
 * 触发它的是**会话集合变化事件**，不是表 —— 一台机上短时间内起十几条会话是可能的，
 * 没有这个上限时，一条永远回填不了的待办会跟着每一次事件白发一次 IPC。
 */
export const PENDING_LAUNCH_MAX_ASKS = 8;

/**
 * 🔴 **同时最多挂几条**：4。超了就丢**最老**的那条。
 *
 * 上限的用途不是省内存，是让「回填失败」有一个**确定的**结局：
 * 表长不封顶时，一次失败的回填会一直占着位置，直到某个说不清的时刻。
 */
export const PENDING_LAUNCH_CAP = 4;

let pendingLocalLaunches: PendingLocalLaunch[] = [];

/**
 * 记下「我刚发出去一次本机新会话拉起，token 是这个，将来要把 pin 记到这个账号上」。
 *
 * 调用方是 `views/history.ts::runNewSession`（**全仓唯一**起本机新会话的那条路）。
 *
 * ⚠ **账号名说不出就不记**：那时反查到 sid 也无事可做（`recordLocalLaunchAccount`
 * 本来就「没表态就不记」）——挂一条什么都不会做的待办只会白发 IPC。
 * ⚠ **resume 那一支不进这张表**：`K-P5g` 现打过，resume 时 token 就是 sid，
 * 「反查」退化成「答案要么是它自己要么 `null`」⇒ 那一支根本不需要回填。
 */
export function rememberLocalLaunch(
  token: string | null | undefined,
  accountName: string | null,
  nowMs: number = Date.now(),
): void {
  if (!token || !accountName) return;
  // 顺手扫过期（惰性，见 `PENDING_LAUNCH_TTL_MS` 头注）。
  pendingLocalLaunches = pendingLocalLaunches.filter((p) => p.deadlineMs > nowMs);
  pendingLocalLaunches.push({
    token,
    accountName,
    deadlineMs: nowMs + PENDING_LAUNCH_TTL_MS,
    asksLeft: PENDING_LAUNCH_MAX_ASKS,
  });
  // 超了丢最老的那条（`shift`，不是 `pop` —— 新的那条才是用户刚点的）。
  while (pendingLocalLaunches.length > PENDING_LAUNCH_CAP) pendingLocalLaunches.shift();
}

/**
 * 把还挂着的那几条待回填**问一遍**：拿 token 反查 sid，查到就把账号 pin 写上。
 *
 * # 🔴 `KP5HD3`：token 是在会话**起来之前**铸的，所以回填必然是「过一会儿再问」
 *
 * `--session-accounts` 要**进程已经在跑**才读得到，而 token 在 `exec` 之前就铸好了
 * ⇒ 拉起返回的那一刻问，必然查不到。「过一会儿再问」有两种做法，本件选的是第二种：
 *
 * | 做法 | 为什么不是它 / 为什么是它 |
 * |---|---|
 * | 起一个定时器隔 N 秒重试 | 🔴 **本项目有一条已交付的性质是「判活不靠定时轮询（内核一有事就通知）」**（`polling_registry` / `rust_timer_registry` 两张表在管），在这里开一个新的周期唤醒就是开倒车 |
 * | ★ **搭已有的那条事件**：会话集合变了才问 | backend 侧 `sessions/<PID>.json` 的变化本来就会一路走到前端的 `session-started` 事件（`lib.rs` 那个 `session-changes-emitter`）——**一条新会话出生正是它响的时刻**，而这正是我们要等的那件事 |
 *
 * ⇒ **本函数不排任何定时器**，它的调用方是 `main.ts` 里 `onSessionStarted` 那一跳。
 * 「等多久 / 问几次 / 问不到怎么办」三格分别由 [`PENDING_LAUNCH_TTL_MS`] ·
 * [`PENDING_LAUNCH_MAX_ASKS`] · 下面那条「查不到就留着，问完就丢」回答。
 *
 * # 问不到怎么办
 *
 * **什么都不做，且不猜**：pin 不写（= 今天的行为，逐字节旧路），
 * 次数用完或过了 TTL 就把待办丢掉。⚠ 不许回落到「拿当前账号当 pin」——
 * 那正是 `#75` 那个病灶的形状（把一条会话悄悄绑到别的号上）。
 *
 * # ⚠ 它买不到什么（如实写）
 *
 * - **Windows**：`accounts-sessions` 要读 `/proc/<pid>/environ`，
 *   那边恒 `available:false` ⇒ 这条路上回填**永远查不到**，静默作废（fail closed）。
 *   那是 `K-P5f` 已登记的残留洞，不在本件账上。
 * - **父会话已死的继承值**：见 [`sidOfLaunch`] 头注 —— 那一格上回填会**指错**。
 * - **会话起来了但那条事件没响**：本函数就再也不会被叫到（没有兜底轮询，这是刻意的）。
 *   代价明确：那一次的 pin 没写上，与本件没做完全一样，**不会写错**。
 */
export async function resolvePendingLocalLaunches(nowMs: number = Date.now()): Promise<void> {
  // 惰性扫过期 + 次数用尽（两条都在这里收口，别散到调用点上）。
  pendingLocalLaunches = pendingLocalLaunches.filter(
    (p) => p.deadlineMs > nowMs && p.asksLeft > 0,
  );
  if (pendingLocalLaunches.length === 0) return;

  // 〔C4a〕经通道问本机后端（`fetchSessionAccounts` 那一处，`force`：刚起的会话不许被 8 秒缓存挡住）。
  //   查不到（Windows / 没有本机后端 / 没有控制通道）⇒ 空行集 ⇒ 下面一条都命不中 = 不猜，待办留着等下一次事件。
  const rows: SessionAccount[] = await fetchSessionAccounts(LOCAL_ORIGIN, true);

  const still: PendingLocalLaunch[] = [];
  for (const p of pendingLocalLaunches) {
    p.asksLeft -= 1;
    const sid = sidOfLaunch(rows, p.token);
    if (sid) {
      // ★ 这一行就是本件的正题：**起会话方终于知道「我刚起的那条是哪个会话」**，
      //   于是那条今天写不出来的 pin 写得出来了（`recordLocalLaunchAccount` 头注里
      //   「新会话无 sid → 不记账」那一格）。
      recordLocalLaunchAccount(sid, p.accountName);
      continue; // 命中即出表
    }
    if (p.asksLeft > 0) still.push(p);
  }
  pendingLocalLaunches = still;
}

/** 只给判据用：清空待回填表（生产段没有调用方）。 */
export function __resetPendingLocalLaunchesForTests(): void {
  pendingLocalLaunches = [];
}

/** 只给判据用：还挂着几条待回填。 */
export function __pendingLocalLaunchCountForTests(): number {
  return pendingLocalLaunches.length;
}

// ------------------------------------------------------------ config.json

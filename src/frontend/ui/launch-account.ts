/**
 * **起会话挑哪个号** —— 起停域里「账号」那一格的唯一住址。
 *
 * - 远端（及「开新 Claude」）：[`withAccount`] —— 显式选号 / 跟随（这条会话的 pin → 当前号 → 基座）解析，注入，起成了记 pin；
 * - 本机：同步快照（[`localLaunchAccountSync`] 一族，量出来的：主路不许多等一拍）＋ 载荷上 `account` 那一格的形状（键名来自生成物，`K-R95`）；
 * - 两边同一个出口：要的号选不了 ⇒ [`refuseUnavailableAccount`]（D-h：不起、说清、给「用当前账号」的显式选择）；
 * - pin 存在历史注解里（`history-reads.ts` 的 `annotate` / `lastAccounts`）—— 账号面里**唯一**碰历史域的一处就在这里。
 *
 * 从 `accounts.ts` 拆出来（审计 B §6 必须拆 4：那个文件跨账号 · 起停 · 历史三个域）。
 * 守的要求：「一个判定只有一个家」· 「「哪个账号」非有不可 —— 缺了 resume 会静默落到默认号，撞 `D4`」。
 */
import type { LaunchModifiers } from "./launch-types";
// 🔴 `K-R95`（定框 `K28`：前端不许自己发明对外行为）：本机拉起载荷里「哪个号」那一格的
// **wire 键名从后端来**，前端不再自己写 `{ kind: "named", configDir, name }` 这三个字面量。
// 源：`src/backend/control/launch_render/wire.rs::export_bindings_launch_render_facts`
// （它每次生成都跑一遍 `history.rs::LaunchAccount` 的生产反序列化器验一次）。
import { LOCAL_LAUNCH_ACCOUNT_WIRE } from "./generated/launch-render-facts";
import { isLocalOrigin, LOCAL_ORIGIN } from "./ipc/origin";
import { appStore, putAccounts } from "./app-store";
// 上次账号那一份注解归本机常驻后端（`history-last-accounts` / `history-annotate`）。
import { annotate, lastAccounts } from "./history-reads";
import { showActionFailureToast } from "./error-toast";
import { copyText } from "./copy-table";
import { currentWorkingAccount, isSelectable, resolveAccount, alternativeAccountOf, type AccountResolution, type AccountsState } from "./accounts";
import { fetchAccounts, fetchLocalAccounts } from "./account-reads";
import { getModelForAccount } from "./account-prefs";

/**
 * `K-H2b` `D1 阻-1`：**本机起会话时把账号说出来** —— 三条主路共用的唯一取值口。
 *
 * # 它为什么必须存在
 *
 * `D1` 现打、PM 复核：`tabs.ts` 那处 `invoke("resume_history_session", …)` 与
 * `views/history.ts` 那两处**一个账号都没传**，而 `history.rs` 自己的注释就写着
 * 「`fork-flow.ts` 是**全仓唯一**给 `resume_history_session` 传 `configDir` 的」。
 * ⇒ 主路上账号恒缺席，后果两条：① 起会话落到 shell rc 里那个默认号上（静默串号）；
 * ② 中转那一格**永远拼不出路由键**（没有 id ⇒ 不注入）。
 * **一件叫「接上注入点」的东西，主路没接。**
 *
 * # ★★ 它为什么是**同步**的（这一格是量出来的，不是选出来的）
 *
 * 第一版写成 `async`，在三条主路上 `await` 一下再拼进参数。**实测当场红两条**：
 * `views/history-search-resume.vitest.ts:67` 与 `views/history-actions.vitest.ts:95`
 * 逐字 `btn.click(); await Promise.resolve();` —— **只放行一个微任务**，
 * 而 `await` 一次就多一拍 ⇒ 那两条断言在 invoke 还没发出去时就跑了。
 * 那两个 vitest **不在本件写区**，而「为了让判据过而去改判据」是本区禁的方向。
 * ⇒ 换成**同步读快照**：取值那一跳零 `await`，主路的时序**一拍不动**。
 *
 * # 快照从哪来、冷的时候怎么办（**诚实边界**）
 *
 * 快照由 [`fetchLocalAccounts`]（chip 每次 refresh 都调）与
 * [`primeLocalLaunchAccounts`]（三条主路各在自己那一跳**不等待**地踢一脚）填。
 * ⚠ **进程起来后的第一次起会话，快照可能还是冷的** ⇒ 本函数回 `undefined`
 * = 「没表态」= **逐字节旧行为**（不注入、不切号）。
 * **这是一个真的洞，不是「应该没事」**：它的代价是那一次会话不走中转。
 * 消掉它要么让主路等一拍（撞上面那两条判据），要么在启动时就 prime
 * （那是另一件事的接线面）。⇒ 如实登记。
 *
 * # 取哪个账号（两条路，各自的理由）
 *
 * | 场景 | 取谁 | 为什么 |
 * |---|---|---|
 * | resume 一条已有会话 | 那条会话**上次用的**账号（`list_last_accounts` 的 pin） | 与远端那条路同形（`withAccount(..., {follow:{lastAccount}})`）；用别的号 resume 会在错的数据目录里找不到会话（`#75` 那一族） |
 * | 起新会话 | **当前账号**（`currentWorkingAccount`） | 「新开一个」本来就该用用户此刻选中的那个 |
 *
 * **说不出就缺席，绝不猜**：快照没有 / pin 指向一个已经不可选的号 / 那个号没有
 * `configDir` ⇒ 一律 `undefined`。⚠ 尤其**不回落到「当前账号」** —— 那会把一条
 * resume 悄悄换到别的号上，正是 `#75` 那个病灶的形状。
 */
let localLaunchPins: Record<string, string> | null = null;

/**
 * 〔「账号快照……收进一处」〕本机那份账号清单**不在这里另存**：读 `appStore.accounts` 里本机那一格
 * （`account-reads.ts::fetchAccounts` 每取回一次就换进去）；这里只留 resume 跟随要的「上次用的号」（history-metadata 的 pin，不是账号快照）。
 * 两样都到了才算热 —— 只有清单没有 pin 时照旧不表态（否则 resume 会落到当前号上，`#75` 那一形）。
 */
function localLaunchSnapshotNow(): { state: AccountsState; pins: Record<string, string> } | null {
  const state = appStore.accounts.get().get(LOCAL_ORIGIN) ?? null;
  return state && localLaunchPins ? { state, pins: localLaunchPins } : null;
}

/**
 * 上面那条的**取名字**半 —— 与取 `configDir` 那半共用同一条规则（不许两处各判一次）。
 *
 * # 规则（`D2 阻-3` / `D3 阻-2` 之后改成与 `withAccount` **同源**）
 *
 * | 这一格 | 取谁 | 与远端那条 `withAccount` 的关系 |
 * |---|---|---|
 * | 有 pin，且那个号可选 | **pin** | 同（`opts.follow.lastAccount` 优先） |
 * | 有 pin，但那个号**不可选** | **不表态**（`null`） | 同（`withAccount` 那一支「下沉到 current ⇒ 不记账」，保住原 pin —— 悄悄翻成当前号正是 `#75` 那个病灶） |
 * | 没有 pin | **当前账号**（`currentWorkingAccount`） | 同（远端那条没 pin 时同样落到当前号） |
 *
 * 🔴 **上一拍这里读的是 `a.isDefault`（manifest 字段），而头注写的是 `currentWorkingAccount`
 * （优先 config.json 的 `defaultName`）—— 两者在「用户切过号」之后就不是同一个答案。**
 * 后果是**切过号之后新会话静默串号**，而且上游选择会按错的 id 换上别人那一行的 key。
 * ⇒ 快照现在整份存 `AccountsState`（`defaultName` 在里面），这里直接调那条唯一的规则。
 */
export function localLaunchAccountNameSync(sid: string | null): string | null {
  const snap = localLaunchSnapshotNow();
  if (!snap) return null; // 快照还是冷的 ⇒ 没表态（见下面那条诚实边界）
  const pin = sid ? snap.pins[sid] : undefined;
  if (pin) {
    const hit = snap.state.accounts.find((a) => a.name === pin);
    // ⚠ 有 pin 但那个号不可选 ⇒ **不表态**，绝不下沉到当前号（那会把一条会话悄悄翻号）。
    return hit && isSelectable(hit) ? hit.name : null;
  }
  const cur = currentWorkingAccount(snap.state);
  return cur && isSelectable(cur) ? cur.name : null;
}

/**
 * 本机 resume 跟随那一态的**三种答案**（取名字那半的规则不变，只是把「说不出」拆开）：
 *
 * | 答案 | 什么时候 | 调用方怎么办 |
 * |---|---|---|
 * | `named` | pin 可选 / 没 pin 而当前号可选 | 带这个号起 |
 * | `silent` | 快照还冷 / 没 pin 也没可选的当前号 / 那个号没有 `configDir` | 缺席（逐字节旧行为，见上面那条诚实边界） |
 * | `pinGone` | **有 pin，而那个号选不了** | **不起**，说清、给「用当前账号」的显式选择（`launch-account.ts::refuseUnavailableAccount`） |
 *
 * 🔴 `pinGone` 先前落进 `silent`：载荷不带账号 ⇒ 落 shell rc 里的默认号，**不说一个字**（E7 的本机那一形）。
 * 「「哪个账号」非有不可 —— 缺了 resume 会静默落到默认号，撞 `D4`」。
 */
export type LocalFollowPlan =
  | { kind: "named"; name: string; configDir: string; wire: LocalLaunchAccountWire }
  | { kind: "silent" }
  /**
   * `alternative`：当前可选的号（名字 ＋ 目录）；`null` = 没有 ⇒ 给的选择是「不指定账号」。
   * `listKnown: false` = 账号清单读不到（`available:false`）⇒ 说不清当前号是谁，选择只给「不指定账号」。
   */
  | {
      kind: "pinGone";
      pin: string;
      listKnown: boolean;
      alternative: { name: string; configDir: string } | null;
    };

export function localFollowPlan(sid: string): LocalFollowPlan {
  const snap = localLaunchSnapshotNow();
  if (!snap) return { kind: "silent" };
  const pin = snap.pins[sid];
  if (pin) {
    const hit = snap.state.accounts.find((a) => a.name === pin);
    if (!hit || !isSelectable(hit)) {
      if (!snap.state.available) return { kind: "pinGone", pin, listKnown: false, alternative: null };
      const alt = alternativeAccountOf(snap.state, pin);
      const altAcct = alt === null ? undefined : snap.state.accounts.find((a) => a.name === alt);
      return {
        kind: "pinGone",
        pin,
        listKnown: true,
        alternative: altAcct?.configDir ? { name: altAcct.name, configDir: altAcct.configDir } : null,
      };
    }
  }
  const wire = localLaunchAccountSync(sid);
  const name = localLaunchAccountNameSync(sid);
  const configDir = localLaunchConfigDirSync(sid);
  return wire && name && configDir ? { kind: "named", name, configDir, wire } : { kind: "silent" };
}

/**
 * 上面那条的**载荷半** —— 把「哪个号」摊成后端收得下的形状。
 *
 * # 🔴 `K-R53`（09-11）：**名字也要交出去，不只是目录**
 *
 * 后端那条 ccm 路只会 `--account <名字>`（`shared/ccm:606`）。本函数先前只回
 * `{kind:"named", configDir}` ⇒ Rust 那侧的 `LaunchAccount::Named` 手上**没有名字**
 * ⇒ 本机后端那条 ccm 路（当时的渲染函数）对它必然 §35 短路 ⇒ **本机具名账号一条都进不了
 * ccm 容器**。而盘上四个本机拉起入口里有三个只说得出具名账号（`tabs.ts` 一处 +
 * `views/history.ts` 两处，人群由 `ipc/commands.vitest.ts` 那条「恰好 4 处」钉着）
 * ⇒ 那三条**在类型上**就到不了后端那条路，100% 落第二实现。
 *
 * ⚠ 名字这一半**本来就在手上**（[`localLaunchAccountNameSync`]，与取目录那半同源）——
 * 缺的从来不是数据，是**没往下传**。所以这里是把同一条规则的两半一起交出去，
 * **不是**在后端那侧从目录名反推一个名字：反推错的失效方向是 `shared/ccm` 当场 `die`
 *（退出码 2 = 一次本来能起的会话变成一条报错），与 `apikey_account_id_of_dir`
 * 那条「推错就回落」的保守方向相反。理由逐字住 `history.rs` 的 `LaunchAccount::Named::name`。
 */
export type LocalLaunchAccountWire = Record<
  typeof LOCAL_LAUNCH_ACCOUNT_WIRE.tag,
  typeof LOCAL_LAUNCH_ACCOUNT_WIRE.named
> &
  Record<typeof LOCAL_LAUNCH_ACCOUNT_WIRE.configDir, string> &
  Record<typeof LOCAL_LAUNCH_ACCOUNT_WIRE.name, string>;

/**
 * 本机这次起会话**要用的账号配置目录**（[`localLaunchAccountSync`] 载荷里 `configDir` 那一格的同一个值，
 * 同一个快照、同一条规则）。说不出 ⇒ `undefined`（基座）。resume 之前问记录还在不在，要查的就是这棵树
 * （`tab-session-actions.ts::recordStillThere`）。
 */
export function localLaunchConfigDirSync(sid: string | null): string | undefined {
  const snap = localLaunchSnapshotNow();
  const name = localLaunchAccountNameSync(sid);
  if (!snap || !name) return undefined;
  return snap.state.accounts.find((a) => a.name === name)?.configDir || undefined;
}

export function localLaunchAccountSync(sid: string | null): LocalLaunchAccountWire | undefined {
  const snap = localLaunchSnapshotNow();
  const name = localLaunchAccountNameSync(sid);
  if (!snap || !name) return undefined;
  const picked = snap.state.accounts.find((a) => a.name === name);
  // 🔴 `K-R95`：**计算键**，不是三个字面量。这一行此前逐字是
  //   `{ kind: "named", configDir: picked.configDir, name }`
  // —— 那是前端自己渲染了一遍后端的载荷形状，而两边靠头注里一句「同源」对齐。
  // 现在键名与判别值都来自生成物 ⇒ `history.rs::LaunchAccount` 那边改名，
  // `npm run gen:types` 当场 panic（生成器跑生产反序列化器验过），
  // 跑完之后**本函数吐出去的键跟着变**，不用回来改这里。
  //
  // ⚠ 载荷里**带什么**一个字没改（`K-R95` `§0b`：只改「谁渲染它」）：
  // 仍是「说不出就缺席，绝不猜」，仍不回落到「当前账号」。
  return picked?.configDir ? namedLocalAccountWire(picked.configDir, name) : undefined;
}

/** 具名账号 → 本机载荷上的 `account`（名字与目录一起交，`K-R53`）。键名与判别值都来自生成物（`K-R95`）。 */
export function namedLocalAccountWire(configDir: string, name: string): LocalLaunchAccountWire {
  return {
    [LOCAL_LAUNCH_ACCOUNT_WIRE.tag]: LOCAL_LAUNCH_ACCOUNT_WIRE.named,
    [LOCAL_LAUNCH_ACCOUNT_WIRE.configDir]: configDir,
    [LOCAL_LAUNCH_ACCOUNT_WIRE.name]: name,
  };
}

/** 本机载荷上 `account` 那一格能装的三种形状之二（第三种 = 缺席 = 没表态）。 */
export type LocalAccountWire =
  | Record<typeof LOCAL_LAUNCH_ACCOUNT_WIRE.tag, typeof LOCAL_LAUNCH_ACCOUNT_WIRE.base>
  | LocalLaunchAccountWire
  | (Record<typeof LOCAL_LAUNCH_ACCOUNT_WIRE.tag, typeof LOCAL_LAUNCH_ACCOUNT_WIRE.named> &
      Record<typeof LOCAL_LAUNCH_ACCOUNT_WIRE.configDir, string>);

/**
 * **用户显式选的那一格** → 本机载荷上的 `account`（分叉小窗 · 换号重启菜单）。
 *
 * - `configDir === null` = 账号 0 ⇒ `base`（后端产出 `unset CLAUDE_CONFIG_DIR`）—— **不是省略**：
 *   省略 = 没表态 = 一个字都不注入，会被 shell rc 里的默认号顶掉（Phase G 抓出的静默串号）。
 * - 具名：名字说得出就一起交（`K-R53`）；说不出（分叉继承的是源会话的**目录**）⇒ 只交目录，
 *   后端诚实短路回旧路，**绝不从目录名反推**（`accounts.ts` 的 Z01：空值 ≠ 未设）。
 */
export function explicitLocalAccountWire(configDir: string | null, name: string | null): LocalAccountWire {
  if (configDir === null) return { [LOCAL_LAUNCH_ACCOUNT_WIRE.tag]: LOCAL_LAUNCH_ACCOUNT_WIRE.base };
  return name === null
    ? {
        [LOCAL_LAUNCH_ACCOUNT_WIRE.tag]: LOCAL_LAUNCH_ACCOUNT_WIRE.named,
        [LOCAL_LAUNCH_ACCOUNT_WIRE.configDir]: configDir,
      }
    : namedLocalAccountWire(configDir, name);
}

/**
 * `D3 阻-2` / `阻-3`：**本机这条路也要往 pin 里写。**
 *
 * 现打（`D3`，PM 复核属实）：`recordLastAccount` 的生产调用点**恰好 2**，
 * 而两处**结构上只走远端** —— `withAccount(` 的 6 个生产调用点 **6/6** 在 `origin` 分支内
 * （`tabs.ts` 那处自陈「只在远端调」）；`restartWithAccount(` 的唯一调用点首行逐字
 * `if (tab.origin === null) return false;`。⇒ **本机的 `list_last_accounts` 恒空**，
 * 于是上面那个取值口的「pin 优先」那一支**在本机永远走不到**。
 *
 * ⇒ 本机起会话成功之后由调用方喊一声。**不等待**（同 prime，多一拍会撞那两条 DOM 判据）。
 * ⚠ 只在**真的用了一个具名账号**时记 —— 没表态就不记，别把「不知道」写成一条 pin。
 */
export function recordLocalLaunchAccount(sid: string, name: string | null): void {
  if (!sid || !name) return;
  void recordLastAccount(sid, name);
}

/**
 * 把上面那份快照填上。**调用方不许 `await` 它**（那就又多一拍了，见 [`localLaunchAccountSync`] 头注）。
 *
 * ⚠ 它自己吞掉所有异常：这条路上任何一步坏掉的**正确行为都一样** —— 快照留旧的 / 留空，
 * 下一次取值回 `undefined` = 逐字节旧行为。让它抛出去会把一次能起的会话变成一个 toast。
 */
export function primeLocalLaunchAccounts(): void {
  void (async () => {
    try {
      const [state, pins] = await Promise.all([
        fetchLocalAccounts(),
        lastAccounts().catch(() => ({}) as Record<string, string>),
      ]);
      // ⚠ 整份存 `state`，**不是**只存 `accounts` —— 「当前账号」这条规则要读
      //   `state.defaultName`（config.json），只留 accounts 就只剩 manifest 的 `isDefault`，
      //   那正是上一拍那条静默串号的成因（`D2 阻-3`）。
      // 清单已由 `fetchLocalAccounts` 换进 `appStore.accounts`（本机那一格）；这里只记 pin。
      if (state) localLaunchPins = pins ?? {};
    } catch {
      /* 保持旧快照 —— 见上 */
    }
  })();
}

/** 只给判据用：把快照清回冷态（生产段没有调用方）。 */
export function __resetLocalLaunchSnapshotForTests(): void {
  localLaunchPins = null;
  const next = new Map(appStore.accounts.get());
  next.delete(LOCAL_ORIGIN);
  appStore.accounts.set(next);
}

/** 只给判据用：直接喂一份快照（免得判据去摆布两条 IPC 的时序）。 */
export function __setLocalLaunchSnapshotForTests(
  state: AccountsState,
  pins: Record<string, string>,
): void {
  putAccounts(LOCAL_ORIGIN, state);
  localLaunchPins = pins;
}

/**
 * A4：记录「这个会话上次用账号 X 起」到 history-metadata（源②，DESIGN §3）。history / tabs
 * 两处「带账号 resume」共用。失败不挡 resume 本身（记忆是非关键路径），但**要说一句**
 * 〔E §3.3〕：原先只打 console，下次 resume 的账号跟随悄悄失准，用户无从知道为什么。
 */
export async function recordLastAccount(sessionId: string, account: string): Promise<void> {
  try {
    await annotate(sessionId, { lastAccount: account });
  } catch (e) {
    console.warn("record lastAccount failed:", e);
    showActionFailureToast(
      copyText("launchAccount.lastAccount.notRecorded"),
      copyText("launchAccount.lastAccount.notRecordedBody", { account, e: String(e) }),
      { level: "info", durationMs: 6000 },
    );
  }
}

/**
 * 这次起会话用哪个号：账号清单（读不到 ⇒ `undefined`）＋ 显式点的号 / 跟随（这条会话上次的号）⇒ 解析结局。
 * `withAccount` 与 tab 栏批量起共用这一份（批量那边每台只取一次账号清单）。
 */
export function resolveLaunchAccount(
  state: AccountsState | undefined,
  accountName: string | null,
  follow?: { lastAccount?: string | null },
): AccountResolution {
  const priorPin = follow?.lastAccount ?? null;
  return state
    ? resolveAccount(state, { explicit: accountName, follow })
    : accountName
      ? { kind: "unavailable", requestedName: accountName }
      : priorPin
        ? { kind: "unavailable", requestedName: priorPin, pinned: true }
        : { kind: "base" };
}

/**
 * 跟随解析命中之后记不记「上次用的号」—— U3 审计 重要-1：不 clobber 既有 pin。仅当**无既有 pin**（变 sticky）、
 * 或**解析结果 == 既有 pin**（no-op）时才记；既有 pin 存在但不可选、下沉到当前号 ⇒ **不记**，保住原 pin
 * （守「粘性优先」，避免默认 resume 把会话账号悄悄翻成当前账号）。
 */
export function followRecordName(priorPin: string | null, resolution: { name: string }): string | null {
  return !priorPin || resolution.name === priorPin ? resolution.name : null;
}

/**
 * A4：**统一「带账号起会话」编排**——history resume / tabs resume / 「开新 Claude」对话框
 * 三站点共用，消除各写一遍 `resolve configDir + record lastAccount` 的漂移（DESIGN §4）。
 * A5「换号重启」是本编排的超集（在 run 前插 checkTrust/compact、run 后同样 record），届时在此扩展。
 * F05：内部改用 `resolveAccount` 求解（行为逐字节不变，见其头注）；`run` 回调新增第二参数
 * `accountName`——命中账号时非空，否则 `undefined`（F05 交付）。
 * F07：模型偏好 `modelOverride`——命中账号时查一次 `getModelForAccount`（本机 config.json 偏好）。
 * **R03**：这三者不再是三个位置参数，统一收进 `LaunchModifiers` 一次交给 `run`。
 *
 *   - `accountName == null` **且无 `opts.follow`** → 默认起：`run({ 三字段皆 undefined })`（不注入、不记账、不 fetch，A4 逐字节旧行为）。
 *   - `accountName == null` **且有 `opts.follow`**（account-ux U2 opt-in 跟随）→ `fetchAccounts` 后：
 *       · 会话**有 pin、而那个号选不了**（或账号清单读不到）⇒ **不起**，[`refuseUnavailableAccount`]
 *         说清、给「用当前账号」的显式选择（先前这里静默下沉到当前号 / 基座 —— E7）；
 *       · 否则经 `resolveFollowAccount`（lastAccount → 当前账号 → null）解析：命中则注入其 configDir +（给了
 *         sessionId 时）记 lastAccount（会话账号 sticky 自增强）；解析不到（没 pin、也没可选的当前号）→ `run({ 三字段皆 undefined })` 落基座。
 *   - `accountName` 非空 → `fetchAccounts` 解析 configDir：
 *       · 解析不到（不可选 / 账号库不可用）⇒ **不起**，同上说清 ＋ 显式选择（先前：调用方 toast 后**退化为默认起**）；
 *       · 解析到 → `run({ configDir, accountName, modelOverride })`；再在**给了 sessionId 时**记 lastAccount（源②，新会话无 sid 不记）。
 * `run` 内部的拉起失败由 run 自己处理（runRemote* 有复制命令回退）；本编排只统一 resolve/record 口径。
 */
export async function withAccount(
  origin: string,
  accountName: string | null,
  /** R03：收 `LaunchModifiers` 而非三个位置参数。这里曾是**整条位置参数长列车的车头**——
   *  F05 加 `accountName`、F07 再加 `modelOverride`，每次都要同时改这个签名与全部调用点，
   *  于是成功标准②（加维度零改调用点）永远差最后一层。收成 bag 后，
   *  加第 4 个维度只需本函数内部往 `mods` 里多塞一个字段，**6 个**调用点一个字符都不用改
   *  （审计核实是 6 不是 7：`remote-section.ts` 1 + `views/history.ts` 2 + `tabs.ts` 3）。
   *  注意这只对"值能由本函数自己推出"的维度成立；若是用户在 UI 现场勾选的维度
   *  （如 `--dangerously-skip-permissions`），本函数推不出来，届时需给 `opts` 加
   *  `extraModifiers?: LaunchModifiers` 让调用方注入并在内部 merge，那时 lambda 才真的零改。 */
  /** 回 `false` = 这一趟没起（被记录探针拦下）⇒ 不记「上次用的账号」。 */
  run: (mods: LaunchModifiers) => Promise<void | false>,
  opts: {
    sessionId?: string;
    /** account-ux U2:仅当 accountName===null 时生效——启用「跟随」解析(lastAccount→当前账号→基座)。 */
    follow?: { lastAccount?: string | null };
  } = {},
): Promise<void> {
  let state: AccountsState | undefined;
  if (accountName || opts.follow) {
    try {
      state = await fetchAccounts(origin);
    } catch {
      state = undefined; // 账号库拿不到（fetchAccounts 通常不抛，防御性兜底）
    }
  }
  const priorPin = opts.follow?.lastAccount ?? null;
  const resolution = resolveLaunchAccount(state, accountName, opts.follow);
  // 🔴 D-h：要的那个号选不了 ⇒ **不起**，说清是哪个号、给一个显式选择（点了就以显式选号再走一次，A4 语义记 pin）。
  //   先前：显式点号 ⇒ 提示后按基座起（提示说的「改用上次的账号 / 当前账号」与做的也不一致）；
  //   跟随 ⇒ 下沉、不说（E7）。两形都是「不静默换号」要拦的（＋ D4）。
  if (resolution.kind === "unavailable") {
    refuseUnavailableAccount({
      machine: origin,
      name: resolution.requestedName ?? "",
      pinned: resolution.pinned === true,
      // 账号清单读不到（`available:false` / 抛）⇒ 说不清「当前账号」是谁 ⇒ 给的选择只剩「不指定账号」。
      listKnown: state?.available === true,
      alternative: state?.available ? alternativeAccountOf(state, resolution.requestedName ?? "") : null,
      choose: (alt) => withAccount(origin, alt, run, { sessionId: opts.sessionId }),
    });
    return;
  }

  let configDir: string | undefined;
  let recordName: string | null = null; // 成功注入后要记的账号名(显式=accountName / 跟随=解析名)
  if (resolution.kind === "account") {
    configDir = resolution.configDir;
    if (accountName) {
      // 显式选号(A4 语义不变)
      recordName = resolution.name;
    } else {
      recordName = followRecordName(priorPin, resolution);
    }
  }
  const modelOverride =
    resolution.kind === "account" ? await getModelForAccount(resolution.name) : undefined;
  const launched = await run({
    configDir,
    accountName: resolution.kind === "account" ? resolution.name : undefined,
    modelOverride,
  });
  if (launched !== false && recordName && configDir && opts.sessionId) {
    void recordLastAccount(opts.sessionId, recordName);
  }
}

/**
 * **账号选不了 ⇒ 不起、说清、给「用当前账号」的显式选择** —— 本机远端同一句话、同一个出口。
 *
 * 守的要求：D-h（「选不了原账号时 resume ⇒ 照 / D4：不静默换号，拒并说清、给「用当前账号」的显式选择」）；
 * 「「哪个账号」非有不可 —— 缺了 resume 会静默落到默认号，撞 `D4`」；
 * 「**显式反馈优于静默隐藏**」。
 *
 * @param listKnown 账号清单读到了没有；没读到 ⇒ 说「读不到清单」，选择只给「不指定账号」（说不清当前号是谁）。
 * @param alternative `string` = 当前账号的名字；`null` = 「不指定账号」。
 * @param choose 用户点了那个选择 ⇒ 以**显式**选号再起一次（`null` = 不指定账号）。
 */
export function refuseUnavailableAccount(r: {
  machine: string;
  name: string;
  pinned: boolean;
  listKnown: boolean;
  alternative: string | null;
  choose: (alternative: string | null) => void | Promise<unknown>;
}): void {
  const machine = isLocalOrigin(r.machine) ? copyText("accountPick.machine.local") : r.machine;
  const body = !r.listKnown
    ? copyText("accountPick.refused.listUnknown", { machine, name: r.name })
    : r.pinned
        ? r.alternative === null
          ? copyText("accountPick.refused.pinGoneToBase", { name: r.name })
          : copyText("accountPick.refused.pinGoneToCurrent", { name: r.name, current: r.alternative })
        : r.alternative === null
          ? copyText("accountPick.refused.explicitGoneToBase", { name: r.name })
          : copyText("accountPick.refused.explicitGoneToCurrent", { name: r.name, current: r.alternative });
  const alt = r.listKnown ? r.alternative : null;
  showActionFailureToast(copyText("accountPick.refused.title"), body, {
    level: "error",
    durationMs: 15000,
    onClick: () => void r.choose(alt),
  });
}

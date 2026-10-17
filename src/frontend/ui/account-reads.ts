/**
 * **账号域的读面**：经通道问那台机器（本机也一样）的后端。界面不存一份：每一次要用都问那台，推来「变了」也是重问。
 *
 * 两条帧命令：账号清单 `accounts-list`（每号带 API key 那一枚徽章）· 信任预检 `accounts-trust`。会话跑在哪个号下由那台在会话的脸上说（`face.account`）。全程走 A2 的 `available:false` 降级：未迁移 / 旧后端一律安静隐藏账号 UI，不报错。
 *
 * 入参 · 回包按生成类型问（`callTyped`，形状由握手指纹管）；规则留在 `accounts.ts`（纯），这里只管「去问、收」。
 */
import { appStore, putIn } from "./app-store";
import type { Origin } from "./ipc/origin";
import { budgetWithin, callTyped, isOldBackend } from "./ipc/chan-caller";
import { DEFAULT_AGENT, lookupAgentProfile, type AgentProfileRow } from "./agent-profile";
import { AGENT_PROFILE_TABLE } from "./generated/agent-profile-table";
import type { AccountsState } from "./accounts";
import { recallSeen, rememberSeen } from "./last-seen";
import { saidOf } from "./kit/detail";

/**
 * 问那台机器（**本机也一样**）的账号状态：每次都发 `accounts-list`，答回来那一份放进 store（画的那几处订它）。
 *
 * **走通道，后端出成品**：帧命令直接问那台机器的后端（本机那台由 `<local>` 那条长连接答），应答就是界面要的形状。
 *
 * `agent`：这次起会话的是哪一家（适配器 id）。并表只认那一家的行，后端不猜 ⇒ 由这里带过去。
 * 失败（没有控制通道 / 后端不认 / 对端说不行 / 期限到 / 形状对不上）⇒ `available:false` ＋ 一句人话（通道那一层的说法
 * 住 `ipc/chan-caller.ts::saidFrom`，不在账号面再写一份）＋ 本机后端记着的那台上次那一份（`last`，只给「画上次的」用）；
 * 「不可用」不是错误（前端据此降级，不弹错）。
 */
export async function fetchAccounts(origin: Origin): Promise<AccountsState> {
  let state: AccountsState;
  try {
    const got = await callTyped(origin, "accounts-list", { agent: launchAgentId() }, budgetWithin(ACCOUNTS_BUDGET_MS));
    // `notice`：后端说的「能用但有缺」（manifest 里没有账号 0 等）。
    state = { origin, available: true, error: null, oldBackend: false, meta: got.meta, accounts: got.accounts, notice: got.notice };
    void rememberSeen(origin, "accounts", got);
  } catch (e) {
    state = {
      origin,
      available: false,
      error: saidOf(e),
      oldBackend: isOldBackend(e),
      meta: null,
      accounts: [],
      notice: null,
      last: await recallLast(origin),
    };
  }
  putIn(appStore.accounts, origin, state); // 画的那几处订它
  return state;
}

/** 本机后端记着的那台上次那一份（没有 / 存档不是这一版 ⇒ `null`；本机那台不记）。 */
async function recallLast(origin: Origin): Promise<AccountsState["last"]> {
  const seen = await recallSeen(origin, "accounts");
  if (seen === null) return null;
  if (seen.state === "unreadable") {
    console.warn(`[accounts] ${origin} 记下的上次值不是这一版，读不懂`);
    return null;
  }
  return { meta: seen.value.meta, accounts: seen.value.accounts, atMs: seen.atMs };
}

/**
 * 账号清单那一问的期限：30 秒 —— 与它上一个住址（monitor 侧 `frame_query` 的 `LINES_BUDGET`）同值：
 * 盖的是「那台后端读一份 manifest ＋ stat 各账号目录 ＋ 读一份 apikey 表 ＋ 回程」，不含握手（长连接早就连着）。
 */
const ACCOUNTS_BUDGET_MS = 30_000;
/** 信任预检那一问的期限：30 秒 —— 与它上一个住址（逐次拨号那条的 `LIST_TIMEOUT`）同值。 */
const TRUST_BUDGET_MS = 30_000;

/**
 * 账号库那一家（线上的 kind，每条命令收的 `agent` 都是它）：新会话默认起的那一家 —— 账号 chip · 设置页 · 「新会话默认」说的都是它。
 * **值从后端来**：生成物里的 `DEFAULT_AGENT`（注册表里声明默认的那一家）。表里没有 ⇒ 抛（不回落到任何一家）。
 */
export function launchAgentId(): string {
  const got = lookupAgentProfile(DEFAULT_AGENT);
  if (!got.known) throw new Error(got.message);
  return got.facts.agent;
}

/** 账号页那一家的画像（与 [`launchAgentId`] 同一个出处）：叫法 · 认得的模型。查不到 ⇒ `null`。 */
export function accountsAgentProfile(): AgentProfileRow | null {
  let id: string;
  try {
    id = launchAgentId();
  } catch {
    return null;
  }
  return AGENT_PROFILE_TABLE.find((r) => r.agent === id) ?? null;
}

/** 换号前的目录信任预检（A5 会用；A3 先提供）。available:false → 视为"未知"，调用方只警告不拦。 */
export interface TrustResult {
  available: boolean;
  trusted: boolean;
  known: boolean;
  error: string | null;
}
/**
 * Z01：`configDir` 传 `null` = 问账号 0（它的 `.claude.json` 在那台机器的 `$HOME`）。**绝不传空串**——那会被后端判成不安全路径拒掉。
 *
 * **走通道**，本机与远端同一条路。失败一律 `available:false` ＋ 一句人话（调用方按「未知信任状态」只警告不拦）。
 */
export async function checkTrust(origin: Origin, configDir: string | null, cwd: string): Promise<TrustResult> {
  try {
    const got = await callTyped(origin, "accounts-trust", { configDir, cwd }, budgetWithin(TRUST_BUDGET_MS));
    return { available: true, error: null, trusted: got.trusted, known: got.known };
  } catch (e) {
    return { available: false, trusted: false, known: false, error: saidOf(e) };
  }
}

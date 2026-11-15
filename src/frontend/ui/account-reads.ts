/**
 * **账号域的读面**：经通道问那台机器（本机也一样）的后端。界面不存一份：每一次要用都问那台，推来「变了」也是重问。
 *
 * 帧命令：账号清单 `accounts-list`（每号带 API key 那一枚徽章）。会话跑在哪个号下由那台在会话的脸上说（`face.account`）。全程走 A2 的 `available:false` 降级：未迁移 / 旧后端一律安静隐藏账号 UI，不报错。
 *
 * 入参 · 回包按生成类型问（`callTyped`，形状由握手指纹管）；规则留在 `accounts.ts`（纯），这里只管「去问、收」。
 */
import { appStore, putIn } from "./app-store";
import type { Machine } from "./ipc/origin";
import { budgetWithin, callTyped, isOldBackend } from "./ipc/chan-caller";
import { DEFAULT_AGENT, lookupAgentProfile, type AgentProfileRow } from "./agent-profile";
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
export async function fetchAccounts(origin: Machine): Promise<AccountsState> {
  let state: AccountsState;
  try {
    const got = await askList(origin, launchAgentId());
    state = answered(origin, got);
    void rememberSeen(origin, "accounts", got);
  } catch (e) {
    state = { ...unanswered(origin, e), last: await recallLast(origin) };
  }
  putIn(appStore.accounts, origin, state); // 画的那几处订它
  return state;
}

/**
 * 问那台 `agent` 那一家的账号清单（新会话框照框里选的那一家问）：形状与降级同 [`fetchAccounts`]，
 * **不入 store、不记上次那一份**（那两样是账号库那一家的：账号 chip · 设置页订它）。
 */
export async function fetchAccountsOf(origin: Machine, agent: string): Promise<AccountsState> {
  try {
    return answered(origin, await askList(origin, agent));
  } catch (e) {
    return unanswered(origin, e);
  }
}

function askList(origin: Machine, agent: string) {
  return callTyped(origin, "accounts-list", { agent }, budgetWithin(ACCOUNTS_BUDGET_MS));
}

// `notice`：后端说的「能用但有缺」（manifest 里没有账号 0 等）。
function answered(origin: Machine, got: Awaited<ReturnType<typeof askList>>): AccountsState {
  return { origin, available: true, error: null, oldBackend: false, meta: got.meta, accounts: got.accounts, notice: got.notice };
}

function unanswered(origin: Machine, e: unknown): AccountsState {
  return { origin, available: false, error: saidOf(e), oldBackend: isOldBackend(e), meta: null, accounts: [], notice: null };
}

/** 本机后端记着的那台上次那一份（没有 / 存档不是这一版 ⇒ `null`；本机那台不记）。 */
async function recallLast(origin: Machine): Promise<AccountsState["last"]> {
  const seen = await recallSeen(origin, "accounts");
  if (seen === null) return null;
  if (seen.state === "unreadable") {
    console.warn(`[accounts] ${origin} 记下的上次值不是这一版，读不懂`);
    return null;
  }
  return { meta: seen.value.meta, accounts: seen.value.accounts, seen: seen.seen };
}

/**
 * 账号清单那一问的期限：30 秒 —— 与它上一个住址（monitor 侧 `frame_query` 的 `LINES_BUDGET`）同值：
 * 盖的是「那台后端读一份 manifest ＋ stat 各账号目录 ＋ 读一份 apikey 表 ＋ 回程」，不含握手（长连接早就连着）。
 */
const ACCOUNTS_BUDGET_MS = 30_000;

/**
 * 账号库那一家（线上的 kind，每条命令收的 `agent` 都是它）：新会话默认起的那一家 —— 账号 chip · 设置页 · 「新会话默认」说的都是它。
 * **值从后端来**：生成物里的 `DEFAULT_AGENT`（注册表里声明默认的那一家）。
 */
export function launchAgentId(): string {
  return accountsAgentProfile().agent;
}

/** 账号页那一家的画像（与 [`launchAgentId`] 同一个出处）：是哪一家 · 认得的模型（叫法照 `displayNameOf` 取）。表里没有 ⇒ 抛（不回落到任何一家）。 */
export function accountsAgentProfile(): AgentProfileRow {
  const got = lookupAgentProfile(DEFAULT_AGENT);
  if (!got.known) throw new Error(got.message);
  return got.facts;
}

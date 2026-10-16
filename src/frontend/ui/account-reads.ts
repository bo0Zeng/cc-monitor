/**
 * **账号域的读面**：经通道问那台机器（本机也一样）的后端。界面不存一份：每一次要用都问那台，推来「变了」也是重问。
 *
 * 两条帧命令：账号清单 `accounts-list`（每号带 API key 那一枚徽章）· 信任预检 `accounts-trust`。会话跑在哪个号下由那台在会话宣告里说。全程走 A2 的 `available:false` 降级：未迁移 / 旧后端一律安静隐藏账号 UI，不报错。
 *
 * 形状与规则留在 `accounts.ts`（纯），这里只管「去问、收」。
 */
import { appStore, putIn } from "./app-store";
import type { Origin } from "./ipc/origin";
import { chan } from "../../comms/inward/chan";
import { budgetWithin, isOldBackend, jsonBody, readJson, saidFrom } from "./ipc/chan-caller";
import { DEFAULT_AGENT, lookupAgentProfile, type AgentProfileRow } from "./agent-profile";
import { AGENT_PROFILE_TABLE } from "./generated/agent-profile-table";
import { decodeAccountsList, decodeTrust } from "./accounts-decode";
import type { AccountsState } from "./accounts";
import { recallSeen, rememberSeen } from "./last-seen";

/**
 * 问那台机器（**本机也一样**）的账号状态：每次都发 `accounts-list`，答回来那一份放进 store（画的那几处订它）。
 *
 * **走通道，后端出成品**：`chan.call(origin, "accounts-list", {agent}, …)` —— 帧命令直接问那台机器的后端
 * （本机那台由 `<local>` 那条长连接答），应答就是界面要的形状（[`decodeAccountsList`] 按形状收，不解释）。
 *
 * `agent`：这次起会话的是哪一家（适配器 id）。并表只认那一家的行，后端不猜 ⇒ 由这里带过去。
 * 失败（没有控制通道 / 后端不认 / 对端说不行 / 期限到 / 形状对不上）⇒ `available:false` ＋ 一句人话（通道那一层的说法
 * 住 `ipc/chan-caller.ts::saidFrom`，不在账号面再写一份）＋ 本机后端记着的那台上次那一份（`last`，只给「画上次的」用）；
 * 「不可用」不是错误（前端据此降级，不弹错）。
 */
export async function fetchAccounts(origin: Origin): Promise<AccountsState> {
  let state: AccountsState;
  try {
    const body = jsonBody({ agent: launchAgentId() });
    const budget = budgetWithin(ACCOUNTS_BUDGET_MS);
    const reply = await chan.call(origin, "accounts-list", body, budget);
    const raw = readJson(reply);
    const got = decodeAccountsList(raw);
    state = {
      origin,
      available: true,
      error: null,
      oldBackend: false,
      meta: got.meta,
      accounts: got.accounts,
      // Z01：后端说的「能用但有缺」（manifest 里没有账号 0）。
      notice: got.notice,
    };
    if (got.meta !== null) void rememberSeen(origin, "accounts", raw);
  } catch (e) {
    state = {
      origin,
      available: false,
      error: saidFrom(e, origin),
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

/** 本机后端记着的那台上次那一份（收不下 / 没有 ⇒ `null`；本机那台不记）。 */
async function recallLast(origin: Origin): Promise<AccountsState["last"]> {
  const seen = await recallSeen(origin, "accounts");
  if (seen === null) return null;
  try {
    const got = decodeAccountsList(seen.value);
    return got.meta === null ? null : { meta: got.meta, accounts: got.accounts, atMs: seen.atMs };
  } catch {
    // 记着的那份形状对不上（旧版记的）⇒ 当没有。
    return null;
  }
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
 * **走通道**：`chan.call(origin, "accounts-trust", {configDir, cwd}, …)`，本机与远端同一条路。
 * 在此之前是 Tauri 命令 `check_account_trust`：远端每问一次经本机后端开一条链路、在那台 exec 一次后端（最后两条仍逐次拨号的
 * 子命令），本机每问一次 exec 一次性本机后端。失败一律 `available:false` ＋ 一句人话（调用方按「未知信任状态」只警告不拦）。
 */
export async function checkTrust(
  origin: Origin,
  configDir: string | null,
  cwd: string,
): Promise<TrustResult> {
  try {
    const body = jsonBody({ configDir, cwd });
    const budget = budgetWithin(TRUST_BUDGET_MS);
    const reply = await chan.call(origin, "accounts-trust", body, budget);
    return { available: true, error: null, ...decodeTrust(readJson(reply)) };
  } catch (e) {
    return {
      available: false,
      trusted: false,
      known: false,
      error: saidFrom(e, origin),
    };
  }
}

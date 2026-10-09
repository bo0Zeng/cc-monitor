/**
 * API key 那一问**走通道，后端出成品**：
 *
 * | 问什么 | 帧命令 | 成品 |
 * |---|---|---|
 * | 这几个号在这台的表里有没有行 · 这台的中转在不在（`apikey.routing`） | `apikey-routing` | `{routed, running}` |
 *
 * 凭据文件的状态那一问（`apikey-read`）界面不问（账号页只要上面那两格），解码口随之删了；那条帧命令的形状由后端金样钉着。
 *
 * 界面经 `chan.call` 直接问那台机器的后端（本机那台由 `<local>` 那条长连接答）、按形状严格收 —— 多一格 / 缺一格 / 类型不对 ⇒
 * 抛「两端契约对不上」，不替后端补值。读凭据文件、探回环口都在后端（`accounts/upstream_select/endpoint.rs` · `file_face.rs`）。跨语言金样 `tests/__fixtures__/apikey.golden.json` 钉着后端出的形状与这里收的形状。
 *
 * 写 key（`creds.apikey` 写）也走通道：`apikey-key-set`（[`writeApikeyKey`]）。「本机后端写的那份 == 这个 monitor 用的那份」
 * 由连接本身保证：常驻后端的身份带着数据目录（`local_backend_host.rs::hello_verdict` 比 hello 的 `host_env`），
 * 替别的数据目录干活的后端接不上，`<local>` 那条长连接就不存在。
 * ⚠ 账号 id 只由后端推：前端交 `configDir`，一个字都不从它推账号 id。
 */
import { chan } from "../../comms/inward/chan";
import { budgetWithin, jsonBody, readJson, ReplyUnreadable, saidFrom } from "./ipc/chan-caller";
import type { Origin } from "./ipc/origin";
import { exactKeys, isObj } from "./ipc/decode";

/**
 * 账号页那两格事实。**射程别读宽**：`routed` 说的是「这台的表里有这一行」（不是「那把 key 能用」）；
 * `running` 说的是「这台机器上我们的中转在听」。
 */
export interface ApikeyRoutingView {
  /** 传进去的那些 configDir 里，表里**有对应行**的那几个（原样回）。 */
  routed: string[];
  /** 那台机器上我们的中转在不在听。 */
  running: boolean;
}

const nullableStr = (v: unknown): v is string | null =>
  v === null || typeof v === "string";

/** `apikey-routing` 的成品 ⇒ 两格事实。严格收。 */
export function decodeApikeyRouting(v: unknown): ApikeyRoutingView {
  if (
    !isObj(v) ||
    !exactKeys(v, ["routed", "running"]) ||
    !Array.isArray(v.routed) ||
    !v.routed.every((d) => typeof d === "string") ||
    typeof v.running !== "boolean"
  ) {
    throw new ReplyUnreadable("apikeyReads reply shape");
  }
  return { routed: v.routed as string[], running: v.running };
}

/**
 * 两问的期限：10 秒：盖的是「那台后端读一份凭据文件
 * （＋ 装一次表、在回环上探一次中转）＋ 回程」，不含握手（长连接早就连着）。
 */
const APIKEY_BUDGET_MS = 10_000;

/** 这几个号在那台的表里有没有行 · 那台的中转在不在。`agent`：这一家的路由名（凭据文件的行只属于一家，后端不猜）。 */
export async function fetchApikeyRouting(
  origin: Origin,
  agent: string,
  configDirs: string[],
): Promise<ApikeyRoutingView> {
  try {
    const body = jsonBody({ agent, configDirs });
    const budget = budgetWithin(APIKEY_BUDGET_MS);
    const reply = await chan.call(origin, "apikey-routing", body, budget);
    return decodeApikeyRouting(readJson(reply));
  } catch (e) {
    throw new Error(saidFrom(e, origin));
  }
}

/** `apikey-key-set` 的成品：推出来的账号 id · 那份文件在哪 · 写完读回的掩码与端点（**只有掩码**，`KS6`）。 */
export interface ApikeyWritten {
  account: string;
  path: string;
  masked: string;
  baseUrl: string | null;
}

/** `apikey-key-set` 的成品 ⇒ 写完读回的那一行。严格收。 */
export function decodeApikeyWritten(v: unknown): ApikeyWritten {
  if (
    !isObj(v) ||
    !exactKeys(v, ["account", "path", "masked", "baseUrl"]) ||
    typeof v.account !== "string" ||
    typeof v.path !== "string" ||
    typeof v.masked !== "string" ||
    !nullableStr(v.baseUrl)
  ) {
    throw new ReplyUnreadable("apikeyReads reply shape");
  }
  return { account: v.account, path: v.path, masked: v.masked, baseUrl: v.baseUrl };
}

/**
 * 给 `configDir` 那个号配一把 key（与可选的 Base URL），交**那台机器的后端**写（本机 ＝ `<local>` 那条长连接）。
 * 明文只在这一发的请求体里（`key`）：不进任何返回值、不进报错文案（报错只说那台后端回的话）。
 * `baseUrl` 缺席 = 不碰那一格。
 */
export async function writeApikeyKey(
  origin: Origin,
  configDir: string,
  key: string,
  baseUrl?: string,
): Promise<ApikeyWritten> {
  try {
    const body = jsonBody(baseUrl === undefined ? { configDir, key } : { configDir, key, baseUrl });
    const budget = budgetWithin(APIKEY_BUDGET_MS);
    const reply = await chan.call(origin, "apikey-key-set", body, budget);
    return decodeApikeyWritten(readJson(reply));
  } catch (e) {
    throw new Error(saidFrom(e, origin));
  }
}

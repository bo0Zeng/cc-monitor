/**
 * 〔US1 · 第四波 4D〕API key 那两问**走通道，后端出成品**（`设计/05 §14.3` B 组）：
 *
 * | 问什么 | 帧命令 | 成品 |
 * |---|---|---|
 * | 这台机器上那份凭据文件的状态（`creds.apikey` 读） | `apikey-read` | `{configured, masked, path, notice, problem}` |
 * | 这几个号在这台的表里有没有行 · 这台的中转在不在（`apikey.routing`） | `apikey-routing` | `{routed, running}` |
 *
 * 界面经 `chan.call` 直接问那台机器的后端（本机那台由 `<local>` 那条长连接答）、按形状严格收 —— 多一格 / 缺一格 / 类型不对 ⇒
 * 抛「两端契约对不上」，不替后端补值。先前两条 Tauri 命令（`read_apikey_credentials_status` · `apikey_routing_for`）在 monitor 里
 * 本机自己读凭据文件、自己连回环口，远端转一条帧命令 —— 那一份解释与人群搬进了后端（`accounts/upstream/endpoint.rs`
 * · `file_face.rs`），两条命令退役。跨语言金样 `tests/__fixtures__/apikey.golden.json` 钉着后端出的形状与这里收的形状。
 *
 * 〔HX2 · 第四波 4D〕写 key（`creds.apikey` 写）也走通道：`apikey-key-set`（[`writeApikeyKey`]）。从前它留在 Tauri 命令
 * `write_apikey_credentials_key`〔散文墓碑〕里，因为本机那一臂写之前要核「本机后端写的那份 == 这个 monitor 用的那份」（GP1）——
 * 常驻后端的身份带上数据目录之后（`local_backend_host.rs::hello_verdict` 比 hello 的 `host_env`），那一问由**连接本身**答：
 * 接不上一个替别的数据目录干活的后端，`<local>` 那条长连接就不存在。
 * ⚠ 账号 id 只由后端推：前端交 `configDir`，一个字都不从它推账号 id（`KH2C1`）。
 */
import { chan } from "./ipc/chan";
import { budgetWithin, jsonBody, readJson, saidOf } from "./ipc/chan-caller";
import type { Origin } from "./ipc/origin";
import { copyText } from "./copy-table";

/**
 * 那份凭据文件的状态（**只有掩码**，前端永远拿不到明文：`KS6`）。
 * `configured` / `masked` 说的是顶层那一把（`KH2C3`）；`notice` = 权限过宽 / 查不出来时的一句话；
 * `problem` = 读不动 / 解析不了时的一句话（🔴 解析不了不退化成「没配」）。
 */
export interface ApikeyCredentialsStatus {
  configured: boolean;
  masked: string;
  path: string;
  notice: string | null;
  problem: string | null;
}

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

const isObj = (v: unknown): v is Record<string, unknown> =>
  v !== null && typeof v === "object" && !Array.isArray(v);
const sameKeys = (
  o: Record<string, unknown>,
  want: readonly string[],
): boolean => {
  const got = Object.keys(o).sort();
  const w = [...want].sort();
  return got.length === w.length && got.every((k, i) => k === w[i]);
};
const nullableStr = (v: unknown): v is string | null =>
  v === null || typeof v === "string";

/** `apikey-read` 的成品 ⇒ 状态。严格收。 */
export function decodeApikeyStatus(v: unknown): ApikeyCredentialsStatus {
  if (
    !isObj(v) ||
    !sameKeys(v, ["configured", "masked", "path", "notice", "problem"]) ||
    typeof v.configured !== "boolean" ||
    typeof v.masked !== "string" ||
    typeof v.path !== "string" ||
    !nullableStr(v.notice) ||
    !nullableStr(v.problem)
  ) {
    throw new Error(copyText("apikeyReads.status.badShape"));
  }
  return {
    configured: v.configured,
    masked: v.masked,
    path: v.path,
    notice: v.notice,
    problem: v.problem,
  };
}

/** `apikey-routing` 的成品 ⇒ 两格事实。严格收。 */
export function decodeApikeyRouting(v: unknown): ApikeyRoutingView {
  if (
    !isObj(v) ||
    !sameKeys(v, ["routed", "running"]) ||
    !Array.isArray(v.routed) ||
    !v.routed.every((d) => typeof d === "string") ||
    typeof v.running !== "boolean"
  ) {
    throw new Error(copyText("apikeyReads.routing.badShape"));
  }
  return { routed: v.routed as string[], running: v.running };
}

/**
 * 两问的期限：10 秒 —— 与它们上一个住址（monitor `apikey_remote::BUDGET`〔散文墓碑〕）同值：盖的是「那台后端读一份凭据文件
 * （＋ 装一次表、在回环上探一次中转）＋ 回程」，不含握手（长连接早就连着）。
 */
const APIKEY_BUDGET_MS = 10_000;

/** 那台后端比这两问老（不认这条命令）时的那句话。 */
const APIKEY_OLD_BACKEND = copyText("apikeyReads.backend.tooOld");

/** 那台机器上那份凭据文件的状态。问不到 / 形状不对 ⇒ 抛一句人话（不退化成「没配」）。 */
export async function readApikeyStatus(
  origin: Origin,
): Promise<ApikeyCredentialsStatus> {
  try {
    const body = jsonBody({});
    const budget = budgetWithin(APIKEY_BUDGET_MS);
    const reply = await chan.call(origin, "apikey-read", body, budget);
    return decodeApikeyStatus(readJson(reply));
  } catch (e) {
    throw new Error(saidOf(e, APIKEY_OLD_BACKEND));
  }
}

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
    throw new Error(saidOf(e, APIKEY_OLD_BACKEND));
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
    !sameKeys(v, ["account", "path", "masked", "baseUrl"]) ||
    typeof v.account !== "string" ||
    typeof v.path !== "string" ||
    typeof v.masked !== "string" ||
    !nullableStr(v.baseUrl)
  ) {
    throw new Error(copyText("apikeyReads.write.badShape"));
  }
  return { account: v.account, path: v.path, masked: v.masked, baseUrl: v.baseUrl };
}

/**
 * 〔HX2 · 4D〕给 `configDir` 那个号配一把 key（与可选的 Base URL），交**那台机器的后端**写（本机 ＝ `<local>` 那条长连接）。
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
    throw new Error(saidOf(e, APIKEY_OLD_BACKEND));
  }
}

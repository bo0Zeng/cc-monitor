/**
 * **账号偏好**：本机 config.json 里 `accounts.byMachine.<机器>` 那一段 —— 「在那台机器上起新会话默认用哪个号」
 * （`defaultName`）与「那台机器上每个号默认用哪个模型」（`modelByAccount`）。按机器分：各台常用同一套号名，
 * 在一台上设的不许改到别台的同名号。存在本机、不跨机器同步。键是机器的 origin（本机是 `<local>`）。
 *
 * 写口是按键补丁（`config.ts::patchConfig`）：每个写者只交它自己那一格的路径。
 */
import { loadConfig, patchConfig, removeAt, setAt } from "./config";
import { modelNameOk } from "./generated/judgment-rules";
import { copyText } from "./copy-table";
import type { Origin } from "./ipc/origin";

const CFG_KEY = "accounts";
const BY_MACHINE = "byMachine";
const MODEL_MAP_KEY = "modelByAccount";

/** 读回那台机器那一段（没有 ⇒ `null`）。读不动 ⇒ 打一行、当没有。 */
async function machinePrefs(origin: Origin, what: string): Promise<Record<string, unknown> | null> {
  try {
    const cfg = (await loadConfig()) as Record<string, unknown>;
    const a = cfg[CFG_KEY];
    const by = a && typeof a === "object" ? (a as Record<string, unknown>)[BY_MACHINE] : undefined;
    const m = by && typeof by === "object" ? (by as Record<string, unknown>)[origin] : undefined;
    return m && typeof m === "object" ? (m as Record<string, unknown>) : null;
  } catch (e) {
    console.warn(`${what} failed:`, e);
    return null;
  }
}

/** 读那台机器的默认账号名。无则 null。 */
export async function getDefaultName(origin: Origin): Promise<string | null> {
  const dn = (await machinePrefs(origin, "getDefaultName"))?.defaultName;
  return typeof dn === "string" && dn ? dn : null;
}

/** 写那台机器的默认账号名。null = 清除（回退跟随那台的账号清单）。 */
export async function setDefaultName(origin: Origin, name: string | null): Promise<void> {
  const path = [CFG_KEY, BY_MACHINE, origin, "defaultName"] as const;
  await patchConfig([name === null ? removeAt(path) : setAt(path, name)]);
}

/** F07：读那台机器上某个号的默认模型偏好。无则 undefined。 */
export async function getModelForAccount(origin: Origin, name: string): Promise<string | undefined> {
  const map = (await machinePrefs(origin, "getModelForAccount"))?.[MODEL_MAP_KEY];
  const v = map && typeof map === "object" ? (map as Record<string, unknown>)[name] : undefined;
  return typeof v === "string" && v ? v : undefined;
}

/** 写那台机器上某个号的模型偏好。`model === null` 清除这一条（别的号、别的机器不受影响）。
 *
 *  校验在**写入点**做（fail-closed）：一个非法值落了盘，那个号此后每一次起会话都会在
 *  `buildLaunchPlan` 里失败。调用方（界面）负责 catch 并提示。 */
export async function setModelForAccount(origin: Origin, name: string, model: string | null): Promise<void> {
  // 规则读 monitor 从 `shell_quote_core::model_name_ok` 现生成的式子（`src/frontend/ui/generated/judgment-rules.ts`），两侧由共用金样逐条对。
  if (model && !modelNameOk(model)) {
    throw new Error(copyText("accounts.setModel.invalid", { model: JSON.stringify(model) }));
  }
  const path = [CFG_KEY, BY_MACHINE, origin, MODEL_MAP_KEY, name] as const;
  await patchConfig([model ? setAt(path, model) : removeAt(path)]);
}

/** 机器改名：那台的账号偏好跟着搬到新名字下（不搬的话改个名，默认号与默认模型就悄悄没了）。 */
export async function moveMachinePrefs(from: Origin, to: Origin): Promise<void> {
  if (from === to) return;
  const prefs = await machinePrefs(from, "moveMachinePrefs");
  if (prefs === null) return;
  await patchConfig([setAt([CFG_KEY, BY_MACHINE, to], prefs), removeAt([CFG_KEY, BY_MACHINE, from])]);
}

// ------------------------------------------------------------ 带缓存的取数

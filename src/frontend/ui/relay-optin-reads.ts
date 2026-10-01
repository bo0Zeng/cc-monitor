/**
 * 让直接敲的 claude 也走中转（可选）：问一台后端「那份用户级设置文件里写没写、对不对」＋ 要贴的那一段（帧命令 `relay-optin`，
 * 本机远端同一条 `chan.call(origin, …)`）。判定全在那台后端；界面只按形状收、照态画，不读、不写那份文件。
 *
 * 成品的线上形状由跨语言金样 `tests/__fixtures__/relay-optin.golden.json` 钉住（后端产出 == 金样 · {@link decodeRelayOptin} 读同一份）。
 */
import type { Origin } from "./ipc/origin";
import { chan } from "../../comms/inward/chan";
import { budgetWithin, jsonBody, readJson, saidOf } from "./ipc/chan-caller";
import { copyText } from "./copy-table";

/** 五态：已装 · 贴的过期了 · 没装 · 设了别的地址 · 读不了（不当没装）。 */
export type RelayOptinState = "installed" | "stale" | "absent" | "other" | "unreadable";
const STATES: readonly RelayOptinState[] = ["installed", "stale", "absent", "other", "unreadable"];

/** `note` = 那份文件为什么读不了；`missing` = 那一段为什么生成不了；`snippet` 已装 / 生成不了 ⇒ `null`。 */
export type RelayOptinReport = {
  state: RelayOptinState;
  note: string;
  missing: string;
  source: string;
  snippet: string | null;
  listening: boolean;
};

/** 读一份设置文件 ＋ 一份钥匙文件 ＋ 回程。 */
const RELAY_OPTIN_BUDGET_MS = 30_000;

/** `relay-optin` 的成品 → {@link RelayOptinReport}。按形状严格收：多一格 / 缺一格 / 类型不对 / 认不得的态 ⇒ 抛（两端契约对不上）。 */
export function decodeRelayOptin(v: unknown): RelayOptinReport {
  const bad = (what: string): never => {
    throw new Error(`relay-optin reply shape mismatch: ${what}`); // 程序员错误，刻意英文
  };
  if (v === null || typeof v !== "object" || Array.isArray(v)) return bad("reply is not an object");
  const o = v as Record<string, unknown>;
  const keys = ["listening", "missing", "note", "snippet", "source", "state"];
  if (Object.keys(o).sort().join(",") !== keys.join(",")) return bad(`reply has keys ${Object.keys(o).sort().join(",")}`);
  const str = (k: string): string => (typeof o[k] === "string" ? (o[k] as string) : bad(`${k} is not a string`));
  const state = STATES.find((s) => s === o.state) ?? bad(`state is ${JSON.stringify(o.state)}`);
  if (typeof o.listening !== "boolean") return bad("listening is not a boolean");
  return {
    state,
    note: str("note"),
    missing: str("missing"),
    source: str("source"),
    snippet: o.snippet === null ? null : str("snippet"),
    listening: o.listening,
  };
}

/** 问 `origin` 那台后端要一份成品（本机逐字 `LOCAL_ORIGIN`）。问不出来 ⇒ 抛（带那台的原话）。 */
export async function fetchRelayOptin(origin: Origin): Promise<RelayOptinReport> {
  try {
    const body = jsonBody({});
    const budget = budgetWithin(RELAY_OPTIN_BUDGET_MS);
    return decodeRelayOptin(readJson(await chan.call(origin, "relay-optin", body, budget)));
  } catch (e) {
    throw new Error(saidOf(e, copyText("relayOptin.fetch.oldBackend")));
  }
}

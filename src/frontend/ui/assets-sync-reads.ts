/**
 * 资产目录同步**走通道**：界面直问本机常驻后端 `assets-sync`（`<local>`），
 * 远端那一台只报 `origin` —— 怎么够到它由那台流握手那一刻的 `remote-reach` 登记过（可达表住本机后端）。
 * 从前 monitor 的 Tauri 命令 `assets_sync` 替界面拼拨号请求再转交；那条删了，这里按形状严格收
 * （金样 `tests/__fixtures__/assets-sync.golden.json`）。
 */
import { chan } from "../../comms/inward/chan";
import { budgetWithin, jsonBody, readJson, ReplyUnreadable, saidFrom } from "./ipc/chan-caller";
import { isLocalOrigin, LOCAL_ORIGIN, type Origin } from "./ipc/origin";
import { exactKeys, isObj } from "./ipc/decode";

/** 一趟同步的结局（`error` = 那一趟哪里没办成，`null` = 全办成了）。 */
export interface AssetsSyncRow {
  origin: string;
  peer: string | null;
  changed: boolean;
  pushed: number;
  error: string | null;
}

/** 可达表一行：`machine` = 那台目录的 id（还没拉成过 ⇒ `null`）。 */
export interface AssetsReach {
  origin: string;
  machine: string | null;
}

export interface AssetsSynced {
  self: string | null;
  synced: AssetsSyncRow[];
  reach: AssetsReach[];
}

const optStr = (v: unknown): v is string | null => v === null || typeof v === "string";
const bad = (): Error => new ReplyUnreadable("assetsSyncReads reply shape");

/** `assets-sync` 的成品。严格收。 */
export function decodeAssetsSynced(v: unknown): AssetsSynced {
  if (!isObj(v) || !exactKeys(v, ["self", "synced", "reach"]) || !optStr(v.self) || !Array.isArray(v.synced) || !Array.isArray(v.reach))
    throw bad();
  const synced = v.synced.map((r) => {
    if (
      !isObj(r) ||
      !exactKeys(r, ["origin", "peer", "changed", "pushed", "error"]) ||
      typeof r.origin !== "string" ||
      !optStr(r.peer) ||
      typeof r.changed !== "boolean" ||
      typeof r.pushed !== "number" ||
      !optStr(r.error)
    )
      throw bad();
    return { origin: r.origin, peer: r.peer, changed: r.changed, pushed: r.pushed, error: r.error };
  });
  const reach = v.reach.map((r) => {
    if (!isObj(r) || !exactKeys(r, ["origin", "machine"]) || typeof r.origin !== "string" || !optStr(r.machine)) throw bad();
    return { origin: r.origin, machine: r.machine };
  });
  return { self: v.self, synced, reach };
}

/** 一趟同步要拉 / 并 / 推，远端慢的时候分钟级；给 180 秒（与从前 monitor 那一跳同值）。 */
const SYNC_BUDGET_MS = 180_000;

/** 让本机常驻后端对 `origin` 那一台做一趟（本机那一页 ⇒ 对它够得到的每一台）。 */
export async function syncAssets(origin: Origin): Promise<AssetsSynced> {
  try {
    const body = jsonBody(isLocalOrigin(origin) ? {} : { origin });
    const budget = budgetWithin(SYNC_BUDGET_MS);
    return decodeAssetsSynced(readJson(await chan.call(LOCAL_ORIGIN, "assets-sync", body, budget)));
  } catch (e) {
    throw new Error(saidFrom(e, LOCAL_ORIGIN));
  }
}

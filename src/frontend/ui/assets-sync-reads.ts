/**
 * 资产目录同步**走通道**：界面直问本机常驻后端 `assets-sync`（`<local>`），
 * 远端那一台只报 `origin` —— 怎么够到它照本机后端的机器表（按名字组请求）。
 * monitor 不替界面拼拨号请求、不转交。
 * typed：入参 · 回包的类型从 Rust 生成，形状由握手指纹核过（金样 `tests/__fixtures__/assets-sync.golden.json` 由后端判据钉）。
 */
import { budgetWithin, callTyped } from "./ipc/chan-caller";
import { isLocalOrigin, LOCAL, type Machine } from "./ipc/origin";
import type { AssetsSynced } from "./generated/AssetsSynced";
import { saidOf } from "./kit/detail";

/** 一趟同步要拉 / 并 / 推，远端慢的时候分钟级；给 180 秒。 */
const SYNC_BUDGET_MS = 180_000;

/** 让本机常驻后端对 `origin` 那一台做一趟（本机那一页 ⇒ 对机器表里「连这台」的每一台）。 */
export async function syncAssets(origin: Machine): Promise<AssetsSynced> {
  try {
    const req = isLocalOrigin(origin) ? {} : { origin };
    return await callTyped(LOCAL, "assets-sync", req, budgetWithin(SYNC_BUDGET_MS));
  } catch (e) {
    throw new Error(saidOf(e));
  }
}

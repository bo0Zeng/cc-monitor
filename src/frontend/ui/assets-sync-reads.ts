/**
 * 资产目录同步**走通道**：界面直问本机常驻后端 `assets-sync`（`<local>`），
 * 远端那一台只报 `origin` —— 怎么够到它由那台流握手那一刻的 `remote-reach` 登记过（可达表住本机后端）。
 * 从前 monitor 的 Tauri 命令 `assets_sync` 替界面拼拨号请求再转交；那条删了。
 * typed：入参 · 回包的类型从 Rust 生成，形状由握手指纹核过（金样 `tests/__fixtures__/assets-sync.golden.json` 由后端判据钉）。
 */
import { budgetWithin, callTyped, saidFrom } from "./ipc/chan-caller";
import { isLocalOrigin, LOCAL_ORIGIN, type Origin } from "./ipc/origin";
import type { AssetsSynced } from "./generated/AssetsSynced";

/** 一趟同步要拉 / 并 / 推，远端慢的时候分钟级；给 180 秒（与从前 monitor 那一跳同值）。 */
const SYNC_BUDGET_MS = 180_000;

/** 让本机常驻后端对 `origin` 那一台做一趟（本机那一页 ⇒ 对它够得到的每一台）。 */
export async function syncAssets(origin: Origin): Promise<AssetsSynced> {
  try {
    const req = isLocalOrigin(origin) ? {} : { origin };
    return await callTyped(LOCAL_ORIGIN, "assets-sync", req, budgetWithin(SYNC_BUDGET_MS));
  } catch (e) {
    throw new Error(saidFrom(e, LOCAL_ORIGIN));
  }
}

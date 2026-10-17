/**
 * API key 那一问**走通道，后端出成品**：
 *
 * | 问什么 | 帧命令 | 成品 |
 * |---|---|---|
 * | 写 key（`creds.apikey`） | `apikey-key-set` | `{account, path, masked, baseUrl}` |
 *
 * 「这几个号在这台表里有没有行 · 这台的中转在不在」两格并进了账号清单那一枚徽章（`accounts-list` 每号 `badge`，那台后端判），
 * 界面不单问这两样。
 *
 * 界面经 `callTyped` 直接问那台机器的后端（本机那台由 `<local>` 那条长连接答），入参 · 回包按生成类型用、形状由握手指纹管。
 * 读凭据文件、探回环口都在后端（`accounts/upstream_select/endpoint.rs` · `file_face.rs`）。
 *
 * 写 key（`creds.apikey` 写）也走通道：`apikey-key-set`（[`writeApikeyKey`]）。「本机后端写的那份 == 这个 monitor 用的那份」
 * 由连接本身保证：常驻后端的身份带着数据目录（`local_backend_host.rs::hello_verdict` 比 hello 的 `host_env`），
 * 替别的数据目录干活的后端接不上，`<local>` 那条长连接就不存在。
 * ⚠ 账号 id 只由后端推：前端交 `configDir`，一个字都不从它推账号 id。
 */
import { budgetWithin, callTyped, saidFrom } from "./ipc/chan-caller";
import type { Origin } from "./ipc/origin";
import type { KeySet } from "./generated/KeySet";

/** 那一问的期限：10 秒：盖的是「那台后端写一份凭据文件再读回 ＋ 回程」，不含握手（长连接早就连着）。 */
const APIKEY_BUDGET_MS = 10_000;

/**
 * 给 `configDir` 那个号配一把 key（与可选的 Base URL），交**那台机器的后端**写（本机 ＝ `<local>` 那条长连接）；
 * 回推出来的账号 id · 那份文件在哪 · 写完读回的掩码与端点（**只有掩码**，`KS6`）。
 * 明文只在这一发的请求体里（`key`）：不进任何返回值、不进报错文案（报错只说那台后端回的话）。
 * `baseUrl` 缺席 = 不碰那一格。
 */
export async function writeApikeyKey(
  origin: Origin,
  configDir: string,
  key: string,
  baseUrl?: string,
): Promise<KeySet> {
  try {
    const req = baseUrl === undefined ? { configDir, key } : { configDir, key, baseUrl };
    return await callTyped(origin, "apikey-key-set", req, budgetWithin(APIKEY_BUDGET_MS));
  } catch (e) {
    throw new Error(saidFrom(e, origin));
  }
}

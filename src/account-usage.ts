/**
 * F10（剩余账号 UX）：`account_usage` IPC 的前端包装 + 去抖缓存。
 *
 * 缓存**不是** TTL 轮询——只是"同一次设置窗口/chip 菜单打开期间，别对同一个账号重复戳"的
 * 去抖（探测是较重操作：起隐藏会话+网络查询，几秒到十几秒）。没有 `setInterval`，没有后台
 * 定时任务；`force:true`（用户点"刷新用量"）忽略缓存强制重查。
 */
import { commands } from "./ipc/commands";
import { buildUsageProbePayload } from "./remote-launch.ts";
import { parseUsageCapture, type AccountUsageParseResult } from "./account-usage-parse.ts";

export type AccountUsageOutcome =
  | { status: "probe-failed"; error: string }
  | AccountUsageParseResult;

/** F10 Phase D 审计（后端架构 + UX 均指出，重要）：`status: "ok"` 只代表"解析器认出了这个
 *  格式"，不代表格式假设本身已经过真机验证——已用%/剩余% 的方向、具体数值都可能整体猜反。
 *  跟 `unrecognized` 等诚实降级分支不是一回事，容易被误当"验证过的事实"。两处渲染 ok 状态的
 *  UI（accounts-section.ts 的用量单元格、account-chip.ts 的折叠态/菜单展示）共享这句 title
 *  提示，避免各写各的、日后措辞漂移。 */
export const OK_USAGE_UNVERIFIED_CAVEAT =
  "格式基于训练知识猜测，尚未经真机验证——百分比的含义（已用/剩余）与具体数值可能不准确。";

interface CacheEntry {
  ts: number;
  outcome: AccountUsageOutcome;
}

const cache = new Map<string, CacheEntry>();
const cacheKey = (origin: string, accountName: string): string => `${origin}|${accountName}`;

/**
 * per-account 探测 plan 用量窗口%。`force` 忽略缓存（用户点"刷新用量"时传）。
 *
 * Z03：`configDir` 传 **`null`** = 探**账号 0**（载荷前缀是 `unset CLAUDE_CONFIG_DIR; `）。
 * **别传空串**——那是坏数据，`buildUsageProbePayload` 会 throw（被下面 catch 成 probe-failed）。
 */
export async function fetchAccountUsage(
  origin: string,
  accountName: string,
  configDir: string | null,
  opts?: { force?: boolean },
): Promise<AccountUsageOutcome> {
  const key = cacheKey(origin, accountName);
  if (!opts?.force) {
    const cached = cache.get(key);
    if (cached) return cached.outcome;
  }

  let outcome: AccountUsageOutcome;
  try {
    const payload = buildUsageProbePayload(configDir);
    const result = await commands.account_usage({
      origin,
      accountName,
      launchPayload: payload,
    });
    outcome = result.captured
      ? parseUsageCapture(result.raw ?? "")
      : { status: "probe-failed", error: result.error ?? "探测失败（原因未知）" };
  } catch (e) {
    outcome = { status: "probe-failed", error: String(e) };
  }

  cache.set(key, { ts: Date.now(), outcome });
  return outcome;
}

/** 设置面板"刷新"（账号列表本身）不该连带清用量缓存——两者语义分开（F10 计划 §5.2）。
 *  这个函数只在用户明确要求"重查用量"时调（如换了账号的登录态之后）。 */
export function invalidateAccountUsageCache(origin?: string, accountName?: string): void {
  if (!origin) {
    cache.clear();
    return;
  }
  if (!accountName) {
    for (const k of [...cache.keys()]) {
      if (k.startsWith(`${origin}|`)) cache.delete(k);
    }
    return;
  }
  cache.delete(cacheKey(origin, accountName));
}

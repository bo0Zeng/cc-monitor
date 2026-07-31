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

/**
 * ok 状态旁那句提示。**S10 重写（2026-07-31）—— 原文已经过时且在说假话。**
 *
 * 原文写着「格式基于训练知识猜测，尚未经真机验证——百分比的含义（已用/剩余）与具体数值
 * 可能不准确」。那句话在 F10 时是诚实的，**但 E42 之后就不再成立**：用户提供了真机
 * `/usage` 抓屏，解析器据此**重写成结构性识别**并对照夹具
 * （`src/__fixtures__/usage-capture-2026-07-31.txt`）钉住。继续挂那句话，等于告诉用户
 * 「这个数字可能整体反了」——而它其实是对的，会让人白白不敢用。
 *
 * **但也不能改成「已验证，放心用」** —— 残余风险是真的，只是换了个位置：探针用
 * 「画面静止 3s」判定渲染完成，那是**预算不是实测值**（本仓不允许起真实已认证的 claude
 * 去测真实渲染耗时）。真 claude 若渲染中途卡顿超过 3s，会抓到半张屏。
 *
 * 关键在于**那种情况的表现是什么**：抓早了的屏解析不出来 → `unrecognized` → UI 把原始屏
 * 带回来 + 「复制诊断文本」。也就是**可见失败，不是静默错值**。这正是 F10 建立、
 * 主计划要求 S10 合并后必须保住的那条路径。
 */
// 常量名沿用 `..._UNVERIFIED_...`（三处 import 着）——**只改文案不改名**：
// 改名是纯改动面，而这条提示的语义变化已经写在上面的注释里。
export const OK_USAGE_UNVERIFIED_CAVEAT =
  "格式已对照真机 /usage 抓屏验证（2026-07-31）。残余风险：探针用「画面静止 3s」判定渲染完成，" +
  "那是预算不是实测值——真 claude 若渲染中途卡顿超过 3s 会抓早；那种情况会显示成「认不出格式」" +
  "并附原始屏，而不是给出一个错的数字。";

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

/**
 * F03（unify-launch）：ccm CLI 探测缓存——`renderLaunchCommand`（`remote-launch-run.ts`）据此
 * 决定某个 origin 走 CLI 渲染器还是兜底渲染器。
 *
 * **何时探测**：惰性、按 origin、首次用到才探——不新增任何轮询（守 MASTERPLAN §5.4/§0.1）。
 * **缓存多久**：5 分钟——够长（不会让"每点一次 resume 就多一次 ssh 往返"），够短（用户刚装/
 * 升级 ccm 之后，下一次操作基本感知得到，F08 的安装向导完成后应调 `invalidateCcmProbeCache`
 * 让下次启动立刻用上 CLI 渲染器，不必等 TTL）。
 *
 * # 🔴 `K-R53` `KR53D3`：**「探测出错」与「远端没装」是两件事**
 *
 * 上一版这里逐字是：
 *
 * ```text
 * } catch {
 *   // 探测失败（ssh 抖动/远端不可达等）→ 安全降级到兜底渲染器，不抛给调用方。
 *   value = NOT_INSTALLED;
 * }
 * ```
 *
 * 那句注释说的**处置**是对的（渲染那一侧走兜底本来就是对的），错的是它**把处置写进了值里**：
 * 一次 ssh 抖动之后，这个 origin 的探测结果**就是** `installed:false`，
 * 而且它还被 `probeCache.set` **缓存 5 分钟** ⇒ 接下来 5 分钟里，
 * 「这台机器上没有 ccm」这句**它并不知道的话**会被当成已知事实反复使用。
 *
 * ★ 这与本区那条最贵的病同形：**一个值装了两件事**。
 *
 * ⇒ 现在是**判别联合**三态（同 `session-backend.ts::TmuxTarget` 那条「判别式入参取代嗅探」
 * 与 R05 把 `"__base__"` 换成判别联合的理由：分不清楚这件事要么在类型里，要么迟早靠人记着）：
 *
 * | `state` | 含义 | 进不进缓存 |
 * |---|---|---|
 * | `"installed"` | 探到了，带版本与能力集 | 进（5 分钟 TTL） |
 * | `"not-installed"` | 探到了，**它真的没装** | 进（这是一个真答案） |
 * | `"unknown"` | **没探出来**（IPC/ssh 出错），`error` 是唯一线索 | **不进** —— 下一次会重探 |
 *
 * **降级方向没变**：拿不到 `installed` 一律走兜底渲染器（`remote-launch-run.ts`），
 * 探测是**可用性判断**、不是安全边界，fail-open 到被充分验证过的兜底渲染器仍是正确方向
 *（区别于 configDir 能不能拼进命令这类必须 fail-closed 的安全校验 —— 那一道住 `payload.rs::config_dir_command_safe`）。
 * 本次改的是**值**，不是处置 —— 别把这段读成「出错就不降级了」。
 */
import { chan, ChanError } from "./ipc/chan";
import { budgetWithin, jsonBody, readJson } from "./ipc/chan-caller";
import type { Origin } from "./ipc/origin";

/**
 * 一次探测的结果。**三态判别联合**，理由见本文件头注。
 *
 * ⚠ 读它的人**不许写 `!probe.installed`** —— 那个写法正是被治掉的那一形
 *（它把 `"unknown"` 和 `"not-installed"` 读成同一件事）。要判「能不能用 CLI 渲染器」
 * 就判 `state === "installed"`：那是**肯定式**的问法，「不知道」自然落在外面而不被改写成「没有」。
 */
export type CcmProbeResult =
  | { state: "installed"; version: string | null; capabilities: Set<string> }
  | { state: "not-installed" }
  | { state: "unknown"; error: string };

// 〔MIG-3b · `99 §2.1 ⑬`〕线上形状是那台后端 `ccm-probe` 的成品（monitor 那一跳 · 那份原文解析删了），
// 由金样 `tests/__fixtures__/ccm-probe.golden.json` 钉住（后端测试核键集 · {@link decodeCcmProbe} 读同一份）。

/** 一问的期限：纯函数应答 ＋ 回程。 */
const CCM_PROBE_BUDGET_MS = 30_000;

/** `ccm-probe` 的成品 → 「装了」那一态。按形状严格收：多一格 / 缺一格 / 类型不对 ⇒ 抛（两端契约对不上 ⇒ 调用方落「不知道」）。 */
export function decodeCcmProbe(v: unknown): CcmProbeResult {
  const bad = (what: string): never => {
    throw new Error(`ccm-probe reply shape mismatch: ${what}`);
  };
  if (v === null || typeof v !== "object" || Array.isArray(v)) return bad("not an object");
  const o = v as Record<string, unknown>;
  const keys = Object.keys(o).sort().join(",");
  if (keys !== "agents,build,capabilities,version") return bad(`keys ${keys}`);
  const strs = (x: unknown): x is string[] => Array.isArray(x) && x.every((s) => typeof s === "string");
  if (typeof o.version !== "string" || typeof o.build !== "string" || !strs(o.capabilities) || !strs(o.agents)) {
    return bad("a field has the wrong type");
  }
  return { state: "installed", version: o.version, capabilities: new Set(o.capabilities) };
}

const CCM_PROBE_TTL_MS = 5 * 60_000;
const NOT_INSTALLED: CcmProbeResult = { state: "not-installed" };
const probeCache = new Map<string, { at: number; value: CcmProbeResult }>();

export async function probeCcm(origin: string, force = false): Promise<CcmProbeResult> {
  const now = Date.now();
  const cached = probeCache.get(origin);
  if (!force && cached && now - cached.at < CCM_PROBE_TTL_MS) return cached.value;
  let value: CcmProbeResult;
  try {
    // 〔MIG-3b〕`ccm` 就是那台后端本身：问得到就是装了（「没装」那一格只剩对端说「不认这条」—— 那是老后端，按「不知道」走）。
    const budget = budgetWithin(CCM_PROBE_BUDGET_MS);
    const body = jsonBody({});
    try {
      value = decodeCcmProbe(readJson(await chan.call(origin as Origin, "ccm-probe", body, budget)));
    } catch (e) {
      // 对端事前就说「不认这条」⇒ 那台的 `ccm`（就是那台后端）早于名片这一问 —— 这是一个**真答案**：CLI 渲染器用不上。
      if (e instanceof ChanError && e.error.layer === "peer" && e.error.why === "unsupported") value = NOT_INSTALLED;
      else throw e;
    }
  } catch (e) {
    // 探测失败（ssh 抖动/远端不可达等）⇒ **「不知道」**。
    // 渲染那一侧照旧降级到兜底渲染器（见头注），但这个值**不说「没装」**，
    // 也**不进缓存** —— 把一句没人验证过的话缓存 5 分钟，比多一次 ssh 往返贵得多。
    return { state: "unknown", error: String(e) };
  }
  probeCache.set(origin, { at: now, value });
  return value;
}

/** F08 安装向导完成后调用，免得用户装完还要等 5 分钟 TTL 才用上 CLI 渲染器。 */
export function invalidateCcmProbeCache(origin?: string): void {
  if (origin) probeCache.delete(origin);
  else probeCache.clear();
}

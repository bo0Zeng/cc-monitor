/**
 * F03（unify-launch）：ccm CLI 探测缓存——`renderLaunchCommand`（`remote-launch-run.ts`）据此
 * 决定某个 origin 走 CLI 渲染器还是兜底渲染器。
 *
 * **何时探测**：惰性、按 origin、首次用到才探——不新增任何轮询（守 MASTERPLAN §5.4/§0.1）。
 * **缓存多久**：5 分钟——够长（不会让"每点一次 resume 就多一次 ssh 往返"），够短（用户刚装/
 * 升级 ccm 之后，下一次操作基本感知得到，F08 的安装向导完成后应调 `invalidateCcmProbeCache`
 * 让下次启动立刻用上 CLI 渲染器，不必等 TTL）。
 * **探测失败怎么降级**：一律并入"未装"——探测是**可用性判断**，不是安全边界，这里 fail-open
 * 到"更保守、被充分验证过的兜底渲染器"是正确方向（区别于 `isValidConfigDir` 这类必须
 * fail-closed 的安全校验）。
 */
import { invoke } from "@tauri-apps/api/core";

export interface CcmProbeResult {
  installed: boolean;
  version: string | null;
  capabilities: Set<string>;
}

interface RawCcmProbeResult {
  installed: boolean;
  version: string | null;
  capabilities: string[];
}

const CCM_PROBE_TTL_MS = 5 * 60_000;
const NOT_INSTALLED: CcmProbeResult = { installed: false, version: null, capabilities: new Set() };
const probeCache = new Map<string, { at: number; value: CcmProbeResult }>();

export async function probeCcm(origin: string, force = false): Promise<CcmProbeResult> {
  const now = Date.now();
  const cached = probeCache.get(origin);
  if (!force && cached && now - cached.at < CCM_PROBE_TTL_MS) return cached.value;
  let value: CcmProbeResult;
  try {
    const raw = await invoke<RawCcmProbeResult>("probe_ccm_cli", { origin });
    value = raw.installed
      ? { installed: true, version: raw.version, capabilities: new Set(raw.capabilities) }
      : NOT_INSTALLED;
  } catch {
    // 探测失败（ssh 抖动/远端不可达等）→ 安全降级到兜底渲染器，不抛给调用方。
    value = NOT_INSTALLED;
  }
  probeCache.set(origin, { at: now, value });
  return value;
}

/** F08 安装向导完成后调用，免得用户装完还要等 5 分钟 TTL 才用上 CLI 渲染器。 */
export function invalidateCcmProbeCache(origin?: string): void {
  if (origin) probeCache.delete(origin);
  else probeCache.clear();
}

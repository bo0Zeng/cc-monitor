/**
 * **本机能力**（界面这一半）：monitor 跑在哪个系统上、↗ 在这台上是不是真的、本机 shell 说哪种方言、本机 PATH 上 `ccm` 那份短缓存作不作数。
 * 判定只住壳（`src/frontend/shell/src/platform/host_facts.rs`）：它在每个 webview 起页时注入 `window.__CCM_HOST__`，这里只读、不猜
 * （User-Agent 判法退役）。读不到 / 形状不对（不该发生：壳每一页都注入）⇒ 「认不出」那一份 {@link UNKNOWN_HOST}：系统写不出、方言不猜，
 * ↗ 照常显示（错藏起来用户就找不到这颗按钮）。
 */

export type HostOs = "windows" | "macos" | "linux" | "unknown";

/** 壳给的那一份（`platform/host_facts.rs::HostFacts` 的 JSON 形）。 */
export interface HostFacts {
  os: HostOs;
  terminalFront: boolean;
  /** `null` ＝ 认不出（别名那一格明说、安装入口置灰）。 */
  shellDialect: "posix" | "powershell" | null;
  ccmPathCache: boolean;
}

/** 读不到壳那一份时的样子。 */
export const UNKNOWN_HOST: HostFacts = { os: "unknown", terminalFront: true, shellDialect: null, ccmPathCache: true };

const OSES: readonly HostOs[] = ["windows", "macos", "linux"];

/** `window.__CCM_HOST__` ⇒ 那一份；形状不对 ⇒ {@link UNKNOWN_HOST}（纯函数）。 */
export function readHostFacts(v: unknown): HostFacts {
  if (v === null || typeof v !== "object") return UNKNOWN_HOST;
  const o = v as Record<string, unknown>;
  if (typeof o.terminalFront !== "boolean" || typeof o.ccmPathCache !== "boolean") return UNKNOWN_HOST;
  const os = OSES.find((x) => x === o.os) ?? "unknown";
  const shellDialect = o.shellDialect === "posix" || o.shellDialect === "powershell" ? o.shellDialect : null;
  return { os, terminalFront: o.terminalFront, shellDialect, ccmPathCache: o.ccmPathCache };
}

/** 测试覆盖值。非 null 时 `hostFacts()` 直接返回它。 */
let override: HostFacts | null = null;
/** 缓存（一页里不会变）；`null` ＝ 还没读过。 */
let cached: HostFacts | null = null;

/** 这台的那一份。 */
export function hostFacts(): HostFacts {
  if (override !== null) return override;
  cached ??= readHostFacts(typeof window === "undefined" ? undefined : (window as { __CCM_HOST__?: unknown }).__CCM_HOST__);
  return cached;
}

/** 当前 monitor 跑在哪个系统上。 */
export function hostOs(): HostOs {
  return hostFacts().os;
}

/** 仅供测试：置 / 清覆盖值（`null` 同时清缓存）。 */
export function __setHostFactsForTests(f: HostFacts | null): void {
  override = f;
  cached = null;
}

/** 根元素上标出界面跑在哪个系统上（`data-host-os`），给只能按平台分的那几条样式用（`tokens.css` 里 Linux 的等宽字体缺省）。三个窗口的入口各标一次。 */
export function markHostOs(root: HTMLElement): void {
  root.dataset.hostOs = hostOs();
}

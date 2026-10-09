/**
 * monitor 跑在哪个 OS 上：本机页上 PowerShell 那一侧的别名块（`machine-aliases.ts::localShell`）按它选平台。
 * 用 UA 判：零依赖、零 IPC、纯函数好测（三个平台的 webview 是 WebView2 / WKWebView / WebKitGTK，
 * `Windows NT` / `Macintosh` / `X11; Linux` 都是稳定标识）；不为一个显隐装插件或新开命令。
 * 测不出就照常显示（见 `hostOsAllows`）。
 */

export type HostOs = "windows" | "macos" | "linux" | "unknown";

/**
 * 从 UA 串判 OS（纯函数）。顺序从窄到宽：先 Windows、再 macOS、最后 Linux（`Linux` 最容易出现在别的平台的 UA 里，如 Android）。
 * 对今天三个平台的真实 UA 这顺序是等价变异，留给将来加平台；`host-os.vitest.ts` 不假装守它。
 */
export function detectHostOs(ua: string): HostOs {
  if (/Windows NT|Windows/i.test(ua)) return "windows";
  if (/Macintosh|Mac OS X/i.test(ua)) return "macos";
  if (/X11|Linux/i.test(ua)) return "linux";
  return "unknown";
}

/** 测试覆盖值。非 null 时 `hostOs()` 直接返回它。 */
let override: HostOs | null = null;

/** 缓存（UA 在一个进程里不会变）；`null` ＝ 还没算过。 */
let cached: HostOs | null = null;

/** 当前 monitor 跑在哪个 OS 上。 */
export function hostOs(): HostOs {
  if (override !== null) return override;
  if (cached !== null) return cached;
  const ua =
    typeof navigator === "undefined" ? "" : (navigator.userAgent ?? "");
  cached = detectHostOs(ua);
  return cached;
}

/** 仅供测试：置 / 清覆盖值（`null` 同时清缓存）。jsdom 的 UA 含 `linux`：面板测试不置成 windows，PowerShell 那块就不出现。 */
export function __setHostOsForTests(os: HostOs | null): void {
  override = os;
  cached = null;
}

/**
 * 这块在当前 OS 上该不该出现（`undefined` ＝ 与 OS 无关）。`unknown` 走显示：错藏起来 Windows 用户找不到安装入口、
 * 也没线索说它去哪了；错显示只是多一块。
 */
export function hostOsAllows(allowed: readonly HostOs[] | undefined): boolean {
  if (!allowed) return true;
  const os = hostOs();
  return os === "unknown" || allowed.includes(os);
}

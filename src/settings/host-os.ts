/**
 * S9（settings-ia）：**monitor 跑在哪个 OS 上**。
 *
 * # 为什么需要它
 *
 * 「PowerShell 集成」这一块（`cc_integration.ts`）挂在**本机页**上，而它整篇都是
 * `$PROFILE` / `Microsoft.PowerShell_profile.ps1`。v3.4.0 已经发了 `.deb` ——
 * Linux 用户打开设置就会看到一个装 PowerShell profile 的安装器。
 *
 * 而在此之前**全仓前端零 OS 分支**（`Windows NT|X11|Macintosh|win32|darwin` 全仓 0 命中），
 * 所以这个模块是那条判据的第一个落点。
 *
 * # 为什么是 UA，不是插件也不是新命令
 *
 * - `@tauri-apps/plugin-os` 要**装包**。
 * - 新开一个 Tauri 命令读 `std::env::consts::OS`：除了要动三处钉死的命令总数，
 *   更要紧的是它会**新增一项「本地独有能力」**—— 而主计划 §2.2 反对「部署→远端/本地」
 *   分栏的立足点，正是 `parity_ledger` 里本地独有一侧正在清空。为一个 UI 显隐
 *   给自己的架构论证挖坑，不划算。
 * - UA 零依赖、零 IPC、是个**纯函数**，好测。Tauri 的 webview 在三个平台上分别是
 *   WebView2 / WKWebView / WebKitGTK，`Windows NT` / `Macintosh` / `X11; Linux`
 *   都是稳定标识。
 *
 * # 失败方向：测不出就**照常显示**
 *
 * 见 `detectHostOs` 与 `hostOsAllows` 的注释 —— 两种错的代价不对称。
 */

export type HostOs = "windows" | "macos" | "linux" | "unknown";

/**
 * 从 UA 串判 OS。**纯函数**，不碰全局。
 *
 * 顺序**从窄到宽**：先 Windows、再 macOS、最后 Linux（`Linux` 这个词最容易出现在
 * 别的平台的 UA 里，最典型是 Android）。
 *
 * **但别高估这条顺序**：写测试前算过一遍，把顺序反过来，三个平台的真实 UA 结果
 * 一个都不变（Windows 串不含 `Linux`/`X11`，macOS 串也不含）—— 今天它是个**等价变异**。
 * 所以顺序照这么排是给将来加平台留的余地，`host-os.vitest.ts` 里**没有**假装守它的测试。
 */
export function detectHostOs(ua: string): HostOs {
  if (/Windows NT|Windows/i.test(ua)) return "windows";
  if (/Macintosh|Mac OS X/i.test(ua)) return "macos";
  if (/X11|Linux/i.test(ua)) return "linux";
  return "unknown";
}

/** 测试覆盖值。非 null 时 `hostOs()` 直接返回它。 */
let override: HostOs | null = null;

/**
 * 缓存：UA 在一个进程里不会变，没必要每次显隐判断都跑一遍正则。
 * `null` = 还没算过。
 */
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

/**
 * 仅供测试：置 / 清覆盖值。传 `null` 清掉（同时清缓存，下次重新算）。
 *
 * jsdom 的 UA 含 `linux`，所以**面板类测试如果不显式置成 windows，
 * PowerShell 那块就不会出现** —— 那是门生效的正确信号，不是测试环境的毛病。
 */
export function __setHostOsForTests(os: HostOs | null): void {
  override = os;
  cached = null;
}

/**
 * 「这块在当前 OS 上该不该出现」。`undefined` = 与 OS 无关，一律出现。
 *
 * **`unknown` 走显示这条**，因为两种错的代价不对称：
 * - 错判成非 Windows 而藏起来 ⇒ Windows 用户**找不到安装入口**，且界面上没有任何
 *   线索说它去哪了。
 * - 错判成 Windows 而显示 ⇒ 就是加这道门**之前**的行为，不构成回归。
 *
 * 所以不确定性压在无回归的那一侧。
 */
export function hostOsAllows(allowed: readonly HostOs[] | undefined): boolean {
  if (!allowed) return true;
  const os = hostOs();
  return os === "unknown" || allowed.includes(os);
}

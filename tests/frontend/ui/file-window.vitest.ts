/**
 * 原生文件窗口的前端开口（`src/frontend/ui/file-window.ts`）与**全部入口**的判据。
 *
 * 两件事，各一种判法：
 *
 * 1. **开口本身**（行为判据）：三种落点翻成的 `open_file_window` 实参，与 Rust 侧
 *    `filewin::entry::plan_target` 的三支逐一相等；失败带着原文出声，成功不出声。
 *    期望是按 `IPC` 契约**手写**的，实得是 mock 住的 `invoke` 真收到的那一份 —— 两侧不同源。
 * 2. **入口人群**（结构判据，两向相等）：生产树里调 `openFileWindow(` 的 (文件, 落点) 多重集
 *    == 下面 `ENTRIES` 那张表；而 `open_file_window` 这条命令在包装层之外**恰好一处**
 *    （就是那个开口）。多一处 ＝ 有人绕开开口另起一条路；少一处 ＝ 某个入口掉线了。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...a: unknown[]) => invokeMock(...a),
  Channel: class {
    onmessage: ((p: unknown) => void) | null = null;
  },
}));
const toastMock = vi.fn();
vi.mock("../../../src/frontend/ui/kit/toast", () => ({ toast: (...a: unknown[]) => toastMock(...a) }));

import { openFileWindow, fileWindowTheme, FILE_WINDOW_THEME_TOKENS } from "../../../src/frontend/ui/file-window";
import type { RemoteHostConfig } from "../../../src/frontend/ui/remote-config";
import { productionTsFiles, SCAN_TIMEOUT_MS } from "../../test-support/production-sources";
import { stripComments } from "../../test-support/strip-comments";
import { copyText } from "../../../src/frontend/ui/copy-table";

const CFG = {
  label: "devbox",
  host: "h",
  port: 22,
  user: "u",
  keyPath: "",
  hostKeyFingerprint: "",
  addresses: [],
  jump: "",
  resumeCommand: "",
} as unknown as RemoteHostConfig;

describe("F7b 开口：三种落点 → open_file_window 的实参", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    toastMock.mockReset();
  });

  it("目录 / 定位文件 / home 三支，实参与 plan_target 的三支逐一相等", async () => {
    invokeMock.mockResolvedValue(3);
    expect(await openFileWindow(CFG, { dir: "/srv/data" })).toBe(true);
    expect(await openFileWindow(CFG, { revealFile: "/srv/data/a.md" })).toBe(true);
    expect(await openFileWindow(CFG)).toBe(true);
    expect(invokeMock.mock.calls).toEqual([
      // `path` 非空 ⇒ Rust 侧 `Target::Dir`
      ["open_file_window", { cfg: CFG, path: "/srv/data", revealFile: null, theme: fileWindowTheme() }],
      // `path` 空 ＋ `revealFile` ⇒ `Target::Reveal`（父目录与尾段由 Rust 侧切，这里原样交）
      ["open_file_window", { cfg: CFG, path: "", revealFile: "/srv/data/a.md", theme: fileWindowTheme() }],
      // 两个都空 ⇒ `Target::Home`
      ["open_file_window", { cfg: CFG, path: "", revealFile: null, theme: fileWindowTheme() }],
    ]);
    expect(toastMock, "成功那一支不该出声（窗口自己出现就是回应）").not.toHaveBeenCalled();
  });

  it("样子那一格：名单上每个令牌都带上，值取的是 `:root` 此刻的计算值（含用户改过的）", async () => {
    invokeMock.mockResolvedValue(1);
    document.documentElement.style.setProperty("--accent", "#123456");
    await openFileWindow(CFG);
    document.documentElement.style.removeProperty("--accent");
    const theme = (invokeMock.mock.calls[0][1] as { theme: Record<string, string> }).theme;
    expect(Object.keys(theme).sort()).toEqual([...FILE_WINDOW_THEME_TOKENS].sort());
    expect(theme["--accent"]).toBe("#123456");
  });

  it("开不起来 ⇒ 带着 Rust 侧原文出声，回 false", async () => {
    invokeMock.mockRejectedValue("窗口没起来：没有图形会话");
    expect(await openFileWindow(CFG, { dir: "/srv" })).toBe(false);
    expect(toastMock.mock.calls).toEqual([[copyText("fileWindow.openFileWindow.failed"), "窗口没起来：没有图形会话"]]);
  });
});

/** 生产树里（剥掉注释）每一处 `openFileWindow(…)` 的 (文件, 落点)。 */
function entryCensus(): string[] {
  const out: string[] = [];
  for (const { file, text } of productionTsFiles("src")) {
    if (file === "src/frontend/ui/file-window.ts") continue; // 开口自己（声明处）不是入口
    const code = stripComments(text, "ts");
    for (const m of code.matchAll(/\bopenFileWindow\(([^)]*)\)/g)) {
      const arg = m[1];
      const kind = /\brevealFile\b/.test(arg) ? "定位文件" : /\bdir\b/.test(arg) ? copyText("gridMonitor.fact.dir") : "home";
      out.push(`${file} · ${kind}`);
    }
  }
  return out.sort();
}

/**
 * 入口全表。**改入口就改这张表**，理由写在行尾。
 */
const ENTRIES: readonly string[] = [
  "src/frontend/ui/cards/index.ts · 定位文件", // 会话工具卡上的文件链接（老面板 revealPath，F54）
  `src/frontend/ui/settings/ext-section.ts · ${copyText("gridMonitor.fact.dir")}`, // 扩展页抽屉：远端那台上一个 skill 的目录「在文件窗口里打开」
  "src/frontend/ui/settings/data-page.ts · 定位文件", // 「要你动手」点开一件：远端那台上要改的那份文件「打开这份文件」
  `src/frontend/ui/settings/accounts-section.ts · ${copyText("gridMonitor.fact.dir")}`, // 账号页：远端那台一个号的账号目录「在文件窗口里打开」
  "src/frontend/ui/settings/machine-card.ts · home", // 机器页「文件」按钮
  // 顶栏远端文件那两处随选主机小单从 `src/frontend/ui/main.ts` 搬进 `src/frontend/ui/sftp-host-picker.ts`。
  "src/frontend/ui/sftp-host-picker.ts · home", // 顶栏 / 命令面板：只有一台远端时直开
  "src/frontend/ui/sftp-host-picker.ts · home", // 顶栏 / 命令面板：多台时选单里点一台
  // 住址随会话动作从 `src/frontend/ui/tabs.ts` 搬到 `src/frontend/ui/tab-session-actions.ts`（openTabCwd 逐字随行）。
  `src/frontend/ui/tab-session-actions.ts · ${copyText("gridMonitor.fact.dir")}`, // 远端会话「打开工作目录」（老面板 initialDir，F78）
];

describe("F7b 入口人群", () => {
  it("生产树里调开口的 (文件, 落点) == 入口全表（两向）", () => {
    expect(entryCensus()).toEqual([...ENTRIES].sort());
  }, SCAN_TIMEOUT_MS);

  it("`open_file_window` 在包装层之外恰好一处 —— 就是那个开口", () => {
    const sites: string[] = [];
    for (const { file, text } of productionTsFiles("src")) {
      if (file === "src/frontend/ui/ipc/commands.ts") continue; // 包装层自己
      const code = stripComments(text, "ts");
      const n = code.match(/\bopen_file_window\b/g)?.length ?? 0;
      for (let i = 0; i < n; i++) sites.push(file);
    }
    expect(sites).toEqual(["src/frontend/ui/file-window.ts"]);
  }, SCAN_TIMEOUT_MS);
});

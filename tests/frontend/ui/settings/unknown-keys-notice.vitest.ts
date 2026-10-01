/**
 * 🔴 **「未知键出声」的那一半必须出在用户眼前。**
 *
 * # 为什么这条与 `tests/frontend/ui/config-unknown-keys.vitest.ts` 不能合成一条
 *
 * 那条证明的是「数得出来」（`unknownKeysIn` / `loadConfig` 那格快照）。
 * 但「数出来了」与「用户看见了」是两件事 —— 本仓已经在这上面栽过：
 * 一条只写进 `console.warn` 的告知，与根本没告知，在终端上一模一样
 *（`config.ts` 里那句 `console.warn` 的旁注逐字说的就是这件事）。
 *
 * ⇒ 本条**真渲染设置面板**，然后去 DOM 里找那个键名。它买的是
 * 「这条提示确实挂在用户走的那条路上」，而不是「那个函数返回了对的东西」。
 *
 * # 死值验切在哪
 *
 * 切点是**语义**不是拼写：往 mock 的 config.json 里真放一个未知键，看用户那一侧
 * 到底出没出声。把 `panel.ts` 里那行 `appendChild(createUnknownKeysBar())` 摘掉
 *（组件还在、函数还在、编译照过）⇒ 下面「面板上真有那句话」当场红。
 *
 * # 射程
 *
 * - jsdom 里渲染的是**面板**，不是真 app 窗口。主窗口启动时那条（`src/frontend/ui/main.ts`）
 *   `P12` 的写区够不到，今天没有 ⇒ 从没打开过设置的用户看不到。这一条本文件盖不到。
 * - 不判样式（条是不是够显眼）。那归 `src/frontend/ui/styles.css` 与 `css-ledger` 那一族。
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

const store = vi.hoisted(() => ({ cfg: {} as Record<string, unknown>, readFails: false }));

// 只 mock 最外面那层 IPC —— `config.ts` / `behavior.ts` 走生产那条真链。
vi.mock("../../../../src/frontend/ui/ipc/commands", () => ({
  commands: new Proxy(
    {},
    {
      get: (_t, name: string) => {
        if (name === "load_config") {
          return () =>
            store.readFails
              ? Promise.reject(new Error("读不到 config.json"))
              : Promise.resolve(store.cfg);
        }
        if (name === "patch_config") return () => Promise.resolve(); // 写口换成按键补丁
        return () => Promise.reject(new Error(`[录音机] ${name} 没有真后端`));
      },
    },
  ),
}));
vi.mock("../../../../src/frontend/ui/settings/remote-section", () => ({
  MACHINE_PAGE_PREFIX: "machine:",
  LOCAL_MACHINE_PAGE_ID: "machine:（本机）",
  RemoteSection: class {
    element = document.createElement("div");
    refresh = vi.fn().mockResolvedValue(undefined);
  },
}));
vi.mock("../../../../src/frontend/ui/settings/accounts-section", () => ({
  AccountsSection: class { element = document.createElement("div"); },
}));
vi.mock("../../../../src/frontend/ui/settings/mcp-section", () => ({
  McpSection: class { element = document.createElement("div"); },
}));
vi.mock("../../../../src/frontend/ui/settings/plugins-section", () => ({
  PluginsSection: class { element = document.createElement("div"); },
}));
// `cc_integration.ts` 并进了 `machine-aliases.ts`（终端集成成了「别名」那一块 PowerShell 那一侧），它的替身随之删掉。
vi.mock("../../../../src/frontend/ui/keybindings/editor", () => ({
  KeybindingsEditor: class { element = document.createElement("div"); },
}));
vi.mock("../../../../src/frontend/ui/keybindings/registry", () => ({
  dispatcher: {
    pushOverlay: vi.fn(),
    popOverlay: vi.fn(),
    startRecording: vi.fn(),
    cancelRecording: vi.fn(),
    exportOverrides: vi.fn().mockReturnValue({}),
    applyOverrides: vi.fn(),
  },
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ emit: vi.fn() }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({ close: vi.fn() }) }));

import {
  createUnknownKeysBar,
  unknownKeysMessage,
} from "../../../../src/frontend/ui/settings/unknown-keys-notice";
import { __resetUnknownConfigKeysForTests } from "../../../../src/frontend/ui/config";
import { SettingsPanel } from "../../../../src/frontend/ui/settings/panel";
import { __setHostOsForTests } from "../../../../src/frontend/ui/settings/host-os";
import { __resetMachineContextForTests } from "../../../../src/frontend/ui/settings/machine-context";

/** 退役的那个落盘键。**逐字** —— 它出现时这条判据必须红。 */
const RETIRED_KEY = "forceLegacyLaunchRenderer";

const tick = (): Promise<void> => new Promise((r) => setTimeout(r, 0));

beforeEach(() => {
  store.cfg = {};
  store.readFails = false;
  __resetUnknownConfigKeysForTests();
  document.body.innerHTML = "";
  __setHostOsForTests("linux");
  __resetMachineContextForTests();
});

afterEach(() => {
  document.body.innerHTML = "";
});

describe("P12：那句话本身（`unknownKeysMessage` 纯函数）", () => {
  it("没有未知键 ⇒ 没有话说（整条不该出现）", () => {
    expect(unknownKeysMessage([])).toBeNull();
  });

  it("★ 有未知键 ⇒ **逐个指名**，不是只说「有几个」", () => {
    const msg = unknownKeysMessage([RETIRED_KEY, "某个错字"]);
    expect(msg, "一句话都没有").not.toBeNull();
    expect(
      msg,
      "只说了「有未知键」而不说是哪个 —— 用户还是得自己去翻文件逐行比对，" +
        "那跟没说差得不多",
    ).toContain(RETIRED_KEY);
    expect(msg).toContain("某个错字");
  });
});

describe("P12：那条常驻条（`createUnknownKeysBar`）", () => {
  it("★ 配置里真放一个未知键 ⇒ 条亮起来、并把键名写在上面", async () => {
    store.cfg = { theme: {}, [RETIRED_KEY]: true };
    const bar = createUnknownKeysBar();
    await tick();
    expect(bar.hidden, "条还是藏着的 —— 用户什么都看不到").toBe(false);
    expect(
      bar.textContent,
      `盘上有退役键 \`${RETIRED_KEY}\`，而条上一个字都没提它`,
    ).toContain(RETIRED_KEY);
  });

  it("★ 反空真：配置干净 ⇒ 条是藏着的、正文是空的（不是「永远亮着」）", async () => {
    store.cfg = { theme: {}, notifyTurnEnd: true, autoFollowUserActive: false }; // 原来第三个键是已退役的 forceLaunchPayloadRenderer
    const bar = createUnknownKeysBar();
    await tick();
    expect(
      bar.hidden,
      "配置里一个未知键都没有，条却亮着 —— 恒亮的告警等于没有告警（`INVARIANTS §12`）",
    ).toBe(true);
    expect(bar.textContent).toBe("");
  });

  it("★ 刻意**没有关闭按钮** —— 点一下不会让那个键变得生效", async () => {
    store.cfg = { [RETIRED_KEY]: true };
    const bar = createUnknownKeysBar();
    await tick();
    expect(
      bar.querySelectorAll("button").length,
      "给了一个「知道了」—— 用户会把一个仍然为真的状态划掉（同 restart-notice 那条判例）",
    ).toBe(0);
  });

  it("读盘失败**不清空**已经显示的那句（读不到不等于键消失了）", async () => {
    store.cfg = { [RETIRED_KEY]: true };
    // 先让 `config.ts` 那格快照装上东西（app 启动时早就读过了），再让刷新那一发失败。
    const { loadConfig } = await import("../../../../src/frontend/ui/config");
    await loadConfig();
    store.readFails = true;
    const bar = createUnknownKeysBar();
    await tick();
    expect(bar.textContent).toContain(RETIRED_KEY);
  });
});

describe("🔴 P12：真·用户那一侧 —— 打开设置就看得见", () => {
  it("★★ 配置里放一个未知键 ⇒ **设置面板的 DOM 上真有那句话**", async () => {
    store.cfg = { theme: {}, [RETIRED_KEY]: true };
    const panel = new SettingsPanel();
    await panel.open();
    await tick();
    await tick();
    // ⚠ 从 `document` 里捞 —— 面板把自己 `appendChild` 到 body 上，而 `panel.el` 是私有的。
    //   捞 DOM 而不是读字段，正好也更贴「用户看得见」这件事。
    const text = (document.querySelector(".settings-panel") as HTMLElement | null)?.textContent ?? "";
    void panel;
    expect(
      text.length,
      "面板压根没渲染出东西 —— 下面那条会是零命中地绿",
    ).toBeGreaterThan(200);
    expect(
      text,
      "面板上找不到那个键名。\n" +
        "⇒ 「未知键出声」这件事今天只活在函数里：`unknownKeysIn` 数得出来，\n" +
        "   但用户打开设置**什么都看不到** —— 而那正是 `P12` 要治的那个病\n" +
        "   （「关掉了」与「过了」在终端上一模一样）。\n" +
        "最可能的原因：`settings/panel.ts` 的 `build()` 里那行 `createUnknownKeysBar()` 没挂上去。",
    ).toContain(RETIRED_KEY);
  });

  it("★★ 反空真对照：配置干净 ⇒ 面板上**没有**那句话", async () => {
    store.cfg = { theme: {}, notifyTurnEnd: true };
    const panel = new SettingsPanel();
    await panel.open();
    await tick();
    await tick();
    // ⚠ 从 `document` 里捞 —— 面板把自己 `appendChild` 到 body 上，而 `panel.el` 是私有的。
    //   捞 DOM 而不是读字段，正好也更贴「用户看得见」这件事。
    const text = (document.querySelector(".settings-panel") as HTMLElement | null)?.textContent ?? "";
    void panel;
    expect(text.length).toBeGreaterThan(200);
    expect(
      text,
      "配置里没有任何未知键，面板上却挂着那句话 —— 上面那条于是与被测的性质无关，恒绿",
    ).not.toContain("认不出来");
  });
});

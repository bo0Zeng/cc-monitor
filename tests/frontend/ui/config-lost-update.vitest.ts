/**
 * 🔴 **config.json 丢更新**的复现判据（E §E1 · B §2.4）。
 *
 * 守的要求（住址）：
 * -：「两者 …… 各自只写自己那个键」；`§C.3`：「**只动自己那个键**（`tabBar.order`），不整段覆盖 ——
 *   否则 order 与 pinned 两条路会互相把对方写没」。
 * -：「两边写不同的文件 ⇒ 单写者，『读—改—写整份覆盖掉对方刚写的键』那个竞态在构造上不存在」
 *   —— config.json 是**同一个文件、多个写者**，本条要的是同一个性质：谁写的键谁的值留在盘上。
 *
 * # 形状
 *
 * 两个 realm（主窗 / 设置窗 —— 同一个 monitor 进程里的两个 webview，各自一份模块实例）× 全部写者，**同一拍一起发**，
 * `await` 全部，然后看盘：每个写者写的值都在，且盘上没有期望之外的键（**两向相等**，期望手写）。
 *
 * 假盘在 `@tauri-apps/api/core::invoke` 这一层（最外层），于是 `config.ts` / `ipc/commands.ts` / 各写者走的都是
 * 生产那条真链。每一跳 IPC 都让出一次事件循环（`setTimeout 0`），模拟 Tauri 往返 —— 旧代码的
 * 「`load_config` → 改 → `save_config` 整份」两跳之间别人的写会插进来，这正是 E1 的形。 〔散文墓碑〕
 *
 * ⚠ 射程：假盘的 `patch_config` 用 `tests/frontend/ui/config-patch-fake.ts::applyConfigEdits`（与 Rust 写口跑同一份金样）。本条**只**证明
 * 「前端每个写者只交自己那几条路径」；Rust 写口本身在并发下合并得对不对，归
 * `tests/frontend/shell/config_tests.rs`（真文件、真线程）。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

import type { Edit } from "./config-patch-fake";

const disk = vi.hoisted(() => ({
  text: "{}",
  /** 每一次 `patch_config` 收到的 edits（J5：写者只交自己的键）。 */
  patches: [] as unknown[][],
  /** 依次给每一跳 IPC 的额外延迟（毫秒），用完为 0。用来造「先发的后到」。 */
  delays: [] as number[],
}));

vi.mock("@tauri-apps/api/core", async () => {
  // 补丁语义用金样钉住的那一份（`tests/frontend/ui/config-patch-fake.ts`，与 Rust 写口跑同一份金样）。
  const { applyConfigEdits } = await import("./config-patch-fake");
  const hop = (): Promise<void> =>
    new Promise((r) => setTimeout(r, disk.delays.shift() ?? 0));
  return {
    invoke: vi.fn(async (cmd: string, args?: Record<string, unknown>) => {
      await hop();
      switch (cmd) {
        case "load_config":
          return JSON.parse(disk.text);
        case "save_config":
          disk.text = JSON.stringify(args!.value);
          return undefined;
        case "patch_config": {
          const edits = args!.edits as Edit[];
          disk.patches.push(edits);
          disk.text = applyConfigEdits(disk.text, edits);
          return undefined;
        }
        default:
          throw new Error(`fake invoke: 没有这条命令 ${cmd}`);
      }
    }),
    Channel: class {
      onmessage: ((v: unknown) => void) | null = null;
    },
  };
});
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}), emit: vi.fn() }));

const PIN = {
  sid: "sid-p",
  jsonlPath: "/j/p.jsonl",
  cwd: "/w",
  origin: "<local>",
  account: "a1",
  lastActiveAt: 1,
  kind: null,
  name: null,
  title: "固定那条",
};
// 组只存 `{id, name}`；组员关系是 tab 自己的属性（`tabBar.groupOf.<sid>`）。
const COLS = [{ id: "c1", name: "组一" }];
const BEHAVIOR = {
  autoFollowUserActive: false,
  bringMonitorToFrontOnUserActive: true,
  showBgSessions: false,
  resumeCommandLocal: "claude --resume",
  resumeCommandRemote: "claude --resume",
  resumeCommandLocalPresets: [],
  resumeCommandRemotePresets: [],
  notifyTurnEnd: true,
  // `forceLaunchPayloadRenderer` 退役，行为配置里没有这一格了。
};
const KEYS = { "open-settings": "Ctrl+,", "kill-session": null };

/** 手写期望：全部写者一起发之后，盘上**恰好**是这一份。 */
const EXPECTED: Record<string, unknown> = {
  tabCollections: COLS,
  tabBar: { order: ["sid-b", "sid-a"], pinned: [PIN], groupOf: { "sid-a": "c1", "sid-b": "c1" } },
  ...BEHAVIOR,
  theme: { bg: "#010203" },
  claudeDir: "/tmp/cfg1-claude",
  keybindings: KEYS,
  accounts: { byMachine: { aya: { defaultName: "a1", modelByAccount: { a1: "opus" } } } },
  remote: { enabled: true }, // 只写 enabled 那一格，不再整段写出一个空 hosts
};

/** 一个 realm 的全部写者（一份模块实例）。 */
async function realm(): Promise<{
  main: () => Promise<unknown>[];
  settings: () => Promise<unknown>[];
}> {
  const cfg = await import("../../../src/frontend/ui/config");
  const cols = await import("../../../src/frontend/ui/tab-collections");
  const bar = await import("../../../src/frontend/ui/tab-bar-state");
  const beh = await import("../../../src/frontend/ui/behavior");
  const theme = await import("../../../src/frontend/ui/theme");
  const paths = await import("../../../src/frontend/ui/paths");
  const kb = await import("../../../src/frontend/ui/keybindings/store");
  const acc = await import("../../../src/frontend/ui/account-prefs");
  const remote = await import("../../../src/frontend/ui/remote-config");
  return {
    // 主窗：tab 栏那几条（E1 的原形：拖放同拍发分组 ＋ 顺序）。
    // 分组那一条的形状照 `TabBarPrefs.foundGroup`：组表 ＋ 两个 tab 的组 id 键装进**一次**补丁。
    main: () => [
      cfg.patchConfig([
        cols.collectionsEdit(COLS),
        bar.groupOfEdit("sid-a", "c1"),
        bar.groupOfEdit("sid-b", "c1"),
      ]),
      bar.setTabOrder(["sid-b", "sid-a"]),
      bar.setPinned([PIN as never]),
    ],
    // 设置窗：其余全部
    settings: () => [
      beh.setBehavior(BEHAVIOR),
      theme.saveTheme({ bg: "#010203" }),
      paths.setClaudeDirOverride("/tmp/cfg1-claude"),
      kb.setKeybindings(KEYS),
      acc.setDefaultName("aya", "a1"),
      acc.setModelForAccount("aya", "a1", "opus"),
      remote.patchRemoteConfig({ enabled: true }),
    ],
  };
}

describe("CFG1 J1 · 两个 realm × 全部写者同时写，谁写的键谁的值都在", () => {
  beforeEach(() => {
    disk.text = "{}";
    disk.patches = [];
    disk.delays = [];
  });

  it("盘上终态 == 手写期望（两向：期望的键都在，没有期望外的键）", async () => {
    vi.resetModules();
    const a = await realm(); // 主窗那份模块实例
    vi.resetModules();
    const b = await realm(); // 设置窗那份模块实例（独立的队列 / 快照）
    await Promise.all([...a.main(), ...b.settings()]);
    const onDisk = JSON.parse(disk.text) as Record<string, unknown>;
    expect(Object.keys(onDisk).sort()).toEqual(Object.keys(EXPECTED).sort());
    expect(onDisk).toEqual(EXPECTED);
    // J5：每个写者只交自己那几条路径 —— 全部补丁的路径多重集 == 手写清单（两向；多一条 = 有人在动别人的键）。
    const sent = (disk.patches.flat() as Edit[]).map((e) => `${e.op} ${e.path.join(".")}`).sort();
    expect(sent).toEqual(
      [
        "set tabCollections",
        "set tabBar.groupOf.sid-a",
        "set tabBar.groupOf.sid-b",
        "set tabBar.order",
        "set tabBar.pinned",
        ...Object.keys(BEHAVIOR).map((k) => `set ${k}`),
        "set theme",
        "set claudeDir",
        "set keybindings",
        "set accounts.byMachine.aya.defaultName",
        "set accounts.byMachine.aya.modelByAccount.a1",
        "set remote.enabled",
      ].sort(),
    );
  });

  it("同一 realm 连发同一个键两次：盘上是后发的那次（不靠 IPC 到达顺序）", async () => {
    vi.resetModules();
    const cfg = await import("../../../src/frontend/ui/config");
    const cols = await import("../../../src/frontend/ui/tab-collections");
    const v1 = [{ id: "c1", name: "旧" }];
    const v2 = [{ id: "c1", name: "新" }];
    // 第一跳（v1 那次写）晚 20ms 才被处理 ⇒ 两次若同时在飞，v1 会后落盘、盖掉 v2。
    disk.delays = [20];
    await Promise.all([cfg.patchConfig([cols.collectionsEdit(v1)]), cfg.patchConfig([cols.collectionsEdit(v2)])]);
    expect((JSON.parse(disk.text) as Record<string, unknown>).tabCollections).toEqual(v2);
  });
});

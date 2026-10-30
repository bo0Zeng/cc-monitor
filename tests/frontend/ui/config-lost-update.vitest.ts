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
 * `await` 全部，然后看交出去的补丁：每个写者只交自己那几条路径、值逐字对得上，没有期望之外的路径（**两向相等**，期望手写）。
 *
 * ⚠ 射程：替身的 `patch_config` 只记下补丁、不应用。本条**只**证明「前端每个写者只交自己那几条路径」与「后发的后到」；
 * 补丁落到盘上、Rust 写口在并发下合并得对不对，归 `tests/frontend/shell/config_tests.rs`（真文件、真线程）。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

import type { Edit } from "./config-patch-fake";

const disk = vi.hoisted(() => ({
  text: "{}",
  /** 每一次 `patch_config` 收到的 edits，按到达的先后（J5：写者只交自己的键）。 */
  patches: [] as unknown[][],
  /** 依次给每一跳 IPC 多让几拍（宏任务），用完为 0。用来造「先发的后到」：同时在飞的两跳，多让的那一跳必然后落。 */
  delays: [] as number[],
}));

vi.mock("@tauri-apps/api/core", async () => {
  const hop = async (): Promise<void> => {
    for (let n = disk.delays.shift() ?? 0; n >= 0; n--) await new Promise((r) => setTimeout(r, 0));
  };
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

const BEHAVIOR = {
  autoFollowUserActive: false,
  bringMonitorToFrontOnUserActive: true,
  resumeCommandPresets: [],
  // `forceLaunchPayloadRenderer` 退役，行为配置里没有这一格了。
};
const KEYS = { "open-settings": "Ctrl+,", "kill-session": null };

/** 手写期望：全部写者一起发之后，交出去的补丁**恰好**是这几条（不论先后）。 */
const EXPECTED: Edit[] = [
  { op: "set", path: ["tabBar", "order"], value: ["sid-b", "sid-a"] },
  { op: "set", path: ["tabBar", "collapsed"], value: ["g1"] },
  { op: "set", path: ["tabBar", "hiddenOpen"], value: true },
  ...Object.entries(BEHAVIOR).map(([k, value]): Edit => ({ op: "set", path: [k], value })),
  { op: "set", path: ["theme"], value: { bg: "#010203" } },
  { op: "set", path: ["keybindings"], value: KEYS },
  // 机器表 · Claude 数据目录不在这份文件里了（住本机后端 `machines.json` / `prefs.json`，写口是 `machines-set` / `prefs-set`），不在本条的写者里。
];
const byPath = (a: Edit, b: Edit) => a.path.join(".").localeCompare(b.path.join("."));

/** 一个 realm 的全部写者（一份模块实例）。 */
async function realm(): Promise<{
  main: () => Promise<unknown>[];
  settings: () => Promise<unknown>[];
}> {

  const bar = await import("../../../src/frontend/ui/tab-bar-state");
  const beh = await import("../../../src/frontend/ui/behavior");
  const theme = await import("../../../src/frontend/ui/theme");
  const kb = await import("../../../src/frontend/ui/keybindings/store");
  return {
    // 主窗：tab 栏那几条（E1 的原形：拖放同拍发收起 ＋ 顺序；组与固定住后端，不在这份文件里）。
    main: () => [bar.setTabOrder(["sid-b", "sid-a"]), bar.setCollapsed(["g1"]), bar.setHiddenOpen(true)],
    // 设置窗：其余全部
    settings: () => [
      beh.setBehavior(BEHAVIOR),
      theme.saveTheme({ bg: "#010203" }),
      kb.setKeybindings(KEYS),
    ],
  };
}

describe("CFG1 J1 · 两个 realm × 全部写者同时写，谁写的键谁的值都在", () => {
  beforeEach(() => {
    disk.text = "{}";
    disk.patches = [];
    disk.delays = [];
  });

  it("交出去的补丁 == 手写期望（两向：每个写者只交自己那几条路径、值逐字对得上；多一条 = 有人在动别人的键）", async () => {
    vi.resetModules();
    const a = await realm(); // 主窗那份模块实例
    vi.resetModules();
    const b = await realm(); // 设置窗那份模块实例（独立的队列 / 快照）
    await Promise.all([...a.main(), ...b.settings()]);
    expect((disk.patches.flat() as Edit[]).sort(byPath)).toEqual([...EXPECTED].sort(byPath));
  });

  it("同一 realm 连发同一个键两次：后发的那次后到（不靠 IPC 在路上的快慢）", async () => {
    vi.resetModules();
    const cfg = await import("../../../src/frontend/ui/config");
    const v1 = ["旧"];
    const v2 = ["新"];
    // 第一跳（v1 那次写）多让三拍才被处理 ⇒ 两次若同时在飞，v1 必然后落盘、盖掉 v2。
    disk.delays = [3];
    await Promise.all([cfg.patchConfig([cfg.setAt(["tabBar", "collapsed"], v1)]), cfg.patchConfig([cfg.setAt(["tabBar", "collapsed"], v2)])]);
    expect(disk.patches, "写口照到达的先后落盘 ⇒ 后发的要后到").toEqual([[{ op: "set", path: ["tabBar", "collapsed"], value: v1 }], [{ op: "set", path: ["tabBar", "collapsed"], value: v2 }]]);
  });
});

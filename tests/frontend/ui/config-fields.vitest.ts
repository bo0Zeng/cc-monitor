/**
 * **config.json 顶层字段的读写契约**〔audit-0805 §5 2c〕。
 *
 * `paths.ts`（Claude 数据目录）与 `keybindings/store.ts`（`keybindings`）：后者往 `config.json` 的**顶层**读写一个字段；
 * 前者第 7 批搬进了本机后端的偏好（`prefs.json` 的 `agentHomes`），钉的是它交给后端的那一形。
 *
 * # 为什么挑这两个，而不是按 0% 清单从大到小补
 *
 * 2c 挂着「那些 0% 文件没补测试」。逐个看过之后先剔掉两类：
 *
 * - **其实有测试的**：`cards/api-error.ts` · `format.ts` · `remote-health-throttle.ts`
 *   都有 node 侧 `*.test.ts`。「0%」是**覆盖率只统计 `*.vitest.ts`** 的产物，不是「没测」。
 * - **错了看得见的**：`session-viewer` / `keybindings/editor` / `tasks-panel` / `agents-panel`
 *   / 各 `cards/*` 都是渲染与面板 —— 坏了当场看得出来。
 *
 * 剩下这两个不同：**它们错了是静默的**。
 *
 * | 模块 | 静默失败长什么样 |
 * |---|---|
 * | `paths.ts` | 清除时交了别的形状 ⇒ 那一格删不掉 / 被当成相对路径拒 ⇒ 用户以为回到了默认 |
 * | `keybindings/store.ts` | 写覆盖时把 config 顶层别的字段冲掉 ⇒ 主题/数据目录一起没了 |
 *
 * ⚠ 本组**不追覆盖率数字**（同 2v 那轮的判准）：钉的是「错了会不会静默」那一类。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

const store = vi.hoisted(() => ({ cfg: {} as Record<string, unknown>, saved: [] as unknown[] }));

// 写只交补丁（`patchConfig`）；这里按与 Rust 写口同一份金样的语义（`tests/frontend/ui/config-patch-fake.ts`）应用到 `store.cfg`。
vi.mock("../../../src/frontend/ui/config", async (orig) => {
  const actual = await orig<Record<string, unknown>>();
  const { applyConfigEdits } = await import("./config-patch-fake");
  return {
    ...actual,
    loadConfig: vi.fn(async () => store.cfg),
    patchConfig: vi.fn(async (edits: Parameters<typeof applyConfigEdits>[1]) => {
      store.cfg = JSON.parse(applyConfigEdits(JSON.stringify(store.cfg), edits)) as Record<string, unknown>;
      store.saved.push(store.cfg);
    }),
  };
});
const prefs = vi.hoisted(() => ({ homes: {} as Record<string, string>, sets: [] as unknown[][] }));
vi.mock("../../../src/frontend/ui/machine-prefs", () => ({
  readPrefs: vi.fn(async () => ({ state: "ok", reason: null, detail: null, path: null, prefs: { agentHomes: prefs.homes } })),
  setPrefs: vi.fn(async (...a: unknown[]) => {
    prefs.sets.push(a);
    return { state: "ok", reason: null, detail: null, path: null, prefs: { agentHomes: {} } };
  }),
}));
vi.mock("../../../src/frontend/ui/keybindings/actions", () => ({
  findAction: (id: string) => (["tab.next", "tab.close-archived"].includes(id) ? { id } : undefined),
}));

import { readClaudeDir, setClaudeDirOverride } from "../../../src/frontend/ui/paths";
import { DEFAULT_AGENT } from "../../../src/frontend/ui/agent-profile";
import { LOCAL_ORIGIN } from "../../../src/frontend/ui/ipc/origin";
import { getKeybindings, setKeybindings } from "../../../src/frontend/ui/keybindings/store";
import { loadConfig } from "../../../src/frontend/ui/config";

const loadMock = loadConfig as unknown as ReturnType<typeof vi.fn>;

describe("config.json 顶层字段的读写契约（§5 2c：挑「错了会静默」的那两个）", () => {
  beforeEach(() => {
    store.cfg = {};
    store.saved = [];
    loadMock.mockImplementation(async () => store.cfg);
  });

  // ── paths.ts ──────────────────────────────────────────────────────────
  it("★ 数据目录交本机后端的偏好：只点默认那一家；清除 ＝ 空串（后端那一形里空串删那一家），填的去掉两侧空白；不碰 config.json", async () => {
    prefs.sets = [];
    await setClaudeDirOverride(null);
    await setClaudeDirOverride("  /b  ");
    expect(prefs.sets).toEqual([
      [LOCAL_ORIGIN, { agentHomes: { [DEFAULT_AGENT]: "" } }],
      [LOCAL_ORIGIN, { agentHomes: { [DEFAULT_AGENT]: "/b" } }],
    ]);
    expect(store.saved, "数据目录还写进了 config.json").toEqual([]);
    prefs.homes = {};
    expect(await readClaudeDir()).toBeNull();
    prefs.homes = { [DEFAULT_AGENT]: "/srv/c" };
    expect(await readClaudeDir()).toBe("/srv/c");
  });

  // ── keybindings/store.ts ─────────────────────────────────────────────
  it("显式解绑（null）与「没设」必须分得开", async () => {
    store.cfg = { keybindings: { "tab.next": "Ctrl+Tab", "tab.close-archived": null } };
    const got = await getKeybindings();
    expect(got["tab.next"]).toBe("Ctrl+Tab");
    expect(
      "tab.close-archived" in got,
      "显式解绑那一条被当成「没设」丢掉了 —— 用户解绑的键会**自己长回来**",
    ).toBe(true);
    expect(got["tab.close-archived"]).toBeNull();
  });

  it("未知 id 与脏类型都丢掉，但不影响同批里合法的那些", async () => {
    store.cfg = {
      keybindings: { "tab.next": "Ctrl+Tab", "gone.action": "X", "tab.close-archived": 42 },
    };
    const got = await getKeybindings();
    expect(got, "脏数据把整批合法覆盖一起带走了").toEqual({ "tab.next": "Ctrl+Tab" });
  });

  it("★★ 写自己的字段不许动别人的", async () => {
    store.cfg = { theme: { bg: "#000" }, other: 1 };
    await setKeybindings({ "tab.next": "Ctrl+Tab" });
    expect(
      store.cfg,
      "写 keybindings 时把 config 顶层别的字段冲掉了 —— 主题与数据目录会一起没，" +
        "而且**是静默的**：用户下次启动才发现设置回到了默认。",
    ).toEqual({ theme: { bg: "#000" }, other: 1, keybindings: { "tab.next": "Ctrl+Tab" } });
  });
});

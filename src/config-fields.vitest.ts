/**
 * **config.json 顶层字段的读写契约**〔audit-0805 §5 2c〕。
 *
 * `paths.ts`（`claudeDir`）与 `keybindings/store.ts`（`keybindings`）是同一个形状：
 * 都往同一份 `config.json` 的**顶层**读写一个字段。两者今天都是 **0% 覆盖**。
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
 * | `paths.ts` | 读配置抛异常 ⇒ 悄悄回退默认目录 ⇒ 用户看到的是「我的会话都不见了」 |
 * | `keybindings/store.ts` | 写覆盖时把 config 顶层别的字段冲掉 ⇒ 主题/数据目录一起没了 |
 *
 * ⚠ 本组**不追覆盖率数字**（同 2v 那轮的判准）：钉的是「错了会不会静默」那一类。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

const store = vi.hoisted(() => ({ cfg: {} as Record<string, unknown>, saved: [] as unknown[] }));

vi.mock("./config", () => ({
  loadConfig: vi.fn(async () => store.cfg),
  saveConfig: vi.fn(async (v: unknown) => {
    store.saved.push(v);
    store.cfg = v as Record<string, unknown>;
  }),
}));
vi.mock("./keybindings/actions", () => ({
  findAction: (id: string) => (["tab.next", "tab.close-archived"].includes(id) ? { id } : undefined),
}));

import { getClaudeDirOverride, setClaudeDirOverride } from "./paths";
import { getKeybindings, setKeybindings } from "./keybindings/store";
import { loadConfig } from "./config";

const loadMock = loadConfig as unknown as ReturnType<typeof vi.fn>;

describe("config.json 顶层字段的读写契约（§5 2c：挑「错了会静默」的那两个）", () => {
  beforeEach(() => {
    store.cfg = {};
    store.saved = [];
    loadMock.mockImplementation(async () => store.cfg);
  });

  // ── paths.ts ──────────────────────────────────────────────────────────
  it("★ `claudeDir` 读不出来时**留痕**，不是悄悄回退", async () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    loadMock.mockRejectedValueOnce(new Error("config 坏了"));
    expect(await getClaudeDirOverride()).toBeNull();
    expect(
      warn.mock.calls.flat().join(" "),
      "读配置失败时静默回退默认目录 —— 用户看到的是「我的会话都不见了」，" +
        "而没有任何东西说得出为什么（定框 E4：静默失败要给身份）。",
    ).toContain("claudeDir");
    warn.mockRestore();
  });

  it("空白值等于没设，不是空串", async () => {
    store.cfg = { claudeDir: "   " };
    expect(
      await getClaudeDirOverride(),
      "空白串被当成了有效目录 —— 下游会拿它去拼路径，读到一个不存在的地方",
    ).toBeNull();
  });

  it("★ 清除覆盖要**删字段**，不是写空串", async () => {
    store.cfg = { claudeDir: "/a", theme: { bg: "#000" } };
    await setClaudeDirOverride(null);
    expect(
      Object.prototype.hasOwnProperty.call(store.cfg, "claudeDir"),
      "清除时写了空串而不是删字段 —— 之后 `getClaudeDirOverride` 仍返回 null 看着没事，" +
        "但**后端**读 config 时拿到的是一个空字符串字段，与「没设」不是一回事。",
    ).toBe(false);
    expect(store.cfg.theme, "删字段时把别的顶层字段一起弄丢了").toEqual({ bg: "#000" });
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

  // ── 两者共用的那条契约 ────────────────────────────────────────────────
  it("★★ 写自己的字段不许动别人的（两个模块共用的契约）", async () => {
    store.cfg = { theme: { bg: "#000" }, claudeDir: "/a", other: 1 };
    await setKeybindings({ "tab.next": "Ctrl+Tab" });
    expect(
      store.cfg,
      "写 keybindings 时把 config 顶层别的字段冲掉了 —— 主题与数据目录会一起没，" +
        "而且**是静默的**：用户下次启动才发现设置回到了默认。",
    ).toEqual({ theme: { bg: "#000" }, claudeDir: "/a", other: 1, keybindings: { "tab.next": "Ctrl+Tab" } });

    await setClaudeDirOverride("/b");
    expect(store.cfg.keybindings, "写 claudeDir 时把 keybindings 冲掉了").toEqual({
      "tab.next": "Ctrl+Tab",
    });
  });
});

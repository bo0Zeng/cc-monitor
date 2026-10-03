/**
 * **配置落盘失败要出声**（E §3.3 吞错普查：分组 / 固定 / 顺序 / 行为设置 / 快捷键 落盘失败只打 console）。
 *
 * 守的要求（住址）：`INVARIANTS §12`「**关键失败必须** …… 状态栏 toast 红色 3-5s 提示」。
 * ⚠ 与「先改内存再落盘，落盘失败只记日志」相反 —— 按 §12 改成出声
 *（后果是「重启后分组 / 固定 / 顺序没了」，正是用户看不见的那一类损失）；那句由 DD2 改写。
 *
 * 形状：每一处写者注入一次失败 ⇒ toast **恰好一条**、标题 == 文案表里那句（期望从表里取 —— 表是对外文案的唯一来源，
 * 这里要钉的是「出了、出的是这一句」，不是那句话本身怎么写）。反面对照：写成功 ⇒ 零条。
 * 历史浏览器的星标 / 改名 / 隐藏三处在 `tests/frontend/ui/views/history-actions.vitest.ts`（那边有现成的行替身）。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

const fail = vi.hoisted(() => ({ on: true }));
const boom = async (): Promise<void> => {
  if (fail.on) throw new Error("盘写不进去");
};

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(), Channel: class {} }));
vi.mock("@tauri-apps/api/event", () => ({ emit: vi.fn(), listen: vi.fn(async () => () => {}) }));
vi.mock("../../../src/frontend/ui/error-toast", () => ({ showActionFailureToast: vi.fn() }));
// 分组的每一个动作把组表与几个 tab 的组 id 键装进一次 `patchConfig`（`TabBarPrefs.writeGroups`）⇒ 失败注入在写口这一层。
//   别的几个写者在下面各自被替掉，不经这里。
vi.mock("../../../src/frontend/ui/config", async (orig) => ({
  ...(await orig<Record<string, unknown>>()),
  patchConfig: vi.fn(() => boom()),
}));
vi.mock("../../../src/frontend/ui/tab-bar-state", async (orig) => ({
  ...(await orig<Record<string, unknown>>()),
  setPinned: vi.fn(() => boom()),
  setTabOrder: vi.fn(() => boom()),
}));
vi.mock("../../../src/frontend/ui/behavior", async (orig) => ({
  ...(await orig<Record<string, unknown>>()),
  setBehavior: vi.fn(() => boom()),
}));
vi.mock("../../../src/frontend/ui/keybindings/store", () => ({ setKeybindings: vi.fn(() => boom()) }));
vi.mock("../../../src/frontend/ui/keybindings/registry", () => ({
  dispatcher: { exportOverrides: () => ({}) },
  KeybindingDispatcher: class {},
}));

import { showActionFailureToast } from "../../../src/frontend/ui/error-toast";
import { copyText } from "../../../src/frontend/ui/copy-table";
import { TabBarPrefs } from "../../../src/frontend/ui/tab-bar-prefs";
import { KeybindingsEditor } from "../../../src/frontend/ui/keybindings/editor";
import { SettingsPanel } from "../../../src/frontend/ui/settings/panel";

/** 设置面板「行为」那一格：绕过构造（整个面板太重），只摆 `onBehaviorToggle` 读的那几样。 */
function behaviorPanel(): { onBehaviorToggle(): Promise<void> } {
  const cb = (): { checked: boolean } => ({ checked: false });
  const input = (): { value: string } => ({ value: "" });
  return Object.assign(Object.create(SettingsPanel.prototype) as object, {
    updateBringFrontEnabled: () => {},
    renderResumePresets: () => {},
    broadcastApplied: () => {},
    autoFollowCheckbox: cb(),
    bringFrontCheckbox: cb(),
    showBgCheckbox: cb(),
    notifyTurnEndCheckbox: cb(),
    resumeLocalInput: input(),
    resumeRemoteInput: input(),
    resumeLocalPresets: [],
    resumeRemotePresets: [],
    showBgOriginal: false,
  }) as unknown as { onBehaviorToggle(): Promise<void> };
}

const toast = vi.mocked(showActionFailureToast);

function prefs(): TabBarPrefs {
  const store = { orderedIds: [], tabs: new Map(), savedOrder: [], mergedOrder: () => [] };
  const host = { refreshTabBar: () => {} };
  return new TabBarPrefs(store as never, host as never);
}

/** 栏里一个 tab 在组 `g` 里 —— 解散它就是一次分组写。 */
function groupedPrefs(): TabBarPrefs {
  const store = { orderedIds: ["s"], tabs: new Map([["s", { sessionId: "s", group: "g" }]]), savedOrder: [] };
  const p = new TabBarPrefs(store as never, { refreshTabBar: () => {} } as never);
  p.collections = [{ id: "g", name: "n" }];
  return p;
}

const CASES: readonly { name: string; run: () => Promise<void>; head: string }[] = [
  { name: "分组", run: () => groupedPrefs().dissolveGroup("g"), head: copyText("tabBar.persist.collectionsFailed") },
  { name: "固定", run: () => prefs().persistPinned(), head: copyText("tabBar.persist.pinnedFailed") },
  { name: "顺序", run: () => prefs().persistOrder(), head: copyText("tabBar.persist.orderFailed") },
  { name: "行为设置", run: () => behaviorPanel().onBehaviorToggle(), head: copyText("settings.behavior.saveFailed") },
  {
    name: "快捷键",
    run: () =>
      (Object.create(KeybindingsEditor.prototype) as { persist(): Promise<void> }).persist(),
    head: copyText("keybindings.persist.failed"),
  },
];

describe("CFG1 J8 · 配置落盘失败出声", () => {
  beforeEach(() => {
    toast.mockClear();
    vi.spyOn(console, "warn").mockImplementation(() => {});
  });

  for (const c of CASES) {
    it(`${c.name}：写失败 ⇒ 恰好一条 toast、标题是表里那句、原因带着；写成功 ⇒ 零条`, async () => {
      fail.on = true;
      await c.run();
      expect(toast.mock.calls.map((a) => a[0])).toEqual([c.head]);
      expect(String(toast.mock.calls[0]![1])).toContain("盘写不进去");
      toast.mockClear();
      fail.on = false;
      await c.run();
      expect(toast).not.toHaveBeenCalled();
    });
  }
});

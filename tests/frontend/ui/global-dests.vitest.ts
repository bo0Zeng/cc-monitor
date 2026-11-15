/**
 * 栏顶入口清单（`global-dests.ts`）：能上栏顶的只这一张清单，缺省那一排只住这里；栏上没有的都进 ⋯（⋯ 不进清单）。
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

// config 写只交补丁；替身记下交出去的补丁（`sentEdits()`），不应用。
vi.mock("../../../src/frontend/ui/config", async (orig) => (await import("./config-patch-fake")).mockedConfigModule(orig));

import { loadConfig } from "../../../src/frontend/ui/config";
import { sentEdits } from "./config-patch-fake";
import { DEFAULT_HEAD, HEAD_ENTRIES, getTabBarHead, headOf, overflowOf, setTabBarHead } from "../../../src/frontend/ui/global-dests";

const mockLoad = vi.mocked(loadConfig);

describe("栏顶入口清单", () => {
  it("★ 缺省六颗照序：新建会话 · 存好的几组 · 历史 · 计划 · 文件 · 设置", () => {
    expect(DEFAULT_HEAD).toEqual(["new", "sets", "history", "plan", "files", "settings"]);
    expect(headOf(undefined)).toEqual([...DEFAULT_HEAD]);
    expect(headOf("不是表")).toEqual([...DEFAULT_HEAD]);
  });

  it("★ 清单就是能上栏顶的全部（并排监控 · 终端名单 · cc-bus 也在）；⋯ 不在清单里", () => {
    expect(HEAD_ENTRIES.map((e) => e.id)).toEqual(["new", "sets", "history", "plan", "files", "settings", "grid", "terminals", "cc-bus"]);
    expect(HEAD_ENTRIES.map((e) => e.id)).not.toContain("more");
  });

  it("★ 存的那一排：照序；认不得的丢、重复的只留头一个；空表 ⇒ 一颗都没有", () => {
    expect(headOf(["files", "不认得", "new", "files", 7])).toEqual(["files", "new"]);
    expect(headOf([])).toEqual([]);
  });

  it("★ 不在栏上的照清单的先后进 ⋯", () => {
    expect(overflowOf(["files", "new"])).toEqual(["sets", "history", "plan", "settings", "grid", "terminals", "cc-bus"]);
    expect(overflowOf(DEFAULT_HEAD)).toEqual(["grid", "terminals", "cc-bus"]);
  });
});

describe("栏顶那一排落盘（`tabBar.head`）：照存的摆、没存 ⇒ 缺省；编辑只写这一个键、恢复缺省删这个键", () => {
  beforeEach(() => vi.clearAllMocks());

  it("没存 · 段坏了 · 读失败 ⇒ 缺省那六颗", async () => {
    mockLoad.mockResolvedValue({} as never);
    await expect(getTabBarHead()).resolves.toEqual([...DEFAULT_HEAD]);
    mockLoad.mockResolvedValue({ tabBar: "坏的" } as never);
    await expect(getTabBarHead()).resolves.toEqual([...DEFAULT_HEAD]);
    mockLoad.mockRejectedValue(new Error("读不了"));
    await expect(getTabBarHead()).resolves.toEqual([...DEFAULT_HEAD]);
  });

  it("存了 ⇒ 照它的先后；认不得的丢、重复的只留头一个；存了空表 ⇒ 一颗都没有（不是缺省）", async () => {
    mockLoad.mockResolvedValue({ tabBar: { head: ["plan", "不认得", "new", "plan", 3] } } as never);
    await expect(getTabBarHead()).resolves.toEqual(["plan", "new"]);
    mockLoad.mockResolvedValue({ tabBar: { head: [] } } as never);
    await expect(getTabBarHead()).resolves.toEqual([]);
  });

  it("🔴 编辑只交 `tabBar.head` 这一格；恢复缺省 ＝ 删这一格", async () => {
    await setTabBarHead(["files", "new"]);
    await setTabBarHead(null);
    expect(sentEdits()).toEqual([
      { op: "set", path: ["tabBar", "head"], value: ["files", "new"] },
      { op: "remove", path: ["tabBar", "head"] },
    ]);
  });
});

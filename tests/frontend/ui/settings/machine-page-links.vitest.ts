// 机器单页卡头：怎么连过去的那一句照成品的 `route` 原样（有才摆、不拼字）；每条连接的读数不上卡头，在 ⋯ ›「复制详情」里（照成品 `detail` 原样）。
import { describe, it, expect, vi, beforeEach } from "vitest";

const clip = vi.hoisted(() => ({ writes: [] as string[] }));
vi.mock("../../../../src/frontend/ui/clipboard", () => ({
  writeClipboard: async (t: string) => {
    clip.writes.push(t);
  },
}));
type Item = { label: string; onClick?: () => void; divider?: boolean };
const menus = vi.hoisted(() => ({ last: [] as Item[] }));
vi.mock("../../../../src/frontend/ui/kit/menu", () => ({
  openMenu: (_at: unknown, items: Item[]) => {
    menus.last = items;
  },
}));

import { buildMachinePage } from "../../../../src/frontend/ui/settings/machine-page";
import { machineFace, type MachineState } from "../../../../src/frontend/ui/settings/machine-state";
import { copyText } from "../../../../src/frontend/ui/copy-table";

const up = (more: Partial<MachineState> = {}): MachineState => ({
  name: "deep",
  offline: false,
  state: "up",
  reason: null,
  stage: null,
  version: null,
  versionRelation: "same",
  os: null,
  word: "",
  said: "",
  offlineSaid: "deep 离线（夹具）",
  fixes: [],
  seenHostKey: null,
  expectedHostKey: null,
  detail: null,
  ...more,
});

const page = () => buildMachinePage({ pageId: "machine:deep", name: "deep", meta: "", ccMonitor: [], menu: () => [{ label: "连接设置（夹具）" }] });
const routeEl = (p: ReturnType<typeof page>) => p.element.querySelector<HTMLElement>(".machine-route")!;
const openMore = (p: ReturnType<typeof page>): Item[] => {
  p.element.querySelector<HTMLButtonElement>(`button[aria-label="${copyText("machinePage.head.more")}"]`)!.click();
  return menus.last;
};

beforeEach(() => {
  clip.writes.length = 0;
  menus.last = [];
});

describe("机器页 · 卡头只写怎么连过去的", () => {
  it("★ 经跳板 ⇒ 卡头一行，字照成品 `route` 原样；直连（没有 route）⇒ 那一行收起", () => {
    const p = page();
    p.setMachine(machineFace(up({ route: "经跳板 edge · 经代理（夹具）" })));
    expect(routeEl(p).hidden).toBe(false);
    expect(routeEl(p).textContent).toBe("经跳板 edge · 经代理（夹具）");
    p.setMachine(machineFace(up()));
    expect(routeEl(p).hidden).toBe(true);
    expect(routeEl(p).textContent).toBe("");
  });

  it("★ 卡头里不出每条连接的读数（复制详情里那一段不上卡头）", () => {
    const p = page();
    p.setMachine(machineFace(up({ detail: "连接：\n  deep.test:22 · 压缩 · 收 1 KB · 发 2 KB（夹具）" })));
    expect(p.element.querySelector(".machine-head")!.textContent).not.toContain("收 1 KB");
    expect(p.element.querySelector(".machine-links")).toBeNull();
  });

  it("★ ⋯ 菜单：成品有 detail ⇒ 末尾「复制详情」，点了照原样复制；没有 ⇒ 不出这一项", async () => {
    const p = page();
    p.setMachine(machineFace(up({ detail: "连接：\n  一句（夹具）" })));
    const items = openMore(p);
    expect(items[0].label).toBe("连接设置（夹具）");
    const copy = items.at(-1)!;
    expect(copy.label).toBe(copyText("detail.act.copy"));
    copy.onClick!();
    await Promise.resolve();
    expect(clip.writes).toEqual(["连接：\n  一句（夹具）"]);
    p.setMachine(machineFace(up()));
    expect(openMore(p).map((i) => i.label)).toEqual(["连接设置（夹具）"]);
  });

  it("★ 画法不拼字：face 里那一句就是 `route` 本身", () => {
    expect(machineFace(up({ route: "经跳板 a → b（夹具）" })).route).toBe("经跳板 a → b（夹具）");
    expect(machineFace(up()).route).toBe("");
  });
});

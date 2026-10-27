// 机器单页卡头：到那台的每一条连接一行，字照成品的 `links[].said` 原样（地址 · 经没经代理 · 压没压 · 收发字节都是后端写好的），不拼字。
import { describe, it, expect } from "vitest";
import { buildMachinePage } from "../../../../src/frontend/ui/settings/machine-page";
import { machineFace, type MachineState } from "../../../../src/frontend/ui/settings/machine-state";
import type { MachineLink } from "../../../../src/frontend/ui/generated/MachineLink";

const link = (endpoint: string, jump: boolean, said: string): MachineLink => ({
  endpoint,
  jump,
  compressed: !jump,
  viaProxy: jump,
  bytesIn: 4096,
  bytesOut: 512,
  said,
});

const up = (links?: MachineLink[]): MachineState => ({
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
  ...(links ? { links } : {}),
});

const rows = (page: ReturnType<typeof buildMachinePage>) =>
  [...page.element.querySelectorAll<HTMLElement>(".machine-links > .machine-link")].map((r) => r.textContent);

describe("机器页 · 每跳读数", () => {
  it("★ 经跳板连着 ⇒ 卡头下出两行（那台一行、跳板一行），字照成品原样、先后照成品", () => {
    const page = buildMachinePage({ pageId: "machine:deep", name: "deep", meta: "", ccMonitor: [], menu: () => [] });
    const target = link("deep.test:22", false, "那台那一句（夹具）");
    const jump = link("edge.test:22", true, "跳板那一句（夹具）");
    page.setMachine(machineFace(up([target, jump])));
    expect(rows(page)).toEqual([target.said, jump.said]);
    expect(page.element.querySelector<HTMLElement>(".machine-links")!.hidden).toBe(false);
  });

  it("★ 成品没带连接 ⇒ 那一块收起；连接没了再来一份 ⇒ 旧的几行清掉", () => {
    const page = buildMachinePage({ pageId: "machine:deep", name: "deep", meta: "", ccMonitor: [], menu: () => [] });
    page.setMachine(machineFace(up([link("deep.test:22", false, "一句（夹具）")])));
    expect(rows(page)).toHaveLength(1);
    page.setMachine(machineFace(up()));
    expect(rows(page)).toEqual([]);
    expect(page.element.querySelector<HTMLElement>(".machine-links")!.hidden).toBe(true);
  });

  it("★ 画法不拼字：face 里那几行就是 `said` 本身", () => {
    const ls = [link("a:22", false, "甲（夹具）"), link("b:22", true, "乙（夹具）")];
    expect(machineFace(up(ls)).links).toEqual(["甲（夹具）", "乙（夹具）"]);
    expect(machineFace(up()).links).toEqual([]);
  });
});

// 命令面板「机器」那一块怎么从后端回包摆出来：动作几样照回包（灰 · 副文 · 提醒原样带）、会话按那台挑、第几条选中、空输入每台一行。
import { describe, it, expect, vi } from "vitest";

vi.mock("../../../src/frontend/ui/ipc/chan-caller", () => ({ budgetWithin: () => 0, callTyped: vi.fn() }));
vi.mock("../../../src/frontend/ui/open-settings", () => ({ openSettingsWindow: vi.fn(async () => {}) }));

import { machineBlock, machineRows, type MachineActions } from "../../../src/frontend/ui/machine-commands";
import type { Command } from "../../../src/frontend/ui/views/command-bar";
import type { MachineMenu } from "../../../src/frontend/ui/generated/MachineMenu";
import type { MachineState } from "../../../src/frontend/ui/generated/MachineState";
import { copyText } from "../../../src/frontend/ui/copy-table";

const state = (name: string, over: Partial<MachineState> = {}): MachineState => ({
  name,
  state: "up",
  offline: false,
  reason: null,
  stage: null,
  version: null,
  versionRelation: null,
  os: "Linux",
  word: "",
  said: "",
  offlineSaid: `${name} w-off`,
  fixes: [],
  seenHostKey: null,
  expectedHostKey: null,
  detail: null,
  ...over,
});

const ses = (id: string, origin: string): Command => ({
  id,
  title: id,
  session: { dot: "running", dotLabel: "", machine: origin, project: null, title: id, waiting: null, origin },
  run: vi.fn(),
});

const to = (): MachineActions => ({ openTerminal: vi.fn(), newSession: vi.fn(), files: vi.fn(), status: vi.fn(), addMachine: vi.fn() });

const menu: MachineMenu = {
  from: "table",
  head: state("nas", { state: "down", offline: true, word: "w-down", said: "said-down" }),
  actions: [
    { act: "open_terminal", sub: "ssh nas · 登录 shell", blocked: null, note: "终端照开" },
    { act: "new_session", sub: null, blocked: "nas w-down", note: null },
    { act: "files", sub: "sub-home", blocked: "nas w-down", note: null },
    { act: "machine_status", sub: "设置 → 机器 → nas", blocked: null, note: null },
  ],
};

describe("machineBlock", () => {
  it("🔴 头照成品画、动作照回包（灰那一句 · 副文 · 提醒原样）、选中第一样能做的", () => {
    const acts = to();
    const b = machineBlock({ machine: "nas", from: "table", nth: null }, menu, [], acts);
    const [head, ...rest] = b.sections[0].items;
    expect(head.machine).toEqual({ dot: "failed", name: "nas", word: "w-down", said: "said-down", right: "Linux" });
    expect(rest.map((c) => [c.title, c.sub, c.disabled, c.note])).toEqual([
      [copyText("commandBar.machine.openTerminal"), "ssh nas · 登录 shell", undefined, "终端照开"],
      [copyText("commandBar.machine.newSession"), undefined, "nas w-down", undefined],
      [copyText("commandBar.machine.files"), "sub-home", "nas w-down", undefined],
      [copyText("commandBar.machine.status"), "设置 → 机器 → nas", undefined, undefined],
    ]);
    expect(b.select).toBe(rest[0].id);
    rest[0].run();
    expect(acts.openTerminal).toHaveBeenCalledWith("nas");
    rest[3].run();
    expect(acts.status).toHaveBeenCalledWith("nas");
  });

  it("这台的会话按那台挑、照给的先后编号；带了第几条 ⇒ 选中那一条，超出 ⇒ 不选会话", () => {
    const sessions = [ses("a", "nas"), ses("x", "devbox"), ses("b", "nas")];
    const b = machineBlock({ machine: "nas", from: "table", nth: 2 }, menu, sessions, to());
    expect(b.sections[1].title).toBe(copyText("commandBar.group.sessions"));
    expect(b.sections[1].items.map((c) => [c.id, c.hint])).toEqual([
      ["a", "1"],
      ["b", "2"],
    ]);
    expect(b.select).toBe("b");
    expect([...b.claimed]).toEqual(["a", "b"]);
    const far = machineBlock({ machine: "nas", from: "table", nth: 7 }, menu, sessions, to());
    expect(far.select).toBe(far.sections[0].items[1].id);
  });

  it("只在 ssh 配置里的：头说未加入、右侧写出处", () => {
    const b = machineBlock(
      { machine: "backup", from: "ssh_config", nth: null },
      { from: "ssh_config", head: null, actions: [{ act: "open_terminal", sub: "ssh backup · 登录 shell", blocked: null, note: null }, { act: "add_machine", sub: "add-where", blocked: null, note: null }] },
      [],
      to(),
    );
    expect(b.sections[0].items[0].machine).toMatchObject({ name: "backup", word: copyText("commandBar.machine.unlisted"), right: copyText("commandBar.machine.fromSshConfig") });
    expect(b.sections).toHaveLength(1);
  });
});

describe("machineRows", () => {
  it("每台一行，本机在前；词照成品；Enter 填名字", () => {
    const rows = machineRows({ nas: state("nas"), "<local>": state("here"), lab: state("lab", { state: "down", word: "w-fail" }) });
    expect(rows.map((r) => [r.title, r.sub, r.dot, r.fill, r.group])).toEqual([
      ["here", undefined, "up", "here", "machine"],
      ["nas", undefined, "up", "nas", "machine"],
      ["lab", "w-fail", "failed", "lab", "machine"],
    ]);
  });
});

describe("加入机器表…（A2-17-14-1）", () => {
  it("开设置窗机器页、带上打的那个名字（添加框开着、名字预填）", async () => {
    const { openSettingsWindow } = await import("../../../src/frontend/ui/open-settings");
    const { machineActions } = await import("../../../src/frontend/ui/machine-commands");
    const { addMachineOf } = await import("../../../src/frontend/ui/settings-dest");
    machineActions.addMachine("backup");
    expect(vi.mocked(openSettingsWindow)).toHaveBeenCalledWith(undefined, addMachineOf("backup"));
    expect(addMachineOf("backup")).toEqual({ page: "machines", anchor: "add-machine:backup" });
  });
});

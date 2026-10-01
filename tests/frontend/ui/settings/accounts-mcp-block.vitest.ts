// 账号页「各账号共用的 MCP」那一块（设计 96 §8.8 第 3 · 6 条：删除只在 cc-monitor 里做 · 新开的会话才生效 · 两边都改了让用户挑）。
// 界面只画那台后端答的名字与各版、只交意图：删哪一条 · 用哪一版（`from`）。判据看问了哪台、交了什么。
import { describe, it, expect, vi, beforeEach } from "vitest";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invokeMock(...a) }));
const askConfirmMock = vi.fn();
vi.mock("../../../../src/frontend/ui/ask-dialog", () => ({ askConfirm: (...a: unknown[]) => askConfirmMock(...a) }));
vi.mock("../../../../src/frontend/ui/error-toast", () => ({ showActionFailureToast: vi.fn() }));

import { chanArgsJson, chanReply, type ChanCallArgs } from "../../../test-support/chan-fake";
import { renderSharedMcp } from "../../../../src/frontend/ui/settings/accounts-mcp-block";
import { decodeAccountMcpView } from "../../../../src/frontend/ui/account-ops";
import { copyText } from "../../../../src/frontend/ui/copy-table";

const VIEW = {
  enabled: true,
  servers: ["anysearch", "cclsp"],
  conflicts: [
    {
      name: "cclsp",
      choices: [
        { from: null, holders: ["q"], gone: false },
        { from: "z", holders: ["z", "b"], gone: false },
      ],
    },
  ],
  changed: [],
  notes: ["b 号的配置读不出来，这一次不同步它。"],
};

const calls: Array<[string, string, unknown]> = [];

beforeEach(() => {
  calls.length = 0;
  invokeMock.mockReset();
  askConfirmMock.mockReset();
  askConfirmMock.mockResolvedValue(true);
  invokeMock.mockImplementation((cmd: string, args: ChanCallArgs) => {
    if (cmd !== "chan_call") return Promise.resolve(undefined);
    calls.push([args.op, args.origin, chanArgsJson(args)]);
    const after = args.op === "accounts-mcp-read" ? VIEW : { ...VIEW, conflicts: [], changed: ["q"] };
    return Promise.resolve(chanReply(after));
  });
});

async function settle(): Promise<void> {
  for (let i = 0; i < 5; i++) await Promise.resolve();
  await new Promise((r) => setTimeout(r, 0));
}

function buttons(el: HTMLElement): HTMLButtonElement[] {
  return Array.from(el.querySelectorAll("button"));
}

describe("各账号共用的 MCP", () => {
  it("写明删除只在这里做、新开的会话才生效，列出名字、冲突与提示", async () => {
    const el = renderSharedMcp("aya");
    await settle();
    const text = el.textContent ?? "";
    expect(text).toContain(copyText("accountsMcp.block.deleteHere"));
    expect(text).toContain(copyText("accountsMcp.block.newSessions"));
    const names = Array.from(el.querySelectorAll(".accounts-mcp-name")).map((n) => n.textContent);
    expect(names).toEqual(["anysearch", "cclsp"]);
    expect(text).toContain(copyText("accountsMcp.conflict.head", { name: "cclsp" }));
    expect(text).toContain(VIEW.notes[0]);
    expect(calls).toEqual([["accounts-mcp-read", "aya", {}]]);
  });

  it("删一条：先问，确认了才交名字", async () => {
    const el = renderSharedMcp("aya");
    await settle();
    askConfirmMock.mockResolvedValueOnce(false);
    const del = buttons(el).filter((b) => b.textContent === copyText("accountsMcp.list.remove"));
    del[0].click();
    await settle();
    expect(calls.map((c) => c[0])).toEqual(["accounts-mcp-read"]);
    del[0].click();
    await settle();
    expect(calls[1]).toEqual(["accounts-mcp-remove", "aya", { name: "anysearch" }]);
  });

  it("挑一版：共享那一版不带 from，某个号那一版带它的名字", async () => {
    const el = renderSharedMcp("aya");
    await settle();
    const fromZ = buttons(el).find((b) => b.textContent === copyText("accountsMcp.conflict.from", { holders: ["z", "b"].join(copyText("accountsMcp.list.sep")) }));
    const shared = buttons(el).find((b) => b.textContent === copyText("accountsMcp.conflict.shared"));
    fromZ!.click();
    await settle();
    expect(calls[1]).toEqual(["accounts-mcp-pick", "aya", { name: "cclsp", from: "z" }]);
    shared!.click();
    await settle();
    expect(calls[2]).toEqual(["accounts-mcp-pick", "aya", { name: "cclsp" }]);
  });

  it("成品形状严格收：多一个键 / 选项缺一格 ⇒ 读不懂", () => {
    expect(decodeAccountMcpView(VIEW)).not.toBeNull();
    expect(decodeAccountMcpView({ ...VIEW, extra: 1 })).toBeNull();
    expect(decodeAccountMcpView({ ...VIEW, conflicts: [{ name: "x", choices: [{ from: null, holders: [] }] }] })).toBeNull();
  });
});

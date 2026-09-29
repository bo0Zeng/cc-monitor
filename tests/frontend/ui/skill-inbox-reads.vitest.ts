/**
 * 设计/05 §14.3：「成品的两侧对拍：界面按形状严格收（多一格 / 缺一格 / 类型不对 ⇒ 抛「两端契约对不上」，不猜）」。
 *
 * 〔MIG-3a〕收件箱三问（`skill-host-list` / `-read` / `-write`）改走通道：围栏在那台后端，这里按形状严格收、问对那台。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { copyText } from "../../../src/frontend/ui/copy-table";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import { decodeSkillViews, listSkills, readSkillFile, writeSkillFile } from "../../../src/frontend/ui/skill-inbox-reads";
import { chanArgsJson, chanReply, type ChanCallArgs } from "../../test-support/chan-fake";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
beforeEach(() => {
  invokeMock.mockReset();
});

const view = { id: "cc-bus", label: "cc-bus", missing_reason: null, instances: [], editable: ["/p/INBOX.txt"] };

describe("严格收", () => {
  it("缺席原因可空；缺格 / 多格 / 类型不对 ⇒ 抛", () => {
    expect(decodeSkillViews({ skills: [view] })).toEqual([view]);
    expect(decodeSkillViews({ skills: [{ ...view, missing_reason: "没装" }] })[0]?.missing_reason).toBe("没装");
    const { editable: _drop, ...short } = view;
    expect(() => decodeSkillViews({ skills: [short] })).toThrow(copyText("skillInboxReads.reply.badShape"));
    expect(() => decodeSkillViews({ skills: [{ ...view, extra: 1 }] })).toThrow(copyText("skillInboxReads.reply.badShape"));
    expect(() => decodeSkillViews({ skills: [{ ...view, instances: [1] }] })).toThrow(copyText("skillInboxReads.reply.badShape"));
    expect(() => decodeSkillViews({ skills: [view], more: 1 })).toThrow(copyText("skillInboxReads.reply.badShape"));
  });
  it("读回多一格 ⇒ 抛", async () => {
    invokeMock.mockResolvedValueOnce(chanReply({ text: "t", extra: 1 }));
    await expect(readSkillFile("aya", "/p", "planned-build", "/p/INBOX.txt")).rejects.toThrow(copyText("skillInboxReads.reply.badShape"));
  });
});

describe("请求：问对那台、说对那条、参数原样", () => {
  it("三问各一发", async () => {
    invokeMock.mockResolvedValueOnce(chanReply({ skills: [] }));
    await listSkills("aya", "/p");
    invokeMock.mockResolvedValueOnce(chanReply({ text: "t" }));
    expect(await readSkillFile("<local>", "/p", "planned-build", "/p/INBOX.txt")).toBe("t");
    invokeMock.mockResolvedValueOnce(chanReply({ path: "/p/INBOX.txt" }));
    await writeSkillFile("aya", "/p", "planned-build", "/p/INBOX.txt", "new", "old");
    const calls = invokeMock.mock.calls.map((c) => c[1] as ChanCallArgs);
    expect(calls.map((a) => [a.origin, a.op, chanArgsJson(a)])).toEqual([
      ["aya", "skill-host-list", { cwd: "/p" }],
      ["<local>", "skill-host-read", { cwd: "/p", skillId: "planned-build", path: "/p/INBOX.txt" }],
      ["aya", "skill-host-write", { cwd: "/p", skillId: "planned-build", path: "/p/INBOX.txt", content: "new", expected: "old" }],
    ]);
  });
});

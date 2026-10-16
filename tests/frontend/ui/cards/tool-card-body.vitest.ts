// 工具卡展开画什么只照后端给的 `steps[id]`（`body`：改动 · 命令；`file`：可在文件窗口定位的那份文件），不读入参的键；
// 没有那一格 ⇒ 收着那一行只写工具名、展开照原文画入参。
import { describe, it, expect } from "vitest";
import { renderMessage, type RenderContext } from "../../../../src/frontend/ui/cards/index";
import { LOCAL_ORIGIN } from "../../../../src/frontend/ui/ipc/origin";
import type { LineRecord } from "../../../../src/frontend/ui/generated/LineRecord";
import type { ToolStep } from "../../../../src/frontend/ui/generated/ToolStep";
import { copyText } from "../../../../src/frontend/ui/copy-table";

function opened(input: unknown, step: ToolStep | undefined, origin: string = LOCAL_ORIGIN): HTMLDetailsElement {
  const rec = {
    agent: "claude",
    t: "reply",
    id: "a",
    at: "2026-01-01T02:02:00.000Z",
    blocks: [{ type: "tool_use", id: "t1", name: "Tool", input }],
    autoReply: false,
    ...(step ? { steps: { t1: step } } : {}),
  } as unknown as LineRecord;
  const ctx = { parentPath: "/p/s.jsonl", origin, toolUseNames: new Map(), toolUseElements: new Map(), pendingToolResults: new Map() };
  const r = renderMessage(rec, ctx as unknown as RenderContext);
  if (r.kind !== "steps") throw new Error(`期望几步，得到 ${r.kind}`);
  const d = r.units[0] as HTMLDetailsElement;
  d.open = true;
  d.dispatchEvent(new Event("toggle"));
  return d;
}
const step = (extra: Partial<ToolStep>): ToolStep => ({ tool: "Tool", known: true, ...extra });

describe("工具卡展开照 `steps[id]` 画", () => {
  it("★ 改动 ⇒ 行级 diff（几处各一段）；入参里有什么不看", () => {
    const d = opened({ zq: "IGNORED" }, step({ body: { kind: "changes", changes: [{ old: "a\nb", new: "a\nx" }, { old: "", new: "n" }] } }));
    const rows = [...d.querySelectorAll(".block-diff-line")].map((l) => `${l.querySelector(".block-diff-gutter")?.textContent}${l.querySelector(".block-diff-code")?.textContent}`);
    expect(rows).toEqual(["a", "-b", "+x", "+n"]);
    expect(d.querySelectorAll(".block-diff-edit-label")).toHaveLength(2);
    expect(d.textContent).not.toContain("IGNORED");
  });

  it("★ 空的新写画不出行 ⇒ 照原文画入参", () => {
    const d = opened({ zq: "RAW" }, step({ body: { kind: "changes", changes: [{ old: "", new: "" }] } }));
    expect(d.querySelector(".block-diff")).toBeNull();
    expect(d.querySelector(".block-body-json")?.textContent).toContain("RAW");
  });

  it("★ 命令 ⇒ 说明一行注 ＋ 整行命令（多行原样）；没说明只画命令", () => {
    const d = opened({}, step({ note: "找调用点", body: { kind: "command", line: 'rg -n "x" src\n| head' } }));
    expect(d.querySelector(".block-args")?.textContent).toBe(`${copyText("cards.command.note", { text: "找调用点" })}\nrg -n "x" src\n| head`);
    expect(opened({}, step({ body: { kind: "command", line: "ls" } })).querySelector(".block-args")?.textContent).toBe("ls");
  });

  it("★ 远端会话 ＋ `file` ⇒ 顶上一条「在文件窗口里定位」；本机 · 没有 `file` ⇒ 没有", () => {
    const link = (d: HTMLElement) => d.querySelector(".tool-file-link")?.textContent ?? null;
    expect(link(opened({}, step({ arg: "/w/a.ts", path: true, file: "/w/a.ts" }), "pi"))).toBe(copyText("cards.remoteFile.open", { path: "/w/a.ts" }));
    expect(link(opened({}, step({ arg: "a.ts", path: true }), "pi"))).toBeNull();
    expect(link(opened({}, step({ file: "/w/a.ts" })))).toBeNull();
  });

  it("★ 那一家没给 `steps` ⇒ 收着那一行只写工具名、展开照原文画入参", () => {
    const d = opened({ q: "RAW" }, undefined);
    expect(d.querySelector(".step-tool")?.textContent).toBe("Tool");
    expect(d.querySelector(".step-arg")?.textContent).toBe("");
    expect(d.querySelector(".block-body-json")?.textContent).toMatch(/"q": "RAW"/);
  });
});

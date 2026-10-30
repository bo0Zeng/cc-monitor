// 工具卡展开画什么只照后端给的 `steps[id]`（`body`：改动 · 命令；`file`：可在文件窗口定位的那份文件），不读入参的键；
// 没有那一格 ⇒ 收着那一行只写工具名、展开照原文画入参。
import { describe, it, expect } from "vitest";
import { renderMessage, type RenderContext } from "../../../../src/frontend/ui/cards/index";
import { LOCAL_ORIGIN } from "../../../../src/frontend/ui/ipc/origin";
import type { LineRecord } from "../../../../src/frontend/ui/generated/LineRecord";
import type { ToolStep } from "../../../../src/frontend/ui/generated/ToolStep";
import type { LineDiffRow } from "../../../../src/frontend/ui/generated/LineDiffRow";
import type { LineDiff } from "../../../../src/frontend/ui/generated/LineDiff";
import { copyText } from "../../../../src/frontend/ui/copy-table";

const row = (kind: LineDiffRow["kind"], text: string): LineDiffRow => ({ kind, text });
const lineDiff = (rows: LineDiffRow[]): LineDiff => ({ rows, shown: rows.length, truncated: false, added: 0, removed: 0, stat: "" });
const drawn = (d: HTMLElement): string[] =>
  [...d.querySelectorAll(".block-diff-line")].map((l) => `${l.querySelector(".block-diff-gutter")?.textContent}${l.querySelector(".block-diff-code")?.textContent}`);
const newCtx = () => ({ parentPath: "/p/s.jsonl", origin: LOCAL_ORIGIN, toolUseNames: new Map(), toolUseElements: new Map(), pendingToolResults: new Map() });

function opened(input: unknown, step: ToolStep | undefined, origin: string = LOCAL_ORIGIN, given = newCtx()): HTMLDetailsElement {
  const rec = {
    agent: "claude",
    t: "reply",
    id: "a",
    at: "2026-01-01T02:02:00.000Z",
    blocks: [{ type: "tool_use", id: "t1", name: "Tool", input }],
    autoReply: false,
    ...(step ? { steps: { t1: step } } : {}),
  } as unknown as LineRecord;
  const ctx = Object.assign(given, { origin });
  const r = renderMessage(rec, ctx as unknown as RenderContext);
  if (r.kind !== "steps") throw new Error(`期望几步，得到 ${r.kind}`);
  const d = r.units[0] as HTMLDetailsElement;
  d.open = true;
  d.dispatchEvent(new Event("toggle"));
  return d;
}
const step = (extra: Partial<ToolStep>): ToolStep => ({ tool: "Tool", known: true, ...extra });

describe("工具卡展开照 `steps[id]` 画", () => {
  it("★ 改动 ⇒ 照后端给的行画（段头 · 加 · 删 · 没变），不判行、不数数；入参里有什么不看", () => {
    const diff = lineDiff([row("head", "SEG1"), row("ctx", "a"), row("del", "b"), row("add", "x"), row("head", "SEG2"), row("add", "n")]);
    const d = opened({ zq: "IGNORED" }, step({ body: { kind: "changes", diff } }));
    expect(drawn(d)).toEqual(["a", "-b", "+x", "+n"]);
    expect([...d.querySelectorAll(".block-diff-edit-label")].map((l) => l.textContent)).toEqual(["SEG1", "SEG2"]);
    expect(d.textContent).not.toContain("IGNORED");
  });

  it("★ 收着先画后端说的前几行，其余点「显示全部改动」补上；按钮上的数照后端给的", () => {
    const diff = { ...lineDiff([row("add", "1"), row("add", "2"), row("add", "3")]), shown: 2, added: 7, removed: 5 };
    const d = opened({}, step({ body: { kind: "changes", diff } }));
    expect(drawn(d)).toEqual(["+1", "+2"]);
    const btn = d.querySelector<HTMLButtonElement>(".block-body-show-full");
    expect(btn?.textContent).toBe(copyText("diff.buildDiffBody.showAll", { addCount: 7, delCount: 5 }));
    btn?.click();
    expect(drawn(d)).toEqual(["+1", "+2", "+3"]);
    expect(d.querySelector(".block-body-show-full")).toBeNull();
  });

  it("★ 后端说没逐行比（`notice`）⇒ 行差顶上照摆那一句；没带 ⇒ 没有那一行", () => {
    const diff = { ...lineDiff([row("del", "a"), row("add", "b")]), notice: "NOTICE-FROM-BACKEND" };
    const d = opened({}, step({ body: { kind: "changes", diff } }));
    expect(d.querySelector(".block-diff-notice")?.textContent).toBe("NOTICE-FROM-BACKEND");
    expect(drawn(d)).toEqual(["-a", "+b"]);
    expect(opened({}, step({ body: { kind: "changes", diff: lineDiff([row("add", "x")]) } })).querySelector(".block-diff-notice")).toBeNull();
  });

  it("★ 结果带着行差 ⇒ 改画结果那一份（已画了调用那一份的当场换掉）", () => {
    const ctx = newCtx();
    const d = opened({}, step({ body: { kind: "changes", diff: lineDiff([row("add", "FROM-CALL")]) } }), LOCAL_ORIGIN, ctx);
    expect(drawn(d)).toEqual(["+FROM-CALL"]);
    const said = {
      agent: "claude",
      t: "said",
      id: "u",
      at: "2026-01-01T02:02:01.000Z",
      draw: "steps",
      who: { speaker: { kind: "toolResult" }, text: "" },
      blocks: [{ type: "tool_result", for: "t1", content: [{ type: "text", text: "ok", bytes: 2 }], isError: false, bytes: 2 }],
      results: { t1: { ok: true, text: "+1", patch: lineDiff([row("add", "FROM-RESULT")]) } },
    } as unknown as LineRecord;
    renderMessage(said, ctx as unknown as RenderContext);
    expect(drawn(d)).toEqual(["+FROM-RESULT"]);
  });

  it("★ 一行也没有 ⇒ 照原文画入参", () => {
    const d = opened({ zq: "RAW" }, step({ body: { kind: "changes", diff: lineDiff([]) } }));
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

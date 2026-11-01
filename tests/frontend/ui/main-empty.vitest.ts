/**
 * 主窗口空态（A2-17-6-2）：大字与下面那一行照摆本机后端合好的首屏那一句（`FleetLines` 的 `line` · `hint`），界面不数台、不按状态挑字；
 * 没来之前什么都不说（还没读到 ≠ 无会话）。本机终端没接上（`first-run` 里 `terminal` 那一步没打勾）才给［接上终端］；文案是纯字。
 */
import { describe, it, expect, vi } from "vitest";
import { MainEmpty } from "../../../src/frontend/ui/main-empty";
import { copyText } from "../../../src/frontend/ui/copy-table";
import type { Readiness } from "../../../src/frontend/ui/settings/readiness-reads";
import type { FleetLines } from "../../../src/frontend/ui/generated/FleetLines";

const ready = (terminalDone: boolean): Readiness => ({
  steps: [
    { id: "terminal", done: terminalDone, required: true },
    { id: "named", done: false, required: false },
  ],
  left: terminalDone ? 0 : 1,
  skipped: false,
});

const lines = (e: MainEmpty): string[] => [...(e.el.firstElementChild?.querySelectorAll(":scope > div") ?? [])].map((d) => d.textContent ?? "");

describe("主窗口空态", () => {
  it("★ 有台还在连 ⇒ 照摆那一句（「本机无会话」· 那台还在连），不说「无会话」；没来之前一句都不说", () => {
    const e = new MainEmpty(() => {});
    expect(lines(e), "还没读到 ≠ 无会话").toEqual([]);
    const face: FleetLines = { line: "核心·本机那一句", hint: "核心·gpu-01 还在连那一句", count: "核心·计数" };
    e.face(face);
    expect(lines(e)).toEqual([face.line, face.hint]);
  });

  it("没接上 ⇒ 多［接上终端］（点了直达设置）；接上了 · 问不到 ⇒ 不给；那一句照旧是后端那一句", () => {
    const connect = vi.fn();
    const e = new MainEmpty(connect);
    const face: FleetLines = { line: "核心·都报完了", hint: "核心·怎么让它有", count: "核心·计数" };
    e.face(face);
    expect(e.el.querySelector("button")).toBeNull();
    e.readiness(ready(false));
    expect(lines(e)).toEqual([face.line, face.hint]);
    const b = e.el.querySelector("button")!;
    expect(b.textContent).toBe(copyText("main.empty.connect"));
    b.click();
    expect(connect).toHaveBeenCalledTimes(1);
    e.readiness(ready(true));
    expect(e.el.querySelector("button"), "接上了不给按钮").toBeNull();
    e.readiness(ready(false));
    e.readiness(null);
    expect(e.el.querySelector("button"), "问不到不催").toBeNull();
  });

  it("文案是纯字：后端那一句里有尖括号也照字摆", () => {
    const e = new MainEmpty(() => {});
    e.face({ line: "<b>x</b>", hint: "<i>y</i>", count: "" });
    expect(e.el.querySelector("b, i")).toBeNull();
    expect(copyText("main.empty.connect")).not.toMatch(/[<>]/);
  });
});

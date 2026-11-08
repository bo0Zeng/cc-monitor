/**
 * 主窗口空态：大字与下面那一行照摆本机后端合好的首屏那一句（`FleetLines` 的 `line` · `hint`），界面不数台、不按状态挑字；
 * 没来之前什么都不说（还没读到 ≠ 无会话）。底下是「开始用」卡：字照后端那一份，问不到 · 跳过过 · 都做完不出。
 */
import { describe, it, expect, vi } from "vitest";
import { MainEmpty } from "../../../src/frontend/ui/main-empty";
import type { Readiness } from "../../../src/frontend/ui/settings/readiness-reads";
import type { FleetLines } from "../../../src/frontend/ui/generated/FleetLines";

const ready = (terminalDone: boolean, over: Partial<Readiness> = {}): Readiness => ({
  steps: [
    { id: "terminal", done: terminalDone, name: "核心·终端那步", hint: "核心·终端那句", act: { label: "核心·装上", go: "aliases" } },
    { id: "remote", done: false, name: "核心·远端那步", hint: "核心·远端那句", act: { label: "核心·添加", go: "add_machine" } },
  ],
  skipped: false,
  ...over,
});

const host = () => ({ open: vi.fn(), skip: vi.fn() });
const lines = (e: MainEmpty): string[] => [...(e.el.firstElementChild?.querySelectorAll(":scope > div") ?? [])].map((d) => d.textContent ?? "");
const card = (e: MainEmpty): HTMLElement | null => e.el.querySelector('[data-role="start-card"]');

describe("主窗口空态", () => {
  it("★ 有台还在连 ⇒ 照摆那一句，不说「无会话」；没来之前一句都不说、也不出卡", () => {
    const e = new MainEmpty(host());
    e.readiness(ready(false));
    expect(lines(e), "还没读到 ≠ 无会话").toEqual([]);
    expect(card(e)).toBeNull();
    const face: FleetLines = { line: "核心·本机那一句", hint: "核心·gpu-01 还在连那一句", count: "核心·计数", allListed: false };
    e.face(face);
    expect(lines(e)).toEqual([face.line, face.hint]);
  });

  it("★ 「开始用」卡：每步的名字 · 那一句 · 按钮的字照后端；点按钮按 `go` 开那一处；没做的才有按钮", () => {
    const h = host();
    const e = new MainEmpty(h);
    e.face({ line: "核心·都报完了", hint: "核心·怎么让它有", count: "", allListed: true });
    e.readiness(ready(true));
    const c = card(e)!;
    expect(c).not.toBeNull();
    const rows = [...c.querySelectorAll<HTMLElement>("[data-step]")];
    expect(rows.map((r) => r.dataset.step)).toEqual(["terminal", "remote"]);
    expect(rows[0]!.textContent).toContain("核心·终端那步");
    expect(rows[0]!.textContent).toContain("核心·终端那句");
    expect(rows[0]!.querySelector("[data-go]"), "做完的那步不给按钮").toBeNull();
    const b = rows[1]!.querySelector<HTMLButtonElement>("[data-go]")!;
    expect(b.textContent).toBe("核心·添加");
    b.click();
    expect(h.open).toHaveBeenCalledWith("add_machine");
    c.querySelector<HTMLButtonElement>('[data-action="skip"]')!.click();
    expect(h.skip).toHaveBeenCalledTimes(1);
  });

  it("问不到 · 跳过过 · 都做完 ⇒ 不出卡，那一句照旧", () => {
    const e = new MainEmpty(host());
    const face: FleetLines = { line: "核心·一句", hint: "核心·二句", count: "", allListed: false };
    e.face(face);
    e.readiness(null);
    expect(card(e), "问不到不催").toBeNull();
    e.readiness(ready(false, { skipped: true }));
    expect(card(e), "跳过过").toBeNull();
    const all = ready(true);
    all.steps[1]!.done = true;
    e.readiness(all);
    expect(card(e), "都做完").toBeNull();
    expect(lines(e)).toEqual([face.line, face.hint]);
  });

  it("文案是纯字：后端那一句里有尖括号也照字摆", () => {
    const e = new MainEmpty(host());
    e.face({ line: "<b>x</b>", hint: "<i>y</i>", count: "", allListed: false });
    e.readiness(ready(false, { steps: [{ id: "terminal", done: false, name: "<b>n</b>", hint: "<i>h</i>", act: { label: "<u>a</u>", go: "aliases" } }] }));
    expect(e.el.querySelector("b, i, u")).toBeNull();
  });
});

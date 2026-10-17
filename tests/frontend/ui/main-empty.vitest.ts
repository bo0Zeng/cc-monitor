/**
 * 主窗口空态：「无会话」＋ 怎么让它有；本机终端没接上（`first-run` 里 `terminal` 那一步没打勾）才多那一句与［接上终端］，
 * 接上了 / 问不到只有前两句、不给按钮；文案是纯字（不经 HTML 插）。
 */
import { describe, it, expect, vi } from "vitest";
import { MainEmpty } from "../../../src/frontend/ui/main-empty";
import { copyText } from "../../../src/frontend/ui/copy-table";
import type { Readiness } from "../../../src/frontend/ui/settings/readiness-reads";

const ready = (terminalDone: boolean): Readiness => ({
  steps: [
    { id: "terminal", done: terminalDone, required: true },
    { id: "named", done: false, required: false },
  ],
  left: terminalDone ? 0 : 1,
  skipped: false,
});

describe("主窗口空态", () => {
  it("★ 没接上 ⇒ 多那一句与［接上终端］（点了直达设置）；接上了 · 问不到 ⇒ 只有前两句", () => {
    const connect = vi.fn();
    const e = new MainEmpty(connect);
    const lines = (): string[] => [...e.el.firstElementChild!.querySelectorAll(":scope > div")].map((d) => d.textContent ?? "");
    expect(lines(), "问到之前不催").toEqual([copyText("main.empty.title"), copyText("main.empty.hint")]);
    expect(e.el.querySelector("button")).toBeNull();

    e.readiness(ready(false));
    expect(lines()).toEqual([copyText("main.empty.title"), copyText("main.empty.hintUnhooked")]);
    const b = e.el.querySelector("button")!;
    expect(b.textContent).toBe(copyText("main.empty.connect"));
    b.click();
    expect(connect).toHaveBeenCalledTimes(1);

    e.readiness(ready(true));
    expect(e.el.querySelector("button"), "接上了不给按钮").toBeNull();
    expect(lines()).toEqual([copyText("main.empty.title"), copyText("main.empty.hint")]);
    e.readiness(ready(false));
    e.readiness(null);
    expect(e.el.querySelector("button"), "问不到不催").toBeNull();
  });

  it("文案是纯字：几条都不带标签", () => {
    for (const k of ["main.empty.title", "main.empty.hint", "main.empty.hintUnhooked", "main.empty.connect"] as const) {
      expect(copyText(k), k).not.toMatch(/[<>]/);
    }
  });
});

/**
 * 栏顶那一排（`tab-bar-head.ts`）：照存的那一排摆、⋯ 恒在最右；⋯ 里是不在栏上的那几项（照清单先后）＋ 末一行「调整栏顶按钮…」。
 */
import { describe, expect, it, vi } from "vitest";
import { copyText } from "../../../src/frontend/ui/copy-table";
import { DEFAULT_HEAD, type HeadId } from "../../../src/frontend/ui/global-dests";
import { TabBarHead } from "../../../src/frontend/ui/tab-bar-head";

const ids = (els: HTMLElement[]): (string | undefined)[] => els.map((e) => e.dataset.head);

function rig() {
  const ran: string[] = [];
  const mounted: { buttons: HTMLElement[]; end: HTMLElement[] }[] = [];
  const edit = vi.fn();
  const run = Object.fromEntries(
    (["new", "sets", "history", "plan", "files", "settings", "grid", "terminals", "cc-bus"] as HeadId[]).map((id) => [id, () => ran.push(id)]),
  ) as unknown as Record<HeadId, (anchor: HTMLButtonElement) => void>;
  const head = new TabBarHead({
    run,
    chord: () => undefined,
    mount: (buttons, end) => mounted.push({ buttons, end }),
    edit,
  });
  return { head, ran, mounted, edit };
}

describe("栏顶那一排", () => {
  it("★ 缺省六颗照序摆，⋯ 恒在最右", () => {
    const r = rig();
    r.head.show(DEFAULT_HEAD);
    const last = r.mounted.at(-1)!;
    expect(ids(last.buttons)).toEqual(["new", "sets", "history", "plan", "files", "settings"]);
    expect(ids(last.end)).toEqual(["more"]);
  });

  it("★ 照存的那一排摆；点一颗跑那一项", () => {
    const r = rig();
    r.head.show(["files", "new"]);
    const last = r.mounted.at(-1)!;
    expect(ids(last.buttons)).toEqual(["files", "new"]);
    last.buttons[0]!.click();
    expect(r.ran).toEqual(["files"]);
  });

  it("★ ⋯ 里是不在栏上的那几项（照清单先后）＋ 末一行「调整栏顶按钮…」；一颗都不摆 ⇒ 全在 ⋯ 里、⋯ 照样在", () => {
    const r = rig();
    r.head.show(["files", "new"]);
    const items = r.head.moreItems();
    expect(items.filter((i) => i.id).map((i) => i.id)).toEqual(["sets", "history", "plan", "settings", "grid", "terminals", "cc-bus"]);
    const edit = items.at(-1)!;
    expect(edit.label).toBe(copyText("tabBarHead.edit.open"));
    edit.onClick?.();
    expect(r.edit).toHaveBeenCalledTimes(1);
    r.head.show([]);
    expect(ids(r.mounted.at(-1)!.buttons)).toEqual([]);
    expect(ids(r.mounted.at(-1)!.end)).toEqual(["more"]);
    expect(r.head.moreItems().filter((i) => i.id).length).toBe(9);
  });

  it("★ 栏顶右键 ⇒ 开编辑那一处（设置页「栏顶按钮」）：按钮上 · 按钮之间的空白都算；重摆不重挂", () => {
    const row = document.createElement("div");
    row.className = "tab-bar-head";
    document.body.replaceChildren(row);
    const edit = vi.fn();
    const head = new TabBarHead({
      run: Object.fromEntries(DEFAULT_HEAD.map((id) => [id, () => {}])) as unknown as Record<HeadId, (anchor: HTMLButtonElement) => void>,
      chord: () => undefined,
      mount: (buttons, end) => row.replaceChildren(...buttons, ...end),
      edit,
    });
    head.show(DEFAULT_HEAD);
    head.show(["files", "new"]);
    for (const at of [row.querySelector<HTMLElement>("[data-head]")!, row]) {
      const ev = new MouseEvent("contextmenu", { bubbles: true, cancelable: true });
      at.dispatchEvent(ev);
      expect(ev.defaultPrevented).toBe(true);
    }
    expect(edit).toHaveBeenCalledTimes(2);
  });
});

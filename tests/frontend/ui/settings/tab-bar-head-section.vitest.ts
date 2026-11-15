/**
 * 设置页「栏顶按钮」那一节：清单照存的那一排（没存 ⇒ 缺省）；勾一下 · 恢复缺省当场交写口（只写 `tabBar.head`，恢复缺省 ＝ 删它），
 * 写成了广播「设置已应用」（主窗口照它重摆）；写不上出声、不打断。
 */
import { describe, expect, it, vi } from "vitest";

const { toasts } = vi.hoisted(() => ({ toasts: [] as string[] }));
vi.mock("../../../../src/frontend/ui/kit/toast", () => ({ failToast: (t: string) => toasts.push(t), toast: vi.fn(), undoToast: vi.fn() }));

import { copyText } from "../../../../src/frontend/ui/copy-table";
import { DEFAULT_HEAD, type HeadId } from "../../../../src/frontend/ui/global-dests";
import { tabBarHeadSection } from "../../../../src/frontend/ui/settings/tab-bar-head-section";

const flush = async (): Promise<void> => {
  for (let i = 0; i < 8; i++) await Promise.resolve();
};
const checked = (el: HTMLElement): string[] =>
  [...el.querySelectorAll<HTMLElement>("[data-row]")].filter((r) => r.querySelector<HTMLInputElement>("input")?.checked && r.dataset.row !== "more").map((r) => r.dataset.row!);

describe("设置页「栏顶按钮」", () => {
  it("★ 清单照存的那一排；勾一个 ⇒ 写口收到新的一排、写成了广播；恢复缺省 ⇒ 写口收到 null", async () => {
    const writes: (HeadId[] | null)[] = [];
    const written = vi.fn();
    const el = tabBarHeadSection({ read: async () => ["files", "new"], write: async (ids) => void writes.push(ids === null ? null : [...ids]), written });
    document.body.replaceChildren(el);
    await flush();
    expect(el.dataset.anchor).toBe("tab-bar-head");
    expect(checked(el)).toEqual(["files", "new"]);
    el.querySelector<HTMLInputElement>('[data-row="plan"] input')!.click();
    await flush();
    expect(writes).toEqual([["files", "new", "plan"]]);
    expect(written).toHaveBeenCalledTimes(1);
    [...el.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === copyText("rot.cell.reset"))!.click();
    await flush();
    expect(writes.at(-1)).toBeNull();
    expect(checked(el)).toEqual([...DEFAULT_HEAD]);
  });

  it("★ 写不上 ⇒ 出声（不打断）、不广播", async () => {
    toasts.length = 0;
    const written = vi.fn();
    const el = tabBarHeadSection({ read: async () => [...DEFAULT_HEAD], write: () => Promise.reject(new Error("盘满")), written });
    document.body.replaceChildren(el);
    await flush();
    el.querySelector<HTMLInputElement>('[data-row="grid"] input')!.click();
    await flush();
    expect(toasts).toEqual([copyText("tabBarHead.edit.failed")]);
    expect(written).not.toHaveBeenCalled();
  });
});

/**
 * 「消息」一条带几颗动作时怎么排（界面小修 10-09 · C）：只有一颗 ⇒ 摆在句子那一行的行尾；
 * 两颗及以上 ⇒ 动作换到第二行（与 toast 同一种排法：句子一行、动作一行），句子拿整行宽。
 */
import { describe, it, expect, vi } from "vitest";
import { copyText } from "../../../src/frontend/ui/copy-table";
import { toast } from "../../../src/frontend/ui/kit/toast";
import { StatusMessages } from "../../../src/frontend/ui/status-messages";

const noop = (): void => {};

async function open(): Promise<void> {
  const m = new StatusMessages();
  document.body.replaceChildren(m.el);
  m.el.querySelector<HTMLElement>('[data-role="status-messages"]')!.click();
  await vi.waitFor(() => expect(document.querySelector('[data-role="messages-list"]')).not.toBeNull());
}

function rowOf(title: string): HTMLElement {
  const row = [...document.querySelectorAll<HTMLElement>("[data-sm-row]")].find((r) => r.textContent?.includes(title));
  expect(row, `「消息」里找不到 ${title} 那一行`).toBeDefined();
  return row!;
}

/** 句子那一行上的按钮（不在第二行里的）、第二行上的按钮。 */
function split(row: HTMLElement): { inline: string[]; below: string[] } {
  const below = row.querySelector<HTMLElement>("[data-sm-acts]");
  const all = [...row.querySelectorAll("button")];
  return {
    inline: all.filter((b) => !below?.contains(b)).map((b) => b.textContent ?? ""),
    below: below ? [...below.querySelectorAll("button")].map((b) => b.textContent ?? "") : [],
  };
}

describe("「消息」一条带几颗动作", () => {
  it("★ 只有一颗 ⇒ 摆在行尾，没有第二行", async () => {
    toast("单动作-1", "", { level: "info", action: { label: "撤-1", run: noop } });
    await open();
    expect(split(rowOf("单动作-1"))).toEqual({ inline: ["撤-1"], below: [] });
  });

  it("★ 两颗 ⇒ 都换到第二行，句子那一行不留按钮", async () => {
    toast("两动作-2", "", { action: [{ label: "甲-2", run: noop }, { label: "乙-2", run: noop }] });
    await open();
    expect(split(rowOf("两动作-2"))).toEqual({ inline: [], below: ["甲-2", "乙-2"] });
  });

  it("★ 一颗动作 ＋ 复制详情（［详情］［复制详情］）⇒ 三颗都在第二行，修法在前、详情在后", async () => {
    toast("带详情-3", "", { detail: "d-3", action: { label: "修-3", run: noop } });
    await open();
    const { inline, below } = split(rowOf("带详情-3"));
    expect(inline).toEqual([]);
    expect(below.slice(0, 2)).toEqual(["修-3", copyText("messages.record.expand")]);
    expect(below).toHaveLength(3);
  });
});

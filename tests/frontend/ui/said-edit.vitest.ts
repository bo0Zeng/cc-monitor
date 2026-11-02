/**
 * 编辑已发的一句（桌面）：气泡悬停「编辑」· 就地编辑（原文填好、后面的卡淡掉）· 草稿落本地 · 开框当场问那台交的退法、就地摆在框下 ·
 * 一个主按钮「回退并发送」（回车同它）· Esc 收起 · 忙闲一变再问 · 交了等回执（发送中 · 拒 · 期限到了）。
 * 选项与交出去那一口换成假的（照真的形状回）。
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

import { SAID_EDIT_EVENT, SaidEdit, markEditable, type SaidAt, type SaidEditIo } from "../../../src/frontend/ui/said-edit";
import { renderMessage, type RenderContext } from "../../../src/frontend/ui/cards";
import type { RewindOptions } from "../../../src/frontend/ui/generated/RewindOptions";
import type { Rewind } from "../../../src/frontend/ui/generated/Rewind";
import type { LineRecord } from "../../../src/frontend/ui/generated/LineRecord";
import type { PutEnd } from "../../../src/frontend/ui/session-queue";
import { copyText } from "../../../src/frontend/ui/copy-table";
import s from "../../../src/frontend/ui/said-edit.module.css";

const flush = async (): Promise<void> => {
  for (let i = 0; i < 20; i++) await new Promise((r) => setTimeout(r, 0));
};

const AT: SaidAt = { origin: "devbox", sid: "s-1", id: "u-2", text: "第二句" };

const IDLE: RewindOptions = {
  how: [
    { how: "conversation", text: "how-conv", default: true, enabled: true },
    { how: "code_and_conversation", text: "how-code", default: false, enabled: true },
  ],
  when: [],
};
const BUSY: RewindOptions = {
  ...IDLE,
  when: [
    { when: "after_turn", text: "when-after", default: true },
    { when: "now", text: "when-now", default: false },
  ],
};

/** 一段流：三张卡，编辑中间那一张。 */
function stream(): { content: HTMLElement; cards: HTMLElement[] } {
  const content = document.createElement("div");
  content.className = "stream-content";
  const cards = [0, 1, 2].map((i) => {
    const c = document.createElement("div");
    c.className = "card card-user";
    const body = document.createElement("div");
    body.className = "card-body";
    body.textContent = `第${i}句`;
    c.appendChild(body);
    content.appendChild(c);
    return c;
  });
  document.body.replaceChildren(content);
  return { content, cards };
}

/** 假的两口：`options` 依次回 `answers`（用完了回最后一个）；`send` 回 `end`，或等手动放行（`hold`）。 */
function rig(opts: { answers?: (RewindOptions | Error)[]; end?: PutEnd; hold?: boolean } = {}) {
  const sent: { text: string; rewind: Rewind }[] = [];
  const answers = opts.answers ?? [IDLE];
  let asked = 0;
  let release: ((e: PutEnd) => void) | null = null;
  const io: SaidEditIo = {
    options: vi.fn(async () => {
      const a = answers[Math.min(asked++, answers.length - 1)]!;
      if (a instanceof Error) throw a;
      return a;
    }),
    send: vi.fn((_o: string, _s: string, text: string, rewind: Rewind): Promise<PutEnd> => {
      sent.push({ text, rewind });
      const end = opts.end ?? { at: "receipt", said: "排队 · 第 1" };
      if (!opts.hold) return Promise.resolve(end);
      return new Promise<PutEnd>((r) => (release = r));
    }),
  };
  return { io, sent, edit: new SaidEdit(io), release: (e: PutEnd) => release?.(e) };
}

const box = (): HTMLTextAreaElement => document.querySelector('[data-role="said-edit-box"]') as HTMLTextAreaElement;
const btn = (role: string): HTMLButtonElement | null => document.querySelector(`[data-role="${role}"]`);
const radios = (name: string): HTMLInputElement[] => Array.from(document.querySelectorAll<HTMLInputElement>(`input[name="${name}"]`));
const type = (t: string): void => {
  box().value = t;
  box().dispatchEvent(new Event("input"));
};
const key = (k: string, more: KeyboardEventInit = {}): void => {
  box().dispatchEvent(new KeyboardEvent("keydown", { key: k, bubbles: true, cancelable: true, ...more }));
};

beforeEach(() => localStorage.clear());

describe("编辑已发的一句 · 桌面", () => {
  it("★ 就地编辑：原文填好、后面的卡淡掉、上面的不动；取消 ⇒ 正文回来、都不淡", () => {
    const { cards } = stream();
    const { edit } = rig();
    edit.open(cards[1]!, AT);
    expect(box().value).toBe("第二句");
    expect(cards[1]!.querySelector(".card-body")).toBeNull();
    expect(cards[2]!.dataset.saidEditAfter).toBe("1");
    expect(cards[2]!.style.opacity).not.toBe("");
    expect(cards[0]!.dataset.saidEditAfter).toBeUndefined();
    btn("said-edit-cancel")!.click();
    expect(box()).toBeNull();
    expect(cards[1]!.querySelector(".card-body")!.textContent).toBe("第1句");
    expect(cards[2]!.dataset.saidEditAfter).toBeUndefined();
    expect(cards[2]!.style.opacity).toBe("");
  });

  it("★ 草稿落本地：改了字、换一个编辑器再开同一句 ⇒ 还是改过的字", () => {
    const { cards } = stream();
    rig().edit.open(cards[1]!, AT);
    type("改过的");
    const { cards: again } = stream();
    rig().edit.open(again[1]!, AT);
    expect(box().value).toBe("改过的");
  });

  it("★ 开框当场问退法（不点什么）；读中主按钮灰、写「正在读取」；回来后缺省那一项选上、主按钮能点；只有一颗主按钮", async () => {
    const { cards } = stream();
    const { edit, io } = rig();
    edit.open(cards[1]!, AT);
    expect(io.options).toHaveBeenCalledWith("devbox", "s-1", "u-2");
    expect(btn("said-edit-confirm")!.getAttribute("aria-disabled")).toBe("true");
    expect(document.body.textContent).toContain(copyText("saidEdit.pick.loading"));
    await flush();
    expect(radios("said-edit-how").map((r) => r.checked)).toEqual([true, false]);
    expect(radios("said-edit-when")).toEqual([]);
    expect(btn("said-edit-confirm")!.getAttribute("aria-disabled")).toBeNull();
    expect(btn("said-edit-confirm")!.textContent).toBe(copyText("saidEdit.pick.confirm"));
    expect(document.querySelectorAll('[data-kind="primary"]').length).toBe(1);
  });

  it("★ 在干活 ⇒ 多一行回退时机、缺省等本轮完；回退并发送 ⇒ 交出去的那一句带 rewind {to, how, when}", async () => {
    const { cards } = stream();
    const { edit, sent } = rig({ answers: [BUSY] });
    edit.open(cards[1]!, AT);
    type("改过的");
    await flush();
    expect(radios("said-edit-when").map((r) => r.checked)).toEqual([true, false]);
    radios("said-edit-how")[1]!.click();
    radios("said-edit-when")[1]!.click();
    btn("said-edit-confirm")!.click();
    await flush();
    expect(sent).toEqual([{ text: "改过的", rewind: { to: "u-2", how: "code_and_conversation", when: "now" } }]);
  });

  it("★ 那条会话忙闲一变 ⇒ 再问一次、照新回包重摆，选过的那一项还在就留着；值没变 · 别的会话 ⇒ 不问", async () => {
    const { cards } = stream();
    const { edit, io } = rig({ answers: [IDLE, BUSY] });
    edit.activity("s-1", "idle");
    edit.open(cards[1]!, AT);
    await flush();
    radios("said-edit-how")[1]!.click();
    expect(radios("said-edit-when")).toEqual([]);
    edit.activity("s-1", "idle");
    edit.activity("s-9", "working");
    await flush();
    expect(io.options).toHaveBeenCalledTimes(1);
    edit.activity("s-1", "working");
    await flush();
    expect(io.options).toHaveBeenCalledTimes(2);
    expect(radios("said-edit-when").map((r) => r.checked)).toEqual([true, false]);
    expect(radios("said-edit-how").map((r) => r.checked)).toEqual([false, true]);
  });

  it("★ 键同输入行：回车 ＝ 回退并发送；Shift+回车 · 组字中的回车不送；Esc 收起、草稿留着", async () => {
    const { cards } = stream();
    const { edit, sent } = rig({ end: { at: "none", said: "那台断开" } });
    edit.open(cards[1]!, AT);
    await flush();
    type("改过的");
    key("Enter", { shiftKey: true });
    key("Enter", { isComposing: true });
    await flush();
    expect(sent).toEqual([]);
    key("Escape");
    expect(box()).toBeNull();
    const { cards: again } = stream();
    edit.open(again[1]!, AT);
    expect(box().value).toBe("改过的");
    await flush();
    key("Enter");
    await flush();
    expect(sent.map((x) => x.text)).toEqual(["改过的"]);
  });

  it("★ 交了等回执：主按钮「发送中」灰着、再点不交第二次；回执 ⇒ 收框、草稿清掉", async () => {
    const { cards } = stream();
    const r = rig({ hold: true });
    r.edit.open(cards[1]!, AT);
    type("改过的");
    await flush();
    btn("said-edit-confirm")!.click();
    await flush();
    const c = btn("said-edit-confirm")!;
    expect(c.dataset.busy).toBe("true");
    expect(c.textContent).toContain(copyText("mobChat.delivery.sending"));
    expect(radios("said-edit-how").every((x) => x.disabled)).toBe(true);
    c.click();
    key("Enter");
    await flush();
    expect(r.sent.length).toBe(1);
    r.release({ at: "receipt", said: "排队 · 第 1" });
    await flush();
    expect(box()).toBeNull();
    const { cards: again } = stream();
    rig().edit.open(again[1]!, AT);
    expect(box().value).toBe("第二句");
  });

  it("★ 那台拒了 ⇒ 框不收、那一句错误色写在按钮上方、主按钮回来可再送", async () => {
    const { cards } = stream();
    const { edit, sent } = rig({ end: { at: "refused", said: "said-queue-full" } });
    edit.open(cards[1]!, AT);
    await flush();
    btn("said-edit-confirm")!.click();
    await flush();
    expect(box()).not.toBeNull();
    const why = document.querySelector('[data-role="said-edit-refused"]') as HTMLElement;
    expect(why.textContent).toBe("said-queue-full");
    expect(why.className).toContain(s.error);
    expect(why.compareDocumentPosition(btn("said-edit-confirm")!) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(btn("said-edit-confirm")!.dataset.busy).toBeUndefined();
    btn("said-edit-confirm")!.click();
    await flush();
    expect(sent.length).toBe(2);
    expect(document.querySelectorAll('[data-role="said-edit-refused"]').length).toBe(1);
  });

  it("★ 没回执 / 期限到了 ⇒ 收框、草稿留着，气泡下一行小字「已改一版在待发」", async () => {
    const { cards } = stream();
    const { edit } = rig({ end: { at: "none", said: "那台断开" } });
    edit.open(cards[1]!, AT);
    type("又改");
    await flush();
    btn("said-edit-confirm")!.click();
    await flush();
    expect(box()).toBeNull();
    expect(cards[1]!.querySelector('[data-role="said-edit-pending"]')!.textContent).toBe(copyText("saidEdit.card.pending"));
    const { cards: again } = stream();
    rig().edit.open(again[1]!, AT);
    expect(box().value).toBe("又改");
  });

  it("★ 不能选的那一项灰着、点不了，原因写在那一项同一行；退之前要知道的那一句写在选项下面", async () => {
    const { cards } = stream();
    const { edit, sent } = rig({
      answers: [
        {
          how: [
            { how: "conversation", text: "how-conv", default: true, enabled: true },
            { how: "code_and_conversation", text: "how-code", default: false, enabled: false, why: "why-no-code" },
          ],
          when: [],
          said: "said-runs-after",
        },
      ],
    });
    edit.open(cards[1]!, AT);
    await flush();
    const [conv, code] = radios("said-edit-how");
    expect(conv!.disabled).toBe(false);
    expect(code!.disabled).toBe(true);
    const row = code!.closest("label")!;
    expect(row.getAttribute("aria-disabled")).toBe("true");
    expect(row.textContent).toContain("why-no-code");
    expect(conv!.closest("label")!.getAttribute("aria-disabled")).toBeNull();
    expect(document.body.textContent).toContain("said-runs-after");
    code!.click();
    btn("said-edit-confirm")!.click();
    await flush();
    expect(sent.map((x) => x.rewind.how)).toEqual(["conversation"]);
  });

  it("★ 那一家不支持 · 问不成 ⇒ 只写为什么、没有主按钮", async () => {
    const { cards } = stream();
    const { edit, io } = rig({ answers: [{ how: [], when: [], said: "said-unsupported" }] });
    edit.open(cards[1]!, AT);
    await flush();
    expect(document.body.textContent).toContain("said-unsupported");
    expect(btn("said-edit-confirm")).toBeNull();
    expect(btn("said-edit-cancel")).not.toBeNull();
    key("Enter");
    await flush();
    expect(io.send).not.toHaveBeenCalled();

    const { cards: again } = stream();
    const r = rig({ answers: [new Error("那台离线")] });
    r.edit.open(again[1]!, AT);
    await flush();
    expect(document.body.textContent).toContain("那台离线");
    expect(btn("said-edit-confirm")).toBeNull();
  });

  it("★ 卡上的「编辑」：点了往上冒一个事件带那一句；渲染上下文不给 editable ⇒ 人说的那一句没有「编辑」", () => {
    const card = document.createElement("div");
    const header = document.createElement("div");
    header.className = "card-header";
    card.appendChild(header);
    markEditable(card, AT);
    document.body.replaceChildren(card);
    const got: SaidAt[] = [];
    document.addEventListener(SAID_EDIT_EVENT, (e) => got.push((e as CustomEvent<{ at: SaidAt }>).detail.at), { once: true });
    const b = btn("said-edit")!;
    expect(b.textContent).toBe(copyText("rsFilewinEditor.label.edit"));
    b.click();
    expect(got).toEqual([AT]);

    const rec = { agent: "claude", id: "u-9", t: "said", who: { text: "hi", speaker: { kind: "human" } }, blocks: [] } as unknown as LineRecord;
    const base = { parentPath: "/p", origin: "devbox", toolUseNames: new Map(), toolUseElements: new Map() } as unknown as RenderContext;
    const plain = renderMessage(rec, base);
    const editable = renderMessage(rec, { ...base, editSaid: { sid: "s-1" } });
    const el = (r: ReturnType<typeof renderMessage>): HTMLElement => (r as { element: HTMLElement }).element;
    expect(el(plain).querySelector('[data-role="said-edit"]')).toBeNull();
    expect(el(editable).querySelector('[data-role="said-edit"]')).not.toBeNull();
  });
});

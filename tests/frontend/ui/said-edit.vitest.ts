/**
 * 编辑已发的一句（桌面）：气泡悬停「编辑」· 就地编辑（原文填好、后面的卡淡掉）· 草稿落本地 · 发送那一下问那台交的选项 ·
 * 确认那一下带 `rewind` 交给输入队那一份待发。选项与交出去那一口换成假的（照真的形状回）。
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

import { SAID_EDIT_EVENT, SaidEdit, markEditable, type SaidAt, type SaidEditIo } from "../../../src/frontend/ui/said-edit";
import { renderMessage, type RenderContext } from "../../../src/frontend/ui/cards";
import type { RewindOptions } from "../../../src/frontend/ui/generated/RewindOptions";
import type { Rewind } from "../../../src/frontend/ui/generated/Rewind";
import type { LineRecord } from "../../../src/frontend/ui/generated/LineRecord";
import type { PutEnd } from "../../../src/frontend/ui/session-queue";
import { copyText } from "../../../src/frontend/ui/copy-table";

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

function rig(opts: { options?: RewindOptions; end?: PutEnd } = {}) {
  const sent: { text: string; rewind: Rewind }[] = [];
  const io: SaidEditIo = {
    options: vi.fn(async () => opts.options ?? IDLE),
    send: vi.fn(async (_o: string, _s: string, text: string, rewind: Rewind): Promise<PutEnd> => {
      sent.push({ text, rewind });
      return opts.end ?? { at: "receipt", said: "排队 · 第 1" };
    }),
  };
  return { io, sent, edit: new SaidEdit(io) };
}

const box = (): HTMLTextAreaElement => document.querySelector('[data-role="said-edit-box"]') as HTMLTextAreaElement;
const btn = (role: string): HTMLButtonElement | null => document.querySelector(`[data-role="${role}"]`);
const radios = (name: string): HTMLInputElement[] => Array.from(document.querySelectorAll<HTMLInputElement>(`input[name="${name}"]`));

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
    box().value = "改过的";
    box().dispatchEvent(new Event("input"));
    const { cards: again } = stream();
    rig().edit.open(again[1]!, AT);
    expect(box().value).toBe("改过的");
  });

  it("★ 发送 ⇒ 问那台的选项：缺省那一项选上；主 agent 空着 ⇒ 没有「什么时候退」", async () => {
    const { cards } = stream();
    const { edit, io } = rig();
    edit.open(cards[1]!, AT);
    btn("said-edit-send")!.click();
    await flush();
    expect(io.options).toHaveBeenCalledWith("devbox", "s-1", "u-2");
    expect(radios("said-edit-how").map((r) => r.checked)).toEqual([true, false]);
    expect(document.body.textContent).toContain("how-conv");
    expect(radios("said-edit-when")).toEqual([]);
  });

  it("★ 在干活 ⇒ 多一行「什么时候退」、缺省等本轮完；确认 ⇒ 交出去的那一句带 rewind {to, how, when}", async () => {
    const { cards } = stream();
    const { edit, sent } = rig({ options: BUSY });
    edit.open(cards[1]!, AT);
    box().value = "改过的";
    box().dispatchEvent(new Event("input"));
    btn("said-edit-send")!.click();
    await flush();
    expect(radios("said-edit-when").map((r) => r.checked)).toEqual([true, false]);
    radios("said-edit-how")[1]!.click();
    radios("said-edit-when")[1]!.click();
    btn("said-edit-confirm")!.click();
    await flush();
    expect(sent).toEqual([{ text: "改过的", rewind: { to: "u-2", how: "code_and_conversation", when: "now" } }]);
  });

  it("★ 回执到了 ⇒ 编辑框收起、草稿清掉；没回执 ⇒ 草稿留着", async () => {
    const { cards } = stream();
    const { edit } = rig();
    edit.open(cards[1]!, AT);
    box().value = "改过的";
    box().dispatchEvent(new Event("input"));
    btn("said-edit-send")!.click();
    await flush();
    btn("said-edit-confirm")!.click();
    await flush();
    expect(box()).toBeNull();
    const { cards: again } = stream();
    rig().edit.open(again[1]!, AT);
    expect(box().value).toBe("第二句");

    const { cards: third } = stream();
    const r = rig({ end: { at: "none", said: "那台断开" } });
    r.edit.open(third[1]!, AT);
    box().value = "又改";
    box().dispatchEvent(new Event("input"));
    btn("said-edit-send")!.click();
    await flush();
    btn("said-edit-confirm")!.click();
    await flush();
    const { cards: fourth } = stream();
    rig().edit.open(fourth[1]!, AT);
    expect(box().value).toBe("又改");
  });

  it("★ 不能选的那一项灰着、点不了、下面写那台那一句为什么；退之前要知道的那一句写在选项下面", async () => {
    const { cards } = stream();
    const { edit, sent } = rig({
      options: {
        how: [
          { how: "conversation", text: "how-conv", default: true, enabled: true },
          { how: "code_and_conversation", text: "how-code", default: false, enabled: false, why: "why-no-code" },
        ],
        when: [],
        said: "said-runs-after",
      },
    });
    edit.open(cards[1]!, AT);
    btn("said-edit-send")!.click();
    await flush();
    const [conv, code] = radios("said-edit-how");
    expect(conv!.disabled).toBe(false);
    expect(code!.disabled).toBe(true);
    const row = code!.closest("label")!;
    expect(row.getAttribute("aria-disabled")).toBe("true");
    expect(row.parentElement!.textContent).toContain("why-no-code");
    expect(conv!.closest("label")!.getAttribute("aria-disabled")).toBeNull();
    expect(document.body.textContent).toContain("said-runs-after");
    code!.click();
    btn("said-edit-confirm")!.click();
    await flush();
    expect(sent.map((s) => s.rewind.how)).toEqual(["conversation"]);
  });

  it("★ 那一家不支持 ⇒ 只写那台那一句、不给确认", async () => {
    const { cards } = stream();
    const { edit, io } = rig({ options: { how: [], when: [], said: "said-unsupported" } });
    edit.open(cards[1]!, AT);
    btn("said-edit-send")!.click();
    await flush();
    expect(document.body.textContent).toContain("said-unsupported");
    expect(btn("said-edit-confirm")).toBeNull();
    expect(io.send).not.toHaveBeenCalled();
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
    expect(b.textContent).toBe(copyText("saidEdit.card.edit"));
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

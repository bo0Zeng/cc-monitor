// 消息流里人说的那一句带的附件：图片块 ⇒ 缩略图；`attachments[]` ⇒ chip（类别字 · 名 · 大小，都是那台写好的）；
// 那台已清掉 ⇒ 虚线「已清理」、不可点；没有正文、只有附件 / 图片也建卡。
// 图片块只带引用（类型 · 字节数 · 哪一行 · 第几张）：建卡时画定尺寸的占位、不发请求；当场建的卡当场取，惰路批里建的卡进视口
// （`render.ts::enhanceCard`）才取 —— 一条 `history-image`，回来才有 `src`；取不到 ⇒ 占位留着、`title` 写那一句。
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import { renderMessage } from "../../../../src/frontend/ui/cards/index";
import { enhanceCard } from "../../../../src/frontend/ui/render";
import { chanArgsJson, chanReply, isChanCall, NO_CHANNEL } from "../../../test-support/chan-fake";
import { copyText } from "../../../../src/frontend/ui/copy-table";
import { LOCAL_ORIGIN } from "../../../../src/frontend/ui/ipc/origin";
import type { LineRecord } from "../../../../src/frontend/ui/generated/LineRecord";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string, args: unknown) =>
    Promise.resolve(isChanCall(cmd, args, "history-image") ? chanReply({ mediaType: "image/png", data: "AAAA" }) : chanReply(null)),
  );
});

/** 发出去的那几条 `history-image`（载荷解回 JSON）。 */
const asked = () => invokeMock.mock.calls.filter((c) => isChanCall(c[0] as string, c[1], "history-image")).map((c) => chanArgsJson(c[1] as never));

const settle = () => new Promise((r) => setTimeout(r, 0));

const ctx = (lazy = false) => ({
  parentPath: "/p/s.jsonl",
  origin: LOCAL_ORIGIN,
  toolUseNames: new Map(),
  toolUseElements: new Map(),
  pendingToolResults: new Map(),
  lazy,
});

const said = (text: string, blocks: unknown[], attachments?: unknown[]): LineRecord =>
  ({
    agent: "claude",
    t: "said",
    id: "u1",
    at: "2026-10-23T07:31:00.000Z",
    who: { speaker: { kind: "human" }, text },
    blocks,
    cwd: "/w",
    ...(attachments ? { attachments } : {}),
  }) as unknown as LineRecord;

const IMAGE = { type: "image", mediaType: "image/png", bytes: 3, at: 10, n: 0 };

function card(rec: LineRecord, lazy = false): HTMLElement {
  const r = renderMessage(rec, ctx(lazy) as never);
  if (r.kind !== "card") throw new Error(`该建卡，得到 ${r.kind}`);
  return r.element;
}

describe("消息流里人说的那一句带的附件", () => {
  it("★ 图片块画成缩略图、附件画成 chip（类别字 · 名 · 大小），都在正文上方", async () => {
    const el = card(
      said("截图在这", [{ type: "text", text: "截图在这" }, IMAGE], [{ name: "login.css", rel: ".ccm-attach/1023-1529-7c01/login.css", class: "code", classText: "kind-code", bytes: 1300, sizeText: "1.3 KB" }]),
    );
    const strip = el.querySelector('[data-role="said-attachments"]') as HTMLElement;
    expect(strip).not.toBeNull();
    expect(strip.compareDocumentPosition(el.querySelector(".card-body") as Node) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    await settle();
    expect((strip.querySelector("img") as HTMLImageElement).src).toBe("data:image/png;base64,AAAA");
    const chip = strip.querySelector('[data-role="said-attachment"]') as HTMLElement;
    expect(chip.textContent).toBe("kind-codelogin.css1.3 KB");
    expect(chip.dataset.gone).toBe("false");
  });

  it("★ 那一排画在气泡外：是气泡前一个兄弟，不在气泡里；只有附件没有字 ⇒ 不画空气泡；只有字 ⇒ 整张卡就是气泡", () => {
    const el = card(said("截图在这", [{ type: "text", text: "截图在这" }, IMAGE]));
    const strip = el.querySelector('[data-role="said-attachments"]') as HTMLElement;
    const bubble = el.querySelector('[data-role="said-bubble"]') as HTMLElement;
    expect(bubble, "带附件那一句有自己的气泡").not.toBeNull();
    expect(bubble.contains(strip), "附件不在气泡里").toBe(false);
    expect(strip.nextElementSibling).toBe(bubble);
    expect(bubble.querySelector(".card-body")?.textContent).toBe("截图在这");
    expect(card(said("", [IMAGE])).querySelector('[data-role="said-bubble"]'), "没有字不画空气泡").toBeNull();
    const plain = card(said("只有字", [{ type: "text", text: "只有字" }]));
    expect(plain.querySelector('[data-role="said-bubble"]'), "只有字：卡本身是气泡").toBeNull();
    expect(Array.from(plain.children, (c) => c.className)).toEqual(["card-header", "card-body"]);
  });

  it("那台已清掉 ⇒ 虚线「已清理」、不可点", () => {
    const el = card(said("这张是上周那次的", [{ type: "text", text: "x" }], [{ name: "old.pdf", rel: ".ccm-attach/1016-0900-aaaa/old.pdf", class: "pdf", classText: "kind-pdf", gone: true }]));
    const chip = el.querySelector('[data-role="said-attachment"]') as HTMLElement;
    expect(chip.dataset.gone).toBe("true");
    expect(chip.tagName).toBe("SPAN");
    expect(chip.textContent).toContain(copyText("attach.msg.cleared"));
  });

  it("没有正文、只有图片也建卡；什么都没有照旧不建", () => {
    expect(card(said("", [IMAGE])).querySelectorAll("img").length).toBe(1);
    expect(renderMessage(said("", []), ctx() as never).kind).toBe("skip");
  });

  it("★ 当场建的卡：按引用问一条 history-image（哪份 · 哪一行 · 第几张 · 哪一条），回来才有 src", async () => {
    const img = card(said("", [IMAGE])).querySelector("img") as HTMLImageElement;
    expect(img.getAttribute("src"), "回来之前没有 src（占位）").toBeNull();
    await settle();
    expect(asked()).toEqual([{ path: "/p/s.jsonl", at: 10, n: 0, id: "u1" }]);
    expect(img.src).toBe("data:image/png;base64,AAAA");
  });

  it("★ 惰路批里建的卡：建卡不发请求，进视口（enhanceCard）那一下才取", async () => {
    const el = card(said("", [IMAGE]), true);
    await settle();
    expect(asked(), "建卡时不取").toEqual([]);
    expect((el.querySelector("img") as HTMLImageElement).getAttribute("src")).toBeNull();
    enhanceCard(el);
    await settle();
    expect(asked()).toHaveLength(1);
    expect((el.querySelector("img") as HTMLImageElement).src).toBe("data:image/png;base64,AAAA");
  });

  it("取不到 ⇒ 占位留着（没有 src）、title 写那一句", async () => {
    invokeMock.mockRejectedValue(NO_CHANNEL);
    const img = card(said("", [IMAGE])).querySelector("img") as HTMLImageElement;
    await settle();
    await settle();
    expect(img.getAttribute("src")).toBeNull();
    expect(img.title).toContain(NO_CHANNEL.said);
  });
});

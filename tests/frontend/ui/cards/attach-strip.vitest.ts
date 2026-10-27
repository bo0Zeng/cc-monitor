// 消息流里人说的那一句带的附件：图片块 ⇒ 缩略图；`attachments[]` ⇒ chip（类别字 · 名 · 大小，都是那台写好的）；
// 那台已清掉 ⇒ 虚线「已清理」、不可点；没有正文、只有附件 / 图片也建卡。
import { describe, it, expect } from "vitest";
import { renderMessage } from "../../../../src/frontend/ui/cards/index";
import { copyText } from "../../../../src/frontend/ui/copy-table";
import { LOCAL_ORIGIN } from "../../../../src/frontend/ui/ipc/origin";
import type { LineRecord } from "../../../../src/frontend/ui/generated/LineRecord";

const ctx = () => ({
  parentPath: "/p/s.jsonl",
  origin: LOCAL_ORIGIN,
  toolUseNames: new Map(),
  toolUseElements: new Map(),
  pendingToolResults: new Map(),
  lazy: false,
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

const IMAGE = { type: "image", source: { type: "base64", media_type: "image/png", data: "AAAA" } };

function card(rec: LineRecord): HTMLElement {
  const r = renderMessage(rec, ctx() as never);
  if (r.kind !== "card") throw new Error(`该建卡，得到 ${r.kind}`);
  return r.element;
}

describe("消息流里人说的那一句带的附件", () => {
  it("★ 图片块画成缩略图、附件画成 chip（类别字 · 名 · 大小），都在正文上方", () => {
    const el = card(
      said("截图在这", [{ type: "text", text: "截图在这" }, IMAGE], [{ name: "login.css", rel: ".ccm-attach/1023-1529-7c01/login.css", class: "code", bytes: 1300, sizeText: "1.3 KB" }]),
    );
    const strip = el.querySelector('[data-role="said-attachments"]') as HTMLElement;
    expect(strip).not.toBeNull();
    expect(strip.compareDocumentPosition(el.querySelector(".card-body") as Node) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect((strip.querySelector("img") as HTMLImageElement).src).toBe("data:image/png;base64,AAAA");
    const chip = strip.querySelector('[data-role="said-attachment"]') as HTMLElement;
    expect(chip.textContent).toBe(`${copyText("attach.class.code")}login.css1.3 KB`);
    expect(chip.dataset.gone).toBe("false");
  });

  it("那台已清掉 ⇒ 虚线「已清理」、不可点", () => {
    const el = card(said("这张是上周那次的", [{ type: "text", text: "x" }], [{ name: "old.pdf", rel: ".ccm-attach/1016-0900-aaaa/old.pdf", class: "pdf", gone: true }]));
    const chip = el.querySelector('[data-role="said-attachment"]') as HTMLElement;
    expect(chip.dataset.gone).toBe("true");
    expect(chip.tagName).toBe("SPAN");
    expect(chip.textContent).toContain(copyText("attach.msg.cleared"));
  });

  it("没有正文、只有图片也建卡；什么都没有照旧不建", () => {
    expect(card(said("", [IMAGE])).querySelectorAll("img").length).toBe(1);
    expect(renderMessage(said("", []), ctx() as never).kind).toBe("skip");
  });
});

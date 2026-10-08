/**
 * 「消息」里带复制详情的那条（条带 §5.5）：同一颗［复制详情］复制出全部段；［详情］展开只显示最近一段 ＋「另 n 段」，再点收起。
 */
import { describe, it, expect, vi } from "vitest";
import { copyText } from "../../../src/frontend/ui/copy-table";
import { toast } from "../../../src/frontend/ui/kit/toast";
import { StatusMessages } from "../../../src/frontend/ui/status-messages";

describe("「消息」里的复制详情", () => {
  it("★ 展开看最近一段 ＋ 另 n 段；复制出全部段", async () => {
    const written: string[] = [];
    Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText: (t: string) => (written.push(t), Promise.resolve()) } });
    const m = new StatusMessages();
    document.body.appendChild(m.el);
    toast("t-9", "", { detail: "d-1" });
    toast("t-9", "", { detail: "d-2" });
    m.el.querySelector<HTMLElement>('[data-role="status-messages"]')!.click();
    await vi.waitFor(() => expect(document.querySelector('[data-part="copy-detail"]')).not.toBeNull());
    const expand = [...document.querySelectorAll("button")].find((b) => b.textContent === copyText("messages.record.expand"))!;
    expand.click();
    const box = document.querySelector<HTMLElement>('[data-role="message-detail"]')!;
    expect(box.textContent).toBe(`d-2${copyText("messages.record.moreSegments", { n: 1 })}`);
    document.querySelector<HTMLButtonElement>('[data-part="copy-detail"] button')!.click();
    await vi.waitFor(() => expect(written).toHaveLength(1));
    expect(written[0]).toBe(`t-9 ${copyText("kit.toast.count", { n: 2 })}\n\nd-1\n\nd-2`);
    [...document.querySelectorAll("button")].find((b) => b.textContent === copyText("messages.record.expand"))!.click();
    expect(document.querySelector('[data-role="message-detail"]')).toBeNull();
  });
});

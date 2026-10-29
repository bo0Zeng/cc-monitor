/**
 * 消息流底下那个输入框的附件（`composer.ts`）：［＋］· 粘贴 · 拖进同一排 chip；没传完 ⇒ ➤ 灰着说为什么；带附件那一句投进输入队（编号随那一句交）、
 * 回执到了清空，那台拒了原样留着。读写换成假的（附件那几口照真的形状回）。
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

import { Composer, type ComposerReads } from "../../../src/frontend/ui/composer";
import { Outbox } from "../../../src/frontend/ui/outbox";
import type { Tab } from "../../../src/frontend/ui/tab-model";
import { liveFace } from "../../test-support/session-faces";
import type { AttachIo } from "../../../src/frontend/ui/attach-draft";
import type { QueuePut } from "../../../src/frontend/ui/generated/QueuePut";
import { ChanError } from "../../../src/comms/inward/chan";
import { copyText } from "../../../src/frontend/ui/copy-table";
import type { AttachItem } from "../../../src/frontend/ui/generated/AttachItem";
import type { SessionAttachmentCommitReq } from "../../../src/frontend/ui/generated/SessionAttachmentCommitReq";

const TAB = { origin: "devbox", sessionId: "s-1", face: liveFace("idle", true), label: "web-console", labelProject: null, background: false, forkedFromSessionId: null } as unknown as Tab;

const flush = async (): Promise<void> => {
  for (let i = 0; i < 20; i++) await new Promise((r) => setTimeout(r, 0));
};

function rig(opts: { hold?: boolean; put?: () => Promise<QueuePut> } = {}) {
  let release: () => void = () => {};
  const held = new Promise<void>((r) => (release = r));
  let n = 0;
  const io: AttachIo = {
    check: async (_sid: string, items: AttachItem[]) => items.map((it) => ({ ok: true, class: it.name.endsWith(".png") ? ("image" as const) : ("code" as const), class_text: "kind-code" })),
    stage: async () => {
      if (opts.hold) await held;
    },
    commit: async (req: SessionAttachmentCommitReq) => ({ id: `1027-1200-000${++n}`, name: req.name, path: "/p", rel: "r", bytes: req.bytes, sha256: req.expect.sha256, class: "code" as const }),
    drop: async () => {},
    digest: async () => "ab".repeat(32),
    read: async (_b, start, end) => new Uint8Array(end - start),
    newKey: () => "0".repeat(32),
    offline: () => false,
  };
  const say = vi.fn(opts.put ?? (async (): Promise<QueuePut> => ({ outcome: "queued", seq: 1, place: 1, said: "q-1" })));
  const reads: ComposerReads = {
    io: () => io,
    read: async () => ({ items: [], sent: [] }),
    cancel: async () => ({ cancelled: false, queue: { items: [], sent: [] } }),
    watch: () => () => {},
  };
  const outbox = new Outbox(say);
  const page = new Composer({ active: () => TAB, resume: async () => true, front: () => {}, focusStream: () => {} }, outbox, reads);
  document.body.replaceChildren(page.el);
  page.render(); // 第一次画由宿主叫（构造里不画，A2-17-10-1-2）
  return { page, say, release, outbox };
}

async function opened(opts?: Parameters<typeof rig>[0]) {
  const r = rig(opts);
  r.page.render();
  await flush();
  return r;
}

beforeEach(() => localStorage.clear());

const q = <T extends Element>(page: Composer, sel: string): T => page.el.querySelector(sel) as T;
const chips = (page: Composer): HTMLElement[] => Array.from(page.el.querySelectorAll<HTMLElement>('[data-role="attach-chip"]'));
const file = (name: string, n = 3, type = ""): File => new File([new Uint8Array(n)], name, { type });

/** 一份造好的 DataTransfer（jsdom 没有构造器）：文件 · 剪贴板里的字。 */
function transfer(files: File[], text = "", dirs: string[] = []): DataTransfer {
  return {
    files,
    types: ["Files"],
    items: files.map((f) => ({ kind: "file", getAsFile: () => f, webkitGetAsEntry: () => ({ isDirectory: dirs.includes(f.name) }) })),
    getData: (t: string) => (t === "text/plain" ? text : ""),
    dropEffect: "none",
  } as unknown as DataTransfer;
}

describe("输入框的附件", () => {
  it("★［＋］选的文件进输入框上方那一排：类别字 · 名字 · 状态；传完前［送出］灰着、悬停说附件传输中", async () => {
    const { page, release } = await opened({ hold: true });
    const picker = q<HTMLInputElement>(page, '[data-role="attach-plus"] input[type="file"]');
    expect(picker.multiple).toBe(true);
    Object.defineProperty(picker, "files", { value: [file("login.css")], configurable: true });
    picker.dispatchEvent(new Event("change"));
    await flush();
    const [c] = chips(page);
    expect(c.textContent).toContain("kind-code");
    expect(c.textContent).toContain("login.css");
    expect(c.dataset.state).toBe("sending");
    expect(c.querySelector('[data-role="attach-state"]')?.textContent, "传输中只写那个词，不拼字节").toBe(copyText("attach.chip.sending"));
    const send = q<HTMLButtonElement>(page, '[data-role="composer-send"]');
    expect(send.getAttribute("aria-disabled")).toBe("true");
    expect(send.title).toBe(copyText("attach.blocked.sending"));
    release();
    await flush();
    expect(chips(page)[0].dataset.state).toBe("done");
    expect(send.getAttribute("aria-disabled")).toBeNull();
  });

  it("输入框里粘贴只有图片的剪贴板 ⇒ 进这一排（按先后起名），不贴字；有字的剪贴板照旧贴字", async () => {
    const { page } = await opened();
    const box = q<HTMLTextAreaElement>(page, "textarea");
    const ev = new Event("paste", { cancelable: true }) as ClipboardEvent;
    Object.defineProperty(ev, "clipboardData", { value: transfer([file("image.png", 3, "image/png")]) });
    box.dispatchEvent(ev);
    expect(ev.defaultPrevented).toBe(true);
    const words = new Event("paste", { cancelable: true }) as ClipboardEvent;
    Object.defineProperty(words, "clipboardData", { value: transfer([file("image.png", 3, "image/png")], "hello") });
    box.dispatchEvent(words);
    expect(words.defaultPrevented).toBe(false);
    await flush();
    expect(chips(page).map((c) => c.textContent)).toEqual([expect.stringContaining("image-1.png")]);
  });

  it("拖到面板上 ⇒ 整块画目标框（放进哪条会话 · 哪台 · 几份），松手进这一排；目录照样进、由那台判", async () => {
    const { page } = await opened();
    const dt = transfer([file("a.css"), file("assets", 0)], "", ["assets"]);
    const enter = new Event("dragenter", { cancelable: true }) as DragEvent;
    Object.defineProperty(enter, "dataTransfer", { value: dt });
    page.el.dispatchEvent(enter);
    const overlay = q<HTMLElement>(page, '[data-role="attach-drop"]');
    expect(overlay.hidden).toBe(false);
    expect(overlay.textContent).toContain(copyText("attach.drop.title"));
    expect(overlay.textContent).toContain(["web-console", "devbox"].join(copyText("kit.text.sep")));
    const drop = new Event("drop", { cancelable: true }) as DragEvent;
    Object.defineProperty(drop, "dataTransfer", { value: dt });
    page.el.dispatchEvent(drop);
    expect(overlay.hidden).toBe(true);
    await flush();
    expect(chips(page).length).toBe(2);
  });

  it("★ 带附件那一句投进输入队（编号按先后 · 不走送字）；回执到了 ⇒ 字与 chip 一起清", async () => {
    const { page, say } = await opened();
    const picker = q<HTMLInputElement>(page, 'input[type="file"]');
    Object.defineProperty(picker, "files", { value: [file("a.css"), file("b.png")], configurable: true });
    picker.dispatchEvent(new Event("change"));
    await flush();
    const box = q<HTMLTextAreaElement>(page, "textarea");
    box.value = "看下截图";
    box.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter" }));
    await flush();
    expect(say).toHaveBeenCalledWith("devbox", expect.objectContaining({ machine: "<local>", sid: "s-1", text: "看下截图", attachments: ["1027-1200-0001", "1027-1200-0002"], end: "monitor" }));
    expect(box.value).toBe("");
    expect(chips(page)).toEqual([]);
  });

  it("那台拒了（附件已不在）⇒ 不清字、chip 留着，照那台那一句说", async () => {
    const refused = new ChanError({ layer: "peer", why: "refused", body: new TextEncoder().encode(JSON.stringify({ code: "attachment_gone", message: "gone-said" })) }, "gone-said");
    const { page } = await opened({ put: async () => Promise.reject(refused) });
    const picker = q<HTMLInputElement>(page, 'input[type="file"]');
    Object.defineProperty(picker, "files", { value: [file("a.css")], configurable: true });
    picker.dispatchEvent(new Event("change"));
    await flush();
    const box = q<HTMLTextAreaElement>(page, "textarea");
    box.value = "x";
    box.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter" }));
    await flush();
    expect(box.value).toBe("x");
    expect(chips(page).length).toBe(1);
    expect(page.el.textContent).toContain("gone-said");
  });
});

/** ★ 说一句先成待发、没回执就留着：回执到之前那台断开 ⇒ 字留在框里「发送中」；那台连上 ⇒ 照同一个编号再交，回执到了才清框。 */
describe("说一句：回执到了才算送出", () => {
  it("回执前那台断开 ⇒ 待发里还在；连上后重交同一个编号；回执到了清框", async () => {
    const down = new ChanError({ layer: "hop", at: { idx: 0, tag: "wait" }, reach: "Sent", why: "Overrun" }, "no-answer");
    let up = false;
    const { page, say, outbox } = await opened({ put: async () => (up ? { outcome: "seen", seq: 1, place: 1, said: "q-1" } : Promise.reject(down)) });
    const picker = q<HTMLInputElement>(page, 'input[type="file"]');
    Object.defineProperty(picker, "files", { value: [file("a.css")], configurable: true });
    picker.dispatchEvent(new Event("change"));
    await flush();
    const box = q<HTMLTextAreaElement>(page, "textarea");
    box.value = "接着做";
    box.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter" }));
    await flush();
    expect(box.value).toBe("接着做");
    expect(page.el.textContent).toContain("no-answer");
    up = true;
    // 待发那一份的重交挂在「那台接上」（主窗口把机器状态转给它）。
    outbox.machineUp("devbox");
    await flush();
    const ids = say.mock.calls.map((c) => ((c as unknown[])[1] as { id: string }).id);
    expect(ids.length).toBe(2);
    expect(new Set(ids).size).toBe(1);
    expect(box.value).toBe("");
    expect(page.el.textContent).toContain("q-1");
  });
});

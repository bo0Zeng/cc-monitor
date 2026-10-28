/**
 * 终端面板输入行的附件：［＋］· 粘贴 · 拖进同一排 chip；没传完 ⇒［送出］灰着说为什么；带附件那一句投进输入队（编号随那一句交）、
 * 回执到了清空，那台拒了原样留着。读写换成假的（附件那几口照真的形状回）。
 */
import { describe, expect, it, vi } from "vitest";

import { TerminalPage, type AttachReads, type TerminalReads, type Watched } from "../../../src/frontend/ui/terminal-page";
import type { AttachIo } from "../../../src/frontend/ui/attach-draft";
import type { TerminalRow, TerminalShot } from "../../../src/frontend/ui/terminal-reads";
import type { QueuePut } from "../../../src/frontend/ui/generated/QueuePut";
import { ChanError } from "../../../src/comms/inward/chan";
import { copyText } from "../../../src/frontend/ui/copy-table";
import type { AttachItem } from "../../../src/frontend/ui/generated/AttachItem";
import type { SessionAttachmentCommitReq } from "../../../src/frontend/ui/generated/SessionAttachmentCommitReq";

const SLOT: Watched = { kind: "slot", origin: "devbox", terminal: "%7", name: "web-1", title: "web-console" };
const ROW: TerminalRow = { terminal: "%7", name: "web-1", sid: "s-1", clients: 0, input: "shared", programExited: false, inputNo: null };
const SHOT: TerminalShot = { lines: [], text: "$", screen: "fp", atText: "12:00:00" };

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
  const reads = {
    list: vi.fn(async () => [ROW]),
    shot: vi.fn(async () => SHOT),
    send: vi.fn(async () => ({ result: "delivered" as const, acts: [] })),
    follow: vi.fn(() => ({ stop: () => {} })),
  };
  const attach: AttachReads = {
    put: say,
    io: () => io,
    read: async () => ({ items: [], sent: [] }),
    cancel: async () => ({ cancelled: false, queue: { items: [], sent: [] } }),
    watch: () => () => {},
  };
  const page = new TerminalPage({ active: () => SLOT, front: () => {}, focusStream: () => {} }, reads as unknown as TerminalReads, attach);
  document.body.replaceChildren(page.el);
  return { page, reads, say, release };
}

async function opened(opts?: Parameters<typeof rig>[0]) {
  const r = rig(opts);
  r.page.sessionChanged();
  r.page.setVisible(true);
  await flush();
  return r;
}

const q = <T extends Element>(page: TerminalPage, sel: string): T => page.el.querySelector(sel) as T;
const chips = (page: TerminalPage): HTMLElement[] => Array.from(page.el.querySelectorAll<HTMLElement>('[data-role="attach-chip"]'));
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

describe("终端面板输入行的附件", () => {
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
    const send = Array.from(page.el.querySelectorAll("button")).find((b) => b.textContent === copyText("terminal.input.send")) as HTMLButtonElement;
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
    const { page, reads, say } = await opened();
    const picker = q<HTMLInputElement>(page, 'input[type="file"]');
    Object.defineProperty(picker, "files", { value: [file("a.css"), file("b.png")], configurable: true });
    picker.dispatchEvent(new Event("change"));
    await flush();
    const box = q<HTMLTextAreaElement>(page, "textarea");
    box.value = "看下截图";
    box.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter" }));
    await flush();
    expect(say).toHaveBeenCalledWith("devbox", expect.objectContaining({ machine: "<local>", sid: "s-1", text: "看下截图", attachments: ["1027-1200-0001", "1027-1200-0002"], end: "monitor" }));
    expect(reads.send).not.toHaveBeenCalled();
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
    const { page, say } = await opened({ put: async () => (up ? { outcome: "seen", seq: 1, place: 1, said: "q-1" } : Promise.reject(down)) });
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
    page.machineUp("devbox");
    await flush();
    const ids = say.mock.calls.map((c) => ((c as unknown[])[1] as { id: string }).id);
    expect(ids.length).toBe(2);
    expect(new Set(ids).size).toBe(1);
    expect(box.value).toBe("");
    expect(page.el.textContent).toContain("q-1");
  });
});

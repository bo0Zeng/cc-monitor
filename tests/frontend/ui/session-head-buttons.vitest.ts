/**
 * 会话头上放哪几颗由用户自己排，默认 ＝ 打开工作目录起那一排。
 *
 * - 默认：打开工作目录 · 切到终端 · 看它的终端 · 在新窗口打开 · 账号 · 显示 ▾ · 在会话里找 · ⋯（本会话用不上的不出、不留空位）；
 * - 存 `config.json` 的 `sessionHead.buttons`（顺序即会话头顺序；删掉这个键 ＝ 恢复默认；认不出的 id 跳过）；
 * - 入口：按钮那一排右键（调整… · 从会话头移走「x」）·「⋯」最后一项；编辑浮层勾 / 拖 / 恢复默认，立刻生效；
 * - 「结束会话」与按状态那一颗不进清单；放不下从最后一颗起收进「⋯」最上面一组；两个流开关合成一颗「显示 ▾」。
 *
 * 夹具只造结构（sid · 路径），不采会话正文。
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

const store = vi.hoisted(() => ({ cfg: {} as Record<string, unknown>, edits: [] as unknown[] }));
vi.mock("../../../src/frontend/ui/config", async (orig) => {
  const actual = await orig<Record<string, unknown>>();
  const { applyConfigEdits } = await import("./config-patch-fake");
  return {
    ...actual,
    loadConfig: vi.fn(async () => store.cfg),
    patchConfig: vi.fn(async (edits: Parameters<typeof applyConfigEdits>[1]) => {
      store.edits.push(edits);
      store.cfg = JSON.parse(applyConfigEdits(JSON.stringify(store.cfg), edits)) as Record<string, unknown>;
      return { stamped: [] };
    }),
  };
});
vi.mock("../../../src/frontend/ui/kit/toast", () => ({ toast: vi.fn(), undoToast: vi.fn(), failToast: vi.fn(), silentUndo: vi.fn() }));
vi.mock("../../../src/frontend/ui/terminal-front", () => ({ terminalFrontAvailable: vi.fn(() => false) }));

import { SessionHead, type SessionHeadHost } from "../../../src/frontend/ui/session-head";
import { CONFIG_KEY_OWNERS } from "../../../src/frontend/ui/config";
import { DEFAULT_HEAD_BUTTONS, dropHeadButton, headButtonsThatFit, headEditRows, nudgeHeadButton, sanitizeHeadButtons, toggleHeadButton } from "../../../src/frontend/ui/session-head-buttons";
import type { Tab } from "../../../src/frontend/ui/tab-model";
import type { MenuItem } from "../../../src/frontend/ui/kit/menu";
import { closeMenu } from "../../../src/frontend/ui/kit/menu";
import { closePopover } from "../../../src/frontend/ui/kit/popover";
import { undoToast } from "../../../src/frontend/ui/kit/toast";
import { LOCAL_ORIGIN } from "../../../src/frontend/ui/ipc/origin";
import { goldenFace, liveFace } from "../../test-support/session-faces";
import { terminalFrontAvailable } from "../../../src/frontend/ui/terminal-front";
import { copyText } from "../../../src/frontend/ui/copy-table";
import { dispatcher } from "../../../src/frontend/ui/keybindings/registry";

dispatcher.applyOverrides({}); // 建默认键位表（main.ts 启动时做的同一步）
dispatcher.start(); // Esc 走弹层栈（window 上那一个监听）

function tab(sid: string, over: Partial<Tab> = {}): Tab {
  return {
    sessionId: sid, title: sid, background: false, bgName: null, label: null, labelProject: null, projectDir: `/w/${sid}`, face: liveFace("working"), line: null, pinned: false, group: null,
    origin: LOCAL_ORIGIN, activity: null, forkedFromSessionId: null, writers: [], unread: 0, needs: null, backgroundWork: null, pending: [], lastSay: null, ...over,
  } as unknown as Tab;
}

const flush = async (): Promise<void> => {
  for (let i = 0; i < 5; i++) await Promise.resolve();
};

let current: Tab | null;
let shown = { proc: false, injected: false };
const clicked: string[] = [];
function rig(t: Tab | null = tab("a")): { h: SessionHead; host: { more: ReturnType<typeof vi.fn> } } {
  current = t;
  const item = (id: string, label: string, extra: Partial<MenuItem> = {}): MenuItem => ({ id, label, onClick: () => clicked.push(id), ...extra });
  const host = {
    active: () => current,
    viewTerminal: vi.fn(), openCwd: vi.fn(), front: vi.fn(), find: vi.fn(), more: vi.fn(), resume: vi.fn(), attach: vi.fn(), reconnect: vi.fn(), planMark: () => null,
    menuItem: vi.fn((_sid: string, id: string): MenuItem | null => {
      switch (id) {
        case "popOut": return item("popOut", copyText("tabMenu.open.openInWindow"));
        case "account": return item("account", copyText("acct.menu.open"));
        case "pin": return item("pin", copyText("tabMenu.pin.pin"));
        case "group": return { id: "group", label: copyText("tabMenu.collection.add"), submenu: [item("g-new", copyText("tabMenu.collection.new"))] };
        case "close": return item("close", copyText("tabMenu.item.close"), { enabled: false, why: copyText("tabMenu.close.live") });
        case "resume": return { id: "resume", label: copyText("tabMenu.item.resume"), submenu: [] };
        default: return null;
      }
    }),
    displayItems: vi.fn((): MenuItem[] => [
      { label: copyText("stream.injected.toggle"), checked: shown.injected, onClick: () => clicked.push("injected") },
      { label: copyText("stream.proc.toggle"), checked: shown.proc, onClick: () => clicked.push("proc") },
    ]),
    // 换模型 · 切权限档那两颗（判据另在 `session-switch.vitest.ts`）：这里不换，清单问不到。
    switching: { active: () => null, facts: () => new Promise<never>(() => {}), send: vi.fn(), unavailable: () => null, reread: vi.fn(), toast: vi.fn() },
  };
  const h = new SessionHead(host as unknown as SessionHeadHost);
  document.body.appendChild(h.el);
  h.render();
  return { h, host: host as never };
}
const heads = (h: SessionHead): string[] => [...h.el.querySelectorAll<HTMLElement>("[data-head]")].filter((b) => b.style.display !== "none").map((b) => b.dataset.head!);
const btn = (h: SessionHead, id: string): HTMLElement => h.el.querySelector<HTMLElement>(`[data-head="${id}"]`)!;
const menuLabels = (): string[] => [...document.body.querySelectorAll<HTMLElement>("[role=menu] [role^=menuitem]")].map((b) => b.querySelector(".menu-label, [data-label]")?.textContent ?? b.textContent ?? "");
const menuItemEl = (label: string): HTMLElement => {
  const el = [...document.body.querySelectorAll<HTMLElement>("[role=menu] [role^=menuitem]")].find((b) => b.textContent?.startsWith(label));
  if (!el) throw new Error(`菜单里没有「${label}」：${menuLabels().join(" | ")}`);
  return el;
};
const lastButtonsWrite = (): unknown => {
  const e = store.edits.at(-1) as { op: string; path: string[]; value?: unknown }[];
  return e;
};

beforeEach(() => {
  document.body.innerHTML = "";
  store.cfg = {};
  store.edits = [];
  shown = { proc: false, injected: false };
  clicked.length = 0;
  vi.mocked(undoToast).mockClear();
  vi.mocked(terminalFrontAvailable).mockReturnValue(false);
});
afterEach(() => {
  closeMenu();
  closePopover();
});

describe("纯函数：读盘 · 清单 · 勾 · 挪 · 拖 · 放不下", () => {
  it("没设过 ⇒ null（用默认）；认不出的 id 跳过、重复的只留第一个", () => {
    expect(sanitizeHeadButtons(undefined)).toBeNull();
    expect(sanitizeHeadButtons("find")).toBeNull();
    expect(sanitizeHeadButtons(["find", "bogus", "find", 3, "more", "kill", "pin"])).toEqual(["find", "pin"]);
    expect(sanitizeHeadButtons([])).toEqual([]);
  });

  it("默认 ＝ 方案①那一排；「结束会话」与「更多」都不在全集里", () => {
    expect(DEFAULT_HEAD_BUTTONS).toEqual(["cwd", "front", "terminal", "popOut", "account", "display", "find"]);
    const ids = headEditRows([]).map((r) => r.id);
    expect(ids).not.toContain("kill");
    expect(ids).not.toContain("more");
    expect(ids).toHaveLength(11);
  });

  it("清单：上段按顺序勾着，下段按全集顺序；勾上放末尾、取消回下段；Alt+↑↓ 挪一格到头不动", () => {
    expect(headEditRows(["find", "cwd"]).slice(0, 3)).toEqual([{ id: "find", on: true }, { id: "cwd", on: true }, { id: "front", on: false }]);
    expect(toggleHeadButton(["find"], "pin")).toEqual(["find", "pin"]);
    expect(toggleHeadButton(["find", "pin"], "find")).toEqual(["pin"]);
    expect(nudgeHeadButton(["a", "b", "c"] as never, "b" as never, -1)).toEqual(["b", "a", "c"]);
    expect(nudgeHeadButton(["a", "b"] as never, "a" as never, -1)).toEqual(["a", "b"]);
    expect(nudgeHeadButton(["a"] as never, "x" as never, 1)).toEqual(["a"]);
  });

  it("拖：上段之间换位；没勾的拖进上段 ＝ 勾上；勾着的拖进下段 ＝ 取消", () => {
    // 行：find cwd | front terminal popOut account display pin group resume close
    expect(dropHeadButton(["find", "cwd"], "cwd", 0)).toEqual(["cwd", "find"]);
    expect(dropHeadButton(["find", "cwd"], "pin", 1)).toEqual(["find", "pin", "cwd"]);
    expect(dropHeadButton(["find", "cwd"], "find", 2), "紧跟上段最后一颗 ⇒ 还在会话头上").toEqual(["cwd", "find"]);
    expect(dropHeadButton(["find", "cwd"], "find", 6)).toEqual(["cwd"]);
  });

  it("放不下：从最后一颗起收，直到标题够 160（标题本来就短 ⇒ 够它的全长即可）", () => {
    const at = (k: number): number => 400 - 28 * k;
    expect(headButtonsThatFit(8, at, 500, 160)).toBe(8); // 400-224=176 ≥ 160
    expect(headButtonsThatFit(10, at, 500, 160)).toBe(8);
    expect(headButtonsThatFit(10, at, 100, 160), "标题全长 100 ⇒ 留 100 就够").toBe(10);
    expect(headButtonsThatFit(3, () => 0, 500, 160)).toBe(0);
  });
});

describe("会话头那一排", () => {
  it("★ 默认：目录 · 看它的终端 · 在新窗口打开 · 账号 · 显示 · 查找 · ⋯（Linux 没有「切到终端」，不留空位）", () => {
    const { h } = rig();
    expect(heads(h)).toEqual(["cwd", "terminal", "popOut", "account", "display", "find", "more"]);
    vi.mocked(terminalFrontAvailable).mockReturnValue(true);
    const w = rig(tab("w"));
    expect(heads(w.h)).toEqual(["cwd", "front", "terminal", "popOut", "account", "display", "find", "more"]);
  });

  it("★ 照盘上那一格排（读盘时跳过认不出的）；「⋯」永远在最右", async () => {
    store.cfg = { sessionHead: { buttons: ["find", "pin", "bogus", "cwd"] } };
    const { h } = rig();
    await h.loadButtons();
    expect(heads(h)).toEqual(["find", "pin", "cwd", "more"]);
  });

  it("从「⋯」那份菜单借来的那几颗：点了就是那一项（有子菜单 ⇒ 锚在按钮上开）；灰着的说为什么", async () => {
    store.cfg = { sessionHead: { buttons: ["pin", "popOut", "group", "close"] } };
    const { h } = rig();
    await h.loadButtons();
    btn(h, "pin").click();
    btn(h, "popOut").click();
    expect(clicked).toEqual(["pin", "popOut"]);
    btn(h, "group").click();
    expect(menuLabels().some((l) => l.startsWith(copyText("tabMenu.collection.new")))).toBe(true);
    closeMenu();
    expect(btn(h, "close").getAttribute("aria-disabled"), "在跑的关不掉：灰着").toBe("true");
    btn(h, "close").click();
    expect(clicked, "灰着的点了不做事").toEqual(["pin", "popOut"]);
  });

  it("已结束：清单里勾了「恢复」也只出状态那一颗", async () => {
    store.cfg = { sessionHead: { buttons: ["resume", "find"] } };
    const { h } = rig(tab("e", { face: goldenFace("ended") }));
    await h.loadButtons();
    const resumes = [...h.el.querySelectorAll("button")].filter((b) => (b.getAttribute("aria-label") ?? b.textContent ?? "").startsWith(copyText("sessionHead.act.resume")));
    expect(resumes).toHaveLength(1);
    expect(heads(h)).toEqual(["find", "more"]);
  });

  it("「显示 ▾」：两个流开关在它的下拉里；有一个开着 ⇒ 按钮是「开」样（aria-pressed）", () => {
    shown.proc = true;
    const { h } = rig();
    expect(btn(h, "display").getAttribute("aria-pressed")).toBe("true");
    btn(h, "display").click();
    menuItemEl(copyText("stream.proc.toggle")).click();
    expect(clicked).toEqual(["proc"]);
    shown.proc = false;
    const off = rig(tab("b"));
    expect(btn(off.h, "display").getAttribute("aria-pressed")).toBe("false");
  });

  it("「⋯」：交出去的菜单末尾一组是「调整会话头按钮…」，最上面一组是收进去的（没收 ⇒ 空）", () => {
    const { h, host } = rig();
    btn(h, "more").click();
    const extra = host.more.mock.calls[0][2] as { top: MenuItem[]; bottom: MenuItem[] };
    expect(extra.top).toEqual([]);
    expect(extra.bottom.map((i) => i.label)).toEqual([copyText("stream.injected.toggle"), copyText("stream.proc.toggle"), "", copyText("sessionHead.edit.open")]);
  });
});

describe("按钮那一排右键 · 编辑浮层", () => {
  it("★ 右键某一颗：「调整会话头按钮…」·「从会话头移走「x」」；移走 ⇒ 当场落盘、会话头跟着变、toast 可撤", async () => {
    const { h } = rig();
    btn(h, "terminal").dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, clientX: 5, clientY: 5 }));
    const name = copyText("sessionHead.act.terminal");
    expect(menuLabels().map((l) => l.trim())).toEqual([copyText("sessionHead.edit.open"), copyText("sessionHead.edit.remove", { name })]);
    menuItemEl(copyText("sessionHead.edit.remove", { name })).click();
    await flush();
    const want = DEFAULT_HEAD_BUTTONS.filter((x) => x !== "terminal");
    expect(lastButtonsWrite()).toEqual([{ op: "set", path: ["sessionHead", "buttons"], value: want }]);
    expect(heads(h)).not.toContain("terminal");
    expect(vi.mocked(undoToast).mock.calls[0][0]).toBe(copyText("sessionHead.edit.removed", { name }));
    vi.mocked(undoToast).mock.calls[0][1]();
    await flush();
    expect(store.cfg, "撤 ＝ 回到原样：原来没设过 ⇒ 这个键又没了").toEqual({});
    expect(heads(h)).toContain("terminal");
  });

  it("右键「⋯」或空处：只有「调整会话头按钮…」", () => {
    const { h } = rig();
    btn(h, "more").dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, clientX: 5, clientY: 5 }));
    expect(menuLabels().map((l) => l.trim())).toEqual([copyText("sessionHead.edit.open")]);
  });

  it("★ 浮层：标题 · 灰字 · 上段勾着按顺序 · 下段只在「⋯」· 末行「更多」固定；勾一下当场生效、落盘", async () => {
    const { h } = rig();
    h.openEditor();
    const pop = document.body.querySelector<HTMLElement>("[role=dialog]")!;
    expect(pop.getAttribute("aria-label")).toBe(copyText("sessionHead.edit.title"));
    expect(pop.textContent).toContain(copyText("sessionHead.edit.hint"));
    expect(pop.textContent).toContain(copyText("sessionHead.edit.overflow"));
    const rows = (): HTMLElement[] => [...pop.querySelectorAll<HTMLElement>("[data-row]")];
    expect(rows().map((r) => r.dataset.row)).toEqual(["cwd", "front", "terminal", "popOut", "account", "display", "find", "pin", "group", "resume", "close", "more"]);
    const box = (id: string): HTMLInputElement => pop.querySelector<HTMLInputElement>(`[data-row="${id}"] input[type=checkbox]`)!;
    expect(box("cwd").checked).toBe(true);
    expect(box("pin").checked).toBe(false);
    expect(box("more").disabled, "「更多」不能取消").toBe(true);
    expect(pop.querySelector(`[data-row="more"]`)!.textContent).toContain(copyText("sessionHead.edit.fixedMore"));
    expect(pop.querySelector(`[data-row="cwd"]`)!.textContent).toContain(copyText("sessionHead.edit.onlyLocal"));
    expect(pop.querySelector(`[data-row="front"]`)!.textContent).toContain(copyText("sessionHead.edit.onlyWindows"));
    expect(pop.querySelector(`[data-row="resume"]`)!.textContent).toContain(copyText("sessionState.headEdit.onlyEnded"));
    box("pin").click();
    await flush();
    expect(store.cfg).toEqual({ sessionHead: { buttons: [...DEFAULT_HEAD_BUTTONS, "pin"] } });
    expect(heads(h)).toEqual(["cwd", "terminal", "popOut", "account", "display", "find", "pin", "more"]);
    expect(rows().map((r) => r.dataset.row).slice(0, 8), "清单跟着重排：勾上的进上段末尾").toEqual([...DEFAULT_HEAD_BUTTONS, "pin"]);
  });

  it("浮层键盘：Alt+↓ 挪一格（焦点跟着那一行）· 空格勾 / 不勾 · Esc 关、焦点回「⋯」", async () => {
    const { h } = rig();
    h.openEditor();
    const pop = document.body.querySelector<HTMLElement>("[role=dialog]")!;
    const row = (id: string): HTMLElement => pop.querySelector<HTMLElement>(`[data-row="${id}"]`)!;
    row("cwd").focus();
    row("cwd").dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", code: "ArrowDown", altKey: true, bubbles: true }));
    await flush();
    expect((store.cfg.sessionHead as { buttons: string[] }).buttons.slice(0, 2)).toEqual(["front", "cwd"]);
    expect(document.activeElement).toBe(row("cwd"));
    row("cwd").dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", code: "ArrowDown", bubbles: true }));
    expect(document.activeElement, "↓ 走到下一行").toBe(row("terminal"));
    row("terminal").dispatchEvent(new KeyboardEvent("keydown", { key: " ", code: "Space", bubbles: true }));
    await flush();
    expect((store.cfg.sessionHead as { buttons: string[] }).buttons).not.toContain("terminal");
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", code: "Escape", bubbles: true }));
    expect(document.body.querySelector("[role=dialog]")).toBeNull();
    expect(document.activeElement).toBe(btn(h, "more"));
  });

  it("★［恢复默认］＝ 删掉这个键（默认值只在代码一处）＋ toast 可撤", async () => {
    store.cfg = { sessionHead: { buttons: ["find"] } };
    const { h } = rig();
    await h.loadButtons();
    h.openEditor();
    const reset = [...document.body.querySelectorAll<HTMLButtonElement>("[role=dialog] button")].find((b) => b.textContent === copyText("sessionHead.edit.reset"))!;
    reset.click();
    await flush();
    expect(lastButtonsWrite()).toEqual([{ op: "remove", path: ["sessionHead"] }]);
    expect(store.cfg).toEqual({});
    expect(heads(h)).toEqual(["cwd", "terminal", "popOut", "account", "display", "find", "more"]);
    expect(vi.mocked(undoToast).mock.calls[0][0]).toBe(copyText("sessionHead.edit.resetDone"));
    vi.mocked(undoToast).mock.calls[0][1]();
    await flush();
    expect(store.cfg).toEqual({ sessionHead: { buttons: ["find"] } });
  });
});

describe("放不下：从最后一颗起收进「⋯」最上面一组", () => {
  it("★ 标题挤到 160 以下 ⇒ 收；收进去的排在「⋯」最上面（顺序同会话头）；不改用户的设置", () => {
    const { h, host } = rig();
    const title = h.el.querySelector<HTMLElement>("[data-head-title]")!;
    const visible = (): number => heads(h).filter((x) => x !== "more").length;
    Object.defineProperty(title, "clientWidth", { configurable: true, get: () => 300 - 28 * visible() });
    Object.defineProperty(title, "scrollWidth", { configurable: true, get: () => 400 });
    h.fit();
    expect(heads(h)).toEqual(["cwd", "terminal", "popOut", "account", "display", "more"]); // 300-140=160
    btn(h, "more").click();
    const extra = host.more.mock.calls[0][2] as { top: MenuItem[]; bottom: MenuItem[] };
    expect(extra.top.map((i) => i.label)).toEqual([copyText("sessionHead.act.find"), ""]);
    expect(store.edits, "收进去不改设置").toEqual([]);
  });
});

describe("登记", () => {
  it("`sessionHead` 是登记过的顶层键，主人是会话头", () => {
    expect(CONFIG_KEY_OWNERS.sessionHead).toBe("src/frontend/ui/session-head.ts");
  });
});

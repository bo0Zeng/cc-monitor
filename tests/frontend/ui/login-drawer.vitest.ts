/**
 * 登录抽屉（账号 · MCP 同一个，稿 `需手动与登录.md` §2）：每个动作交的命令 · 结果照后端的 `again` 画。
 *
 * - 开抽屉同时 `login-start`（入参 ＝ 交进来的目标 ＋ `browserHere`：本机 ⇒ 真、远端 ⇒ 假）；回包前［打开登录页］转圈。
 * - 回包：［打开登录页］用系统浏览器开 `url` ·［复制链接］进剪贴板 · `auto` 才画「登录页完成后自动结束」· 那一格名字照 `paste`。
 * - ［粘贴］从剪贴板原样填 ·［提交］`login-finish {job, pasted}`：成了 ⇒ 抽屉关、不撤；失败照 `again`：
 *   `paste` ⇒ 那一格下面那一句、作业还在 · `restart` ⇒ 红条 ＋［重新开始］（再发一次 `login-start`）、底部［关闭］· `none` ⇒ 只有那一句。
 * - ［取消］· ✕ · Esc · 点遮罩 ⇒ `login-cancel {job}`，不确认；作业已经没了 ⇒ 不发。
 * - 推来的 `changed {login, key: job}`：`done` ⇒ 抽屉自己关；`failed` ⇒ 红条 ＋［重新开始］。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const { opened } = vi.hoisted(() => ({ opened: [] as string[] }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: (u: string) => void opened.push(u) }));
const { pushes } = vi.hoisted(() => ({ pushes: [] as { origin: string; onChanged: (o: string, topic: string, c: { cells: { key: string | null; rev: null; body: unknown }[]; all: boolean }) => void }[] }));
vi.mock("../../../src/frontend/ui/events", () => ({
  bindEvents: (h: { onChanged: never }, o: { changed: { origin: string; topic: string }[] }) => {
    for (const c of o.changed) if (c.topic === "login") pushes.push({ origin: c.origin, onChanged: h.onChanged });
    return Promise.resolve();
  },
}));

import { invoke } from "@tauri-apps/api/core";
import { openLoginDrawer } from "../../../src/frontend/ui/login-drawer";
import { LOCAL_ORIGIN, type Origin } from "../../../src/frontend/ui/ipc/origin";
import { copyText } from "../../../src/frontend/ui/copy-table";
import { chanArgsJson, chanReply, refusedReply, type ChanCallArgs } from "../../test-support/chan-fake";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
const tick = () => new Promise((r) => setTimeout(r, 0));
const settle = async () => {
  for (let i = 0; i < 8; i++) await tick();
};

const ACCOUNT = { kind: "account" as const, agent: "claude", account: "t1" };
const MCP = { kind: "mcp" as const, agent: "claude", account: "w1", server: "s1", projectDir: null };
const started = (over: Record<string, unknown> = {}) => ({ job: "job-1", url: "https://login.example/x?state=s", paste: "code", auto: true, ...over });

/** 每条帧命令怎么答（缺 ⇒ 当场答好）；记下每一发交了什么。 */
let answer: Record<string, (req: unknown) => unknown> = {};
let clip = "";
const sent = (): [string, string, unknown][] => invokeMock.mock.calls.filter(([c]) => c === "chan_call").map(([, a]) => [(a as ChanCallArgs).origin, (a as ChanCallArgs).op, chanArgsJson(a as ChanCallArgs)]);
const ops = () => sent().map(([, op]) => op);

/** 开过的抽屉（每条判据之前关掉上一条留下的，不让它的撤单落进这一条）。 */
const handles: { close(): Promise<boolean> }[] = [];

beforeEach(async () => {
  for (const h of handles.splice(0)) await h.close();
  document.body.innerHTML = "";
  opened.length = 0;
  clip = "";
  answer = { "login-start": () => started(), "login-finish": () => ({ target: ACCOUNT }), "login-cancel": () => ({ was: true }) };
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string, a: ChanCallArgs & { text?: string }) => {
    if (cmd === "clipboard_read") return clip;
    if (cmd === "clipboard_write") {
      clip = a.text ?? "";
      return null;
    }
    if (cmd !== "chan_call") throw new Error(`没料到：${cmd}`);
    const f = answer[a.op];
    if (!f) throw new Error(`没料到这一问：${a.op}`);
    return chanReply(f(chanArgsJson(a)));
  });
});

const drawer = () => document.querySelector<HTMLElement>("aside[data-login-kind]");
const act = (name: string) => drawer()!.querySelector<HTMLButtonElement>(`[data-action="${name}"]`)!;
const input = () => drawer()!.querySelector<HTMLInputElement>('[data-part="paste"] input')!;

async function open(target: typeof ACCOUNT | typeof MCP = ACCOUNT, origin: Origin = LOCAL_ORIGIN) {
  const h = openLoginDrawer({ origin, target });
  handles.push(h);
  await settle();
  invokeMock.mock.calls.splice(0, invokeMock.mock.calls.findIndex(([c, a]) => c === "chan_call" && (a as ChanCallArgs).op === "login-start"));
  return h;
}

describe("开抽屉 ＝ 起登录", () => {
  it("★ 开的那一下发 login-start（交进来的目标 ＋ browserHere：本机真、远端假）；回包前［打开登录页］转圈、没有粘贴那一格", async () => {
    let release: (v: unknown) => void = () => {};
    answer["login-start"] = () => new Promise((r) => (release = r));
    invokeMock.mockImplementation(async (cmd: string, a: ChanCallArgs) => (cmd === "chan_call" && a.op === "login-start" ? chanReply(await (answer["login-start"](null) as Promise<unknown>)) : null));
    handles.push(openLoginDrawer({ origin: LOCAL_ORIGIN, target: ACCOUNT }));
    await settle();
    expect(sent()).toEqual([[LOCAL_ORIGIN, "login-start", { ...ACCOUNT, browserHere: true }]]);
    expect(act("login-open").dataset.busy).toBe("true");
    expect(drawer()!.querySelector('[data-part="paste"]')).toBeNull();
    release(started());
    await settle();
    expect(act("login-open").dataset.busy).toBeUndefined();
    expect(drawer()!.querySelector('[data-part="paste"]')).not.toBeNull();
  });

  it("远端那台 ⇒ browserHere 假；标题照目标（账号 · MCP），目标行只摆名字", async () => {
    await open(MCP, "gpu-01" as Origin);
    expect(sent()[0]).toEqual(["gpu-01", "login-start", { ...MCP, browserHere: false }]);
    expect(drawer()!.getAttribute("aria-label")).toBe(copyText("login.sheet.mcp"));
    expect(drawer()!.querySelector('[data-part="who"]')!.textContent).toContain("s1");
    expect(drawer()!.querySelector('[data-part="who"]')!.textContent).toContain("gpu-01");
    await open(ACCOUNT);
    expect(drawer()!.getAttribute("aria-label")).toBe(copyText("login.sheet.account"));
  });

  it("★ auto 真 ⇒「登录页完成后自动结束」那一行；假 ⇒ 没有；那一格名字照 paste（code ⇒ 授权码 · redirectUrl ⇒ 回跳地址）", async () => {
    await open();
    expect(drawer()!.querySelector('[data-part="auto"]')?.textContent).toBe(copyText("login.sheet.auto"));
    expect(drawer()!.querySelector('[data-part="paste"] label')?.textContent).toBe(copyText("login.paste.code"));
    answer["login-start"] = () => started({ auto: false, paste: "redirectUrl" });
    await open(MCP, "gpu-01" as Origin);
    expect(drawer()!.querySelector('[data-part="auto"]')).toBeNull();
    expect(drawer()!.querySelector('[data-part="paste"] label')?.textContent).toBe(copyText("login.paste.redirectUrl"));
  });

  it("［打开登录页］用系统浏览器开 url（不发命令）·［复制链接］进剪贴板", async () => {
    await open();
    const before = ops().length;
    act("login-open").click();
    act("login-copy").click();
    await settle();
    expect(opened).toEqual([started().url]);
    expect(clip).toBe(started().url);
    expect(ops().length, "这两样不发命令").toBe(before);
  });
});

describe("粘贴 · 提交 · 结果照 again", () => {
  it("★［粘贴］从剪贴板原样填；［提交］发 login-finish {job, pasted}；成了 ⇒ 抽屉关、不撤", async () => {
    await open();
    clip = "  任意-xyz  ";
    act("login-paste").click();
    await settle();
    expect(input().value).toBe("  任意-xyz  ");
    act("login-submit").click();
    await settle();
    expect(sent().at(-1)).toEqual([LOCAL_ORIGIN, "login-finish", { job: "job-1", pasted: "  任意-xyz  " }]);
    expect(drawer()).toBeNull();
    expect(ops()).not.toContain("login-cancel");
  });

  it("★ again: paste ⇒ 那一格下面那一句（照回包 message）、作业还在、底部仍是［取消］", async () => {
    answer["login-finish"] = () => {
      throw refusedReply("bad_paste", "核心·粘的对不上", { again: "paste" });
    };
    invokeMock.mockImplementation(async (cmd: string, a: ChanCallArgs) => {
      if (cmd !== "chan_call") return null;
      return chanReply(answer[a.op](chanArgsJson(a)));
    });
    await open();
    input().value = "x";
    input().dispatchEvent(new Event("input"));
    act("login-submit").click();
    await settle();
    expect(drawer()!.querySelector('[data-part="paste"]')!.textContent).toContain("核心·粘的对不上");
    expect(drawer()!.querySelector("[data-again]")).toBeNull();
    expect(act("login-bottom").textContent).toBe(copyText("kit.dialog.cancel"));
    act("login-bottom").click();
    await settle();
    expect(sent().at(-1)).toEqual([LOCAL_ORIGIN, "login-cancel", { job: "job-1" }]);
  });

  it("★ again: restart ⇒ 粘贴区收起、红条那一句 ＋［重新开始］（再发一次同一份 login-start）、底部［关闭］、关时不撤", async () => {
    answer["login-finish"] = () => {
      throw refusedReply("refused", "核心·登录没成", { again: "restart" });
    };
    invokeMock.mockImplementation(async (cmd: string, a: ChanCallArgs) => (cmd === "chan_call" ? chanReply(answer[a.op](chanArgsJson(a))) : null));
    await open();
    input().value = "x";
    input().dispatchEvent(new Event("input"));
    act("login-submit").click();
    await settle();
    const bar = drawer()!.querySelector<HTMLElement>('[data-again="restart"]')!;
    expect(bar.textContent).toContain("核心·登录没成");
    expect(drawer()!.querySelector('[data-part="paste"]')).toBeNull();
    expect(act("login-bottom").textContent).toBe(copyText("kit.drawer.close"));
    [...bar.querySelectorAll("button")].find((b) => b.textContent === copyText("login.act.restart"))!.click();
    await settle();
    expect(sent().filter(([, op]) => op === "login-start").map(([, , a]) => a)).toEqual([{ ...ACCOUNT, browserHere: true }, { ...ACCOUNT, browserHere: true }]);
    expect(drawer()!.querySelector('[data-part="paste"]'), "回到起好那一步").not.toBeNull();
  });

  it("★ again: none ⇒ 只有那一句、不给重来、底部［关闭］；起都没起成时也照它", async () => {
    answer["login-start"] = () => {
      throw refusedReply("unsupported", "核心·不支持在 app 里登录", { again: "none" });
    };
    invokeMock.mockImplementation(async (cmd: string, a: ChanCallArgs) => (cmd === "chan_call" ? chanReply(answer[a.op](chanArgsJson(a))) : null));
    await open();
    const bar = drawer()!.querySelector<HTMLElement>('[data-again="none"]')!;
    expect(bar.textContent).toContain("核心·不支持在 app 里登录");
    expect([...bar.querySelectorAll("button")].some((b) => b.textContent === copyText("login.act.restart"))).toBe(false);
    expect(act("login-bottom").textContent).toBe(copyText("kit.drawer.close"));
    act("login-bottom").click();
    await settle();
    expect(ops()).not.toContain("login-cancel");
  });
});

describe("取消 · 推来的结局", () => {
  it("★ ✕ · 点遮罩 ⇒ login-cancel {job}，不确认", async () => {
    await open();
    drawer()!.querySelector<HTMLButtonElement>(`button[aria-label="${copyText("kit.drawer.close")}"]`)!.click();
    await settle();
    expect(sent().at(-1)).toEqual([LOCAL_ORIGIN, "login-cancel", { job: "job-1" }]);
    expect(drawer()).toBeNull();
    await open();
    const scrim = drawer()!.previousElementSibling as HTMLElement;
    scrim.click();
    await settle();
    expect(sent().filter(([, op]) => op === "login-cancel").length).toBe(2);
    expect(drawer()).toBeNull();
  });

  it("★ 推来 changed {login, key: job} done ⇒ 抽屉自己关、不撤；failed ⇒ 红条 ＋［重新开始］；别的作业号不认", async () => {
    await open();
    const feed = pushes.find((p) => p.origin === LOCAL_ORIGIN);
    expect(feed, "起登录之前订好那台的登录推送（一台订一次）").toBeDefined();
    expect(pushes.filter((p) => p.origin === LOCAL_ORIGIN).length).toBe(1);
    const push = (key: string, body: unknown) => feed!.onChanged(LOCAL_ORIGIN, "login", { cells: [{ key, rev: null, body }], all: false });
    push("job-other", { state: "done" });
    expect(drawer()).not.toBeNull();
    push("job-1", { state: "failed", said: "核心·登录程序退了", detail: "登录程序退出时的输出" });
    expect(drawer()!.querySelector('[data-again="restart"]')!.textContent).toContain("核心·登录程序退了");
    await open();
    push("job-1", { state: "done" });
    await settle();
    expect(drawer()).toBeNull();
    expect(ops()).not.toContain("login-cancel");
  });
});

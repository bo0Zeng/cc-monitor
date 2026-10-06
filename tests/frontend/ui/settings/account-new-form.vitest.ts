// 账号表上方就地展开的「新建账号」表单 —— 岔口在表单里、「哪个账号」只问一次、主按钮不是红色。
//
// 本文件判**表单自己**：交出去的请求形状、两支的显隐、校验挡不挡得住绕过 disabled 的点击、撞名那一句是那台说的、
// 「新增命令」是那台答的别名名字、Esc 填过先问。
//
// 预演那几步由那台后端出（帧命令 `accounts-add` 带 `dryRun`）：判据假扮通道那一跳，用罐头答（`accountsFakeInvoke`）
// —— 不在 JS 里再写一份规划器。输入一变表单就问一次，所以每次输入之后要等那一问回来（`settle()`）。
import { describe, it, expect, vi, beforeEach } from "vitest";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invokeMock(...a) }));

import { renderNewAccountForm, aliasHintFor, checkBaseUrl, type NewAccountRequest } from "../../../../src/frontend/ui/settings/account-new-form";
import { accountsFakeInvoke, chanArgsJson, isChanCall, refusedReply, type ChanCallArgs } from "../../../test-support/chan-fake";

/** 表单问的那几发预演（交给了哪台 ＋ 请求体）。 */
const asked: ChanCallArgs[] = [];
/** 下一发预演要不要让那台后端拒（`null` = 照罐头答）。 */
let refuse: string | null = null;

beforeEach(() => {
  asked.length = 0;
  refuse = null;
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string, args: unknown) => {
    if (isChanCall(cmd, args, "accounts-add")) {
      asked.push(args);
      if (refuse !== null) return Promise.reject(refusedReply("refused", refuse));
      return accountsFakeInvoke(args);
    }
    return Promise.reject(new Error(`判据没料到这一发：${cmd}`));
  });
});

/** 等表单那一问回来（通道那几跳全是微任务）。 */
async function settle(): Promise<void> {
  for (let i = 0; i < 30; i++) await Promise.resolve();
}

function form(confirmAnswer = true) {
  const seen: NewAccountRequest[] = [];
  let cancelled = 0;
  const asks: string[] = [];
  const f = renderNewAccountForm("<local>", "devbox", {
    onCreate: (r) => {
      seen.push(r);
    },
    onCancel: () => {
      cancelled++;
    },
    confirm: (spec) => {
      asks.push(spec.title);
      return confirmAnswer;
    },
  });
  const el = f.root;
  document.body.replaceChildren(el);
  const byLabel = (t: string): HTMLInputElement => {
    const l = [...el.querySelectorAll("label")].find((x) => x.textContent === t && x.htmlFor !== "")!;
    return el.querySelector<HTMLInputElement>(`#${l.htmlFor}`)!;
  };
  const name = byLabel("名字");
  const key = byLabel("API key");
  const base = byLabel("地址");
  const cred = byLabel("已有登录的凭据文件");
  const radio = (t: string) => [...el.querySelectorAll<HTMLLabelElement>("label")].find((l) => l.textContent === t)!.querySelector("input")!;
  const btn = (t: string) => [...el.querySelectorAll("button")].find((b) => b.textContent === t)!;
  const type = async (inp: HTMLInputElement, v: string) => {
    inp.value = v;
    inp.dispatchEvent(new Event("input"));
    await settle();
  };
  const pick = async (t: string) => {
    radio(t).click();
    await settle();
  };
  const fieldOf = (inp: HTMLInputElement) => inp.closest<HTMLElement>("div[class]")!.parentElement!.closest<HTMLElement>("div")!;
  return { f, el, seen, name, key, base, cred, radio, btn, type, pick, fieldOf, asks, cancelled: () => cancelled };
}

const SUB = "订阅 · 终端登录";

describe("新建账号表单", () => {
  it("量具自检：两支、四个输入框、两颗按钮都找得到（否则下面全是空真）", () => {
    const f = form();
    for (const x of [f.name, f.key, f.base, f.cred, f.radio(SUB), f.radio("API key")]) expect(x).toBeTruthy();
    expect(f.btn("创建并登录")).toBeTruthy();
    expect(f.btn("取消")).toBeTruthy();
  });

  it("默认是「订阅」：地址 / key 两格藏着、主按钮「创建并登录」；切到 API key 两格出来、主按钮「创建」、导入那一项藏起", async () => {
    const f = form();
    const hidden = (i: HTMLInputElement) => i.closest<HTMLElement>("[hidden]") !== null;
    expect([hidden(f.base), hidden(f.key)]).toEqual([true, true]);
    await f.pick("API key");
    expect([hidden(f.base), hidden(f.key)]).toEqual([false, false]);
    expect(f.btn("创建")).toBeTruthy();
    expect(f.btn("导入已有登录 · 免登录").hidden).toBe(true);
    expect(f.el.textContent).toContain("存于 devbox · 仅显示末四位");
  });

  it("名字下面「新增命令：betacc、betacct」是那台预演答的别名名字", async () => {
    const f = form();
    await f.type(f.name, "b");
    expect(f.el.textContent).toContain("新增命令：betacc、betacct");
    expect(aliasHintFor([])).toBe("");
  });

  it("订阅：交出去的是 {name, kind: subscription, credFile?}，交完各格清空", async () => {
    const f = form();
    await f.type(f.name, "b");
    f.btn("导入已有登录 · 免登录").click();
    await f.type(f.cred, "~/snap.json");
    expect(f.btn("创建并登录").disabled).toBe(false);
    f.btn("创建并登录").click();
    expect(f.seen).toEqual([{ name: "b", kind: "subscription", credFile: "~/snap.json" }]);
    expect([f.name.value, f.cred.value, f.key.value]).toEqual(["", "", ""]);
  });

  it("API key：没填 key 时主按钮是灰的，填了才交；交出去带 key、不带凭据文件；预演从不带 key", async () => {
    const f = form();
    await f.type(f.name, "b");
    await f.pick("API key");
    expect(f.btn("创建").disabled).toBe(true);
    f.btn("创建").click(); // jsdom 里 disabled 不拦 click —— 表单自己也得挡
    expect(f.seen).toEqual([]);
    await f.type(f.key, "  sk-ant-TYPED  ");
    expect(f.btn("创建").disabled).toBe(false);
    f.btn("创建").click();
    expect(f.seen).toEqual([{ name: "b", kind: "api-key", key: "sk-ant-TYPED" }]);
    for (const a of asked) expect((chanArgsJson(a) as { key?: string }).key, "预演带上了 key 的明文").toBeUndefined();
    expect(f.key.value, "交完 key 还留在输入框里").toBe("");
  });

  it("名字不合法：当场说原因、灰掉、绕过 disabled 点也不交；撞名那一句是那台拒的原话", async () => {
    const f = form();
    await f.type(f.name, "B!");
    expect(f.btn("创建并登录").disabled).toBe(true);
    expect(f.el.querySelector("[data-error]")).not.toBeNull();
    f.btn("创建并登录").click();
    expect(f.seen).toEqual([]);
    refuse = "b 已存在 · devbox";
    await f.type(f.name, "b");
    expect(f.el.textContent).toContain("b 已存在 · devbox");
    expect(f.btn("创建并登录").disabled).toBe(true);
  });

  it("名字框里 Enter ＝ 点主按钮（合法时）", async () => {
    const f = form();
    await f.type(f.name, "b");
    f.name.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter" }));
    expect(f.seen).toEqual([{ name: "b", kind: "subscription" }]);
  });

  it("地址填错：表单那一句、主按钮灰（规则读生成物，与后端同一条）", async () => {
    expect(checkBaseUrl("").ok).toBe(true);
    expect(checkBaseUrl("ftp://x").ok).toBe(false);
    const f = form();
    await f.type(f.name, "b");
    await f.pick("API key");
    await f.type(f.base, "ftp://x");
    await f.type(f.key, "sk");
    expect(f.btn("创建").disabled).toBe(true);
  });

  it("Esc：空表单直接收；填过先问「放弃填写的内容」，答不放弃就不收", async () => {
    const a = form();
    expect(await a.f.dismiss()).toBe(true);
    expect([a.asks, a.cancelled()]).toEqual([[], 1]);
    const b = form(false);
    await b.type(b.name, "b");
    expect(await b.f.dismiss()).toBe(false);
    expect([b.asks, b.cancelled()]).toEqual([["放弃填写的内容"], 0]);
  });
});

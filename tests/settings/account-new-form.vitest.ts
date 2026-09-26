// A2（`设计/70 §4.4`）：新建账号那张表单 —— 岔口在表单里、「哪个账号」只问一次、创建不是红色。
//
// 本文件判**表单自己**：交出去的请求形状、两支的显隐、校验挡不挡得住绕过 disabled 的点击。
// 「交出去之后账号分节怎么把 key 写给那个号」在 `accounts-section.vitest.ts` 的 A2 那一族。
//
// 〔DUP2 · J4〕逐字预览的那一行由那台后端出（帧命令 `acct-iso-cmd`）：判据假扮通道那一跳，**照跨语言金样答**
// （`tests/__fixtures__/acct-iso-cmd.golden.json`，后端那侧逐条对生产函数）—— 不在 JS 里再写一份命令构造。
// 输入一变表单就问一次，所以每次输入之后要等那一问回来（`settle()`）。
import { describe, it, expect, vi, beforeEach } from "vitest";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invokeMock(...a) }));

import {
  renderNewAccountForm,
  aliasHintFor,
  checkBaseUrl,
  NEW_ACCOUNT_COPY,
  type NewAccountRequest,
} from "../../src/settings/account-new-form";
import { suggestAliasName } from "../../src/settings/machine-aliases"; // 〔AL1〕随别名那一块搬家
import { acctIsoCmdCase, acctIsoCmdInvoke, isChanCall, type ChanCallArgs } from "../test-support/chan-fake";

/** 表单问的那几发 `acct-iso-cmd`（交给了哪台 ＋ 请求体）。 */
const asked: ChanCallArgs[] = [];

beforeEach(() => {
  asked.length = 0;
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string, args: unknown) => {
    if (isChanCall(cmd, args, "acct-iso-cmd")) {
      asked.push(args);
      return acctIsoCmdInvoke(args);
    }
    return Promise.reject(new Error(`判据没料到这一发：${cmd}`));
  });
});

/** 等表单那一问回来（通道那几跳全是微任务）。 */
async function settle(): Promise<void> {
  for (let i = 0; i < 30; i++) await Promise.resolve();
}

function form() {
  const seen: NewAccountRequest[] = [];
  const el = renderNewAccountForm("<local>", (r) => {
    seen.push(r);
  });
  document.body.replaceChildren(el);
  const q = <T extends Element>(sel: string) => el.querySelector<T>(sel)!;
  const name = q<HTMLInputElement>("input.accounts-maint-name");
  const key = q<HTMLInputElement>(".accounts-new-key input[type=password]");
  const base = q<HTMLInputElement>('.accounts-new-key input[data-field="base-url"]');
  const cred = q<HTMLInputElement>(".accounts-new-adv input");
  const radio = (v: string) => q<HTMLInputElement>(`input[type=radio][value="${v}"]`);
  const btn = (t: string) => [...el.querySelectorAll("button")].find((b) => b.textContent === t)!;
  const type = async (inp: HTMLInputElement, v: string) => {
    inp.value = v;
    inp.dispatchEvent(new Event("input"));
    await settle();
  };
  const pick = async (v: string) => {
    radio(v).checked = true;
    radio(v).dispatchEvent(new Event("change"));
    await settle();
  };
  return { el, seen, name, key, base, cred, radio, btn, type, pick };
}

describe("A2 新建账号表单", () => {
  it("量具自检：两支、三个输入框、两颗按钮都找得到（否则下面全是空真）", () => {
    const f = form();
    for (const x of [f.name, f.key, f.cred, f.radio("subscription"), f.radio("apikey")]) {
      expect(x).toBeTruthy();
    }
    expect(f.btn(NEW_ACCOUNT_COPY.create)).toBeTruthy();
    expect(f.btn(NEW_ACCOUNT_COPY.cancel)).toBeTruthy();
  });

  it("默认是「订阅」：key 那一格藏着、凭据快照那一格在；切到 apikey 两者对调", async () => {
    const f = form();
    const keyBox = f.el.querySelector<HTMLElement>(".accounts-new-key")!;
    const adv = f.el.querySelector<HTMLElement>(".accounts-new-adv")!;
    expect(f.radio("subscription").checked).toBe(true);
    expect([keyBox.hidden, adv.hidden]).toEqual([true, false]);
    expect(f.el.textContent).toContain(NEW_ACCOUNT_COPY.subscriptionHint);
    await f.pick("apikey");
    expect([keyBox.hidden, adv.hidden]).toEqual([false, true]);
    expect(f.el.textContent).toContain(NEW_ACCOUNT_COPY.apikeyHint);
  });

  it("订阅：交出去的是 {name, subscription, credFile?}，交完三格清空", async () => {
    const f = form();
    await f.type(f.name, "b");
    await f.type(f.cred, "/h/snap.json");
    expect(f.btn("创建").disabled).toBe(false);
    f.btn("创建").click();
    expect(f.seen).toEqual([{ name: "b", access: "subscription", credFile: "/h/snap.json" }]);
    expect([f.name.value, f.cred.value, f.key.value]).toEqual(["", "", ""]);
  });

  it("apikey：没填 key 时「创建」是灰的，填了才交；交出去带 key、不带凭据快照", async () => {
    const f = form();
    await f.type(f.name, "b");
    await f.type(f.cred, "/h/snap.json"); // 订阅那一支填过的东西，切走之后不许跟着交出去
    await f.pick("apikey");
    expect(f.btn("创建").disabled).toBe(true);
    f.btn("创建").click(); // jsdom 里 disabled 不拦 click —— 表单自己也得挡
    expect(f.seen).toEqual([]);
    await f.type(f.key, "  sk-ant-TYPED  ");
    expect(f.btn("创建").disabled).toBe(false);
    f.btn("创建").click();
    expect(f.seen).toEqual([{ name: "b", access: "apikey", key: "sk-ant-TYPED" }]);
    expect(f.key.value, "交完 key 还留在输入框里").toBe("");
  });

  it("名字不合法：说出原因、灰掉、绕过 disabled 点也不交", async () => {
    const f = form();
    await f.type(f.name, "a b");
    expect(f.el.querySelector(".accounts-maint-err")!.textContent).not.toBe("");
    expect(f.btn("创建").disabled).toBe(true);
    f.btn("创建").click();
    expect(f.seen).toEqual([]);
    // 非空对照：同一个输入改成合法名，原因消失、按钮亮起。
    await f.type(f.name, "ab");
    expect(f.el.querySelector(".accounts-maint-err")!.textContent).toBe("");
    expect(f.btn("创建").disabled).toBe(false);
  });

  it("〔DUP2 · J4〕命令预览是那台后端答的那一行（`acct-iso-cmd`，交给表单那台）；输入一变就重问、只认最后一问", async () => {
    const f = form();
    await f.type(f.name, "b");
    const want = acctIsoCmdCase(asked.at(-1)!).cmd!;
    expect(asked.at(-1)!.origin).toBe("<local>");
    const pre = () => f.el.querySelector("pre.accounts-wiz-preview")!.textContent!;
    expect(pre()).toContain(want);
    // apikey 那一支还没填 key 时，命令照样预览得出来（命令与 key 无关）。
    await f.pick("apikey");
    expect(pre()).toContain(want);
    // 名字不合法 ⇒ 不问（问了也是拒），预览是空的那一句。
    const n = asked.length;
    await f.type(f.name, "a b");
    expect(asked.length, "名字不合法还去问了后端").toBe(n);
    expect(pre()).toBe(NEW_ACCOUNT_COPY.previewEmpty);
  });

  it("〔DUP2 · J4〕后端拒了（快照路径以 - 开头）⇒ 那一句上屏、「创建」灰、绕过也不交", async () => {
    const f = form();
    await f.type(f.name, "b");
    await f.type(f.cred, "--apply");
    const create = f.btn(NEW_ACCOUNT_COPY.create) as HTMLButtonElement;
    expect(create.disabled).toBe(true);
    expect(f.el.querySelector(".accounts-maint-err")!.textContent).toContain("命令拼不出来");
    create.disabled = false;
    create.click();
    expect(f.seen, "后端拒了还交出去了").toEqual([]);
  });

  it("命令名那一行走唯一那条规则（`suggestAliasName`），名字一变它就跟着变", async () => {
    const f = form();
    await f.type(f.name, "b");
    expect(suggestAliasName("b")).not.toBe("");
    expect(aliasHintFor("b")).toContain(suggestAliasName("b"));
    expect(f.el.textContent).toContain(aliasHintFor("b"));
    await f.type(f.name, "work");
    expect(f.el.textContent).toContain(suggestAliasName("work"));
    expect(f.el.textContent).not.toContain(aliasHintFor("b"));
  });

  it("🔴 `70 §4.3` ②：「创建」不是红色；表单里没有 `.danger`，也没有账号下拉", () => {
    const f = form();
    expect(f.el.querySelectorAll(".danger").length).toBe(0);
    expect(f.btn("创建").className).toContain("settings-btn-primary");
    expect(f.el.querySelector("select")).toBeNull();
  });
});

describe("〔ST2 · `70 §4.4` 线框〕apikey 那一支的 Base URL", () => {
  it("★ 形状关：留空合法（默认上游）· https 收 · 明文 http 只收本机回环 · 别的一律不收", () => {
    expect(checkBaseUrl("  ")).toEqual({ ok: true, value: undefined });
    expect(checkBaseUrl(" https://api.example.com/v1 ")).toEqual({ ok: true, value: "https://api.example.com/v1" });
    for (const loop of ["http://127.0.0.1:8080", "http://localhost:3000/x", "http://[::1]:9"]) {
      expect(checkBaseUrl(loop).ok, `${loop} 是本机回环，该收`).toBe(true);
    }
    for (const bad of ["http://api.example.com", "api.example.com", "ftp://x", "https://"]) {
      expect(checkBaseUrl(bad).ok, `${bad} 不该收`).toBe(false);
    }
  });

  it("★★ 表单：填了 ⇒ 交出去带 baseUrl；留空 ⇒ 请求里**没有**这一格；填错 ⇒「创建」灰着、说为什么、绕过也不交", async () => {
    const f = form();
    await f.pick("apikey");
    await f.type(f.name, "b");
    await f.type(f.key, "sk-x");
    await f.type(f.base, "http://api.example.com");
    const create = f.btn(NEW_ACCOUNT_COPY.create) as HTMLButtonElement;
    expect(create.disabled).toBe(true);
    expect(f.el.querySelector(".accounts-maint-err")?.textContent).toContain("明文 http");
    create.disabled = false;
    create.click();
    expect(f.seen, "形状不对的 Base URL 绕过 disabled 交出去了").toEqual([]);
    await f.type(f.base, "https://api.example.com");
    expect(create.disabled).toBe(false);
    create.click();
    expect(f.seen).toEqual([{ name: "b", access: "apikey", key: "sk-x", baseUrl: "https://api.example.com" }]);
    expect(f.base.value, "交完没清空").toBe("");
    // 留空那一次：请求里没有 baseUrl 这一格（后端那一格不碰）。
    await f.pick("apikey");
    await f.type(f.name, "c");
    await f.type(f.key, "sk-y");
    (f.btn(NEW_ACCOUNT_COPY.create) as HTMLButtonElement).click();
    expect(f.seen[1]).toEqual({ name: "c", access: "apikey", key: "sk-y" });
    expect("baseUrl" in f.seen[1]!).toBe(false);
  });
});


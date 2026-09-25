// A2（`设计/70 §4.4`）：新建账号那张表单 —— 岔口在表单里、「哪个账号」只问一次、创建不是红色。
//
// 本文件判**表单自己**：交出去的请求形状、两支的显隐、校验挡不挡得住绕过 disabled 的点击。
// 「交出去之后账号分节怎么把 key 写给那个号」在 `accounts-section.vitest.ts` 的 A2 那一族。
import { describe, it, expect } from "vitest";
import {
  renderNewAccountForm,
  aliasHintFor,
  checkBaseUrl,
  NEW_ACCOUNT_COPY,
  type NewAccountRequest,
} from "../../src/settings/account-new-form";
import { buildAcctIsoCmd } from "../../src/settings/acct-deploy";
import { suggestAliasName } from "../../src/settings/machine-aliases"; // 〔AL1〕随别名那一块搬家

function form() {
  const seen: NewAccountRequest[] = [];
  const el = renderNewAccountForm((r) => {
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
  const type = (inp: HTMLInputElement, v: string) => {
    inp.value = v;
    inp.dispatchEvent(new Event("input"));
  };
  const pick = (v: string) => {
    radio(v).checked = true;
    radio(v).dispatchEvent(new Event("change"));
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

  it("默认是「订阅」：key 那一格藏着、凭据快照那一格在；切到 apikey 两者对调", () => {
    const f = form();
    const keyBox = f.el.querySelector<HTMLElement>(".accounts-new-key")!;
    const adv = f.el.querySelector<HTMLElement>(".accounts-new-adv")!;
    expect(f.radio("subscription").checked).toBe(true);
    expect([keyBox.hidden, adv.hidden]).toEqual([true, false]);
    expect(f.el.textContent).toContain(NEW_ACCOUNT_COPY.subscriptionHint);
    f.pick("apikey");
    expect([keyBox.hidden, adv.hidden]).toEqual([false, true]);
    expect(f.el.textContent).toContain(NEW_ACCOUNT_COPY.apikeyHint);
  });

  it("订阅：交出去的是 {name, subscription, credFile?}，交完三格清空", () => {
    const f = form();
    f.type(f.name, "b");
    f.type(f.cred, "/h/snap.json");
    expect(f.btn("创建").disabled).toBe(false);
    f.btn("创建").click();
    expect(f.seen).toEqual([{ name: "b", access: "subscription", credFile: "/h/snap.json" }]);
    expect([f.name.value, f.cred.value, f.key.value]).toEqual(["", "", ""]);
  });

  it("apikey：没填 key 时「创建」是灰的，填了才交；交出去带 key、不带凭据快照", () => {
    const f = form();
    f.type(f.name, "b");
    f.type(f.cred, "/h/snap.json"); // 订阅那一支填过的东西，切走之后不许跟着交出去
    f.pick("apikey");
    expect(f.btn("创建").disabled).toBe(true);
    f.btn("创建").click(); // jsdom 里 disabled 不拦 click —— 表单自己也得挡
    expect(f.seen).toEqual([]);
    f.type(f.key, "  sk-ant-TYPED  ");
    expect(f.btn("创建").disabled).toBe(false);
    f.btn("创建").click();
    expect(f.seen).toEqual([{ name: "b", access: "apikey", key: "sk-ant-TYPED" }]);
    expect(f.key.value, "交完 key 还留在输入框里").toBe("");
  });

  it("名字不合法：说出原因、灰掉、绕过 disabled 点也不交", () => {
    const f = form();
    f.type(f.name, "a b");
    expect(f.el.querySelector(".accounts-maint-err")!.textContent).not.toBe("");
    expect(f.btn("创建").disabled).toBe(true);
    f.btn("创建").click();
    expect(f.seen).toEqual([]);
    // 非空对照：同一个输入改成合法名，原因消失、按钮亮起。
    f.type(f.name, "ab");
    expect(f.el.querySelector(".accounts-maint-err")!.textContent).toBe("");
    expect(f.btn("创建").disabled).toBe(false);
  });

  it("命令预览与真正要跑的那条**同源**（`buildAcctIsoCmd`），不是手抄一份", () => {
    const f = form();
    f.type(f.name, "b");
    const want = buildAcctIsoCmd({ kind: "add-apply", name: "b" });
    expect(want.ok).toBe(true);
    const pre = f.el.querySelector("pre.accounts-wiz-preview")!.textContent!;
    expect(pre).toContain(want.ok ? want.cmd : "∅");
    // apikey 那一支还没填 key 时，命令照样预览得出来（命令与 key 无关）。
    f.pick("apikey");
    expect(f.el.querySelector("pre.accounts-wiz-preview")!.textContent).toContain(
      want.ok ? want.cmd : "∅",
    );
  });

  it("命令名那一行走唯一那条规则（`suggestAliasName`），名字一变它就跟着变", () => {
    const f = form();
    f.type(f.name, "b");
    expect(suggestAliasName("b")).not.toBe("");
    expect(aliasHintFor("b")).toContain(suggestAliasName("b"));
    expect(f.el.textContent).toContain(aliasHintFor("b"));
    f.type(f.name, "work");
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

  it("★★ 表单：填了 ⇒ 交出去带 baseUrl；留空 ⇒ 请求里**没有**这一格；填错 ⇒「创建」灰着、说为什么、绕过也不交", () => {
    const f = form();
    f.pick("apikey");
    f.type(f.name, "b");
    f.type(f.key, "sk-x");
    f.type(f.base, "http://api.example.com");
    const create = f.btn(NEW_ACCOUNT_COPY.create) as HTMLButtonElement;
    expect(create.disabled).toBe(true);
    expect(f.el.querySelector(".accounts-maint-err")?.textContent).toContain("明文 http");
    create.disabled = false;
    create.click();
    expect(f.seen, "形状不对的 Base URL 绕过 disabled 交出去了").toEqual([]);
    f.type(f.base, "https://api.example.com");
    expect(create.disabled).toBe(false);
    create.click();
    expect(f.seen).toEqual([{ name: "b", access: "apikey", key: "sk-x", baseUrl: "https://api.example.com" }]);
    expect(f.base.value, "交完没清空").toBe("");
    // 留空那一次：请求里没有 baseUrl 这一格（后端那一格不碰）。
    f.pick("apikey");
    f.type(f.name, "c");
    f.type(f.key, "sk-y");
    (f.btn(NEW_ACCOUNT_COPY.create) as HTMLButtonElement).click();
    expect(f.seen[1]).toEqual({ name: "c", access: "apikey", key: "sk-y" });
    expect("baseUrl" in f.seen[1]!).toBe(false);
  });
});


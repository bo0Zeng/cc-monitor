/**
 * 会话头那两颗（换模型 · 切权限档，`session-switch.ts`）：字 · 灰不灰 · 清单 · 交哪条命令 · 换的结局，全照那台给的。
 * 替身：宿主（清单 · 交命令 · 握手说做不到的那一句 · toast）；`session-writes.ts` 那两条经通道替身核入参与结局。夹具只造结构。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

const sent: { op: string; body: Record<string, unknown> }[] = [];
let answer: (op: string) => unknown = () => ({});

vi.mock("../../../src/comms/inward/chan", async (importOriginal) => {
  const real = await importOriginal<typeof import("../../../src/comms/inward/chan")>();
  return {
    ...real,
    chan: {
      ...real.chan,
      call: async (_origin: string, op: string, body: Uint8Array) => {
        sent.push({ op, body: JSON.parse(new TextDecoder().decode(body)) });
        const v = answer(op);
        if (v instanceof Error) throw v;
        return new TextEncoder().encode(JSON.stringify(v));
      },
    },
  };
});

import { ChanError } from "../../../src/comms/inward/chan";
import { SwitchChips, afterSwitch, chipFace, type SwitchHost, type SwitchLocal, type Switched } from "../../../src/frontend/ui/session-switch";
import { switchModel, switchPermission } from "../../../src/frontend/ui/session-writes";
import { copyText } from "../../../src/frontend/ui/copy-table";
import type { Tab } from "../../../src/frontend/ui/tab-model";
import type { OpenFacts } from "../../../src/frontend/ui/generated/OpenFacts";
import type { Allow } from "../../../src/frontend/ui/generated/Allow";
import type { SessionFace } from "../../../src/frontend/ui/generated/SessionFace";
import { liveFace } from "../../test-support/session-faces";

const YES = { model: { can: "yes" }, permission: { can: "yes" } } as const;
/** 活着那一张脸，换模型 · 切权限档那两格照给的。 */
const switchFace = (sw: { model: Allow; permission: Allow }): SessionFace => {
  const f = liveFace("idle");
  f.can.model = sw.model;
  f.can.permission = sw.permission;
  return f;
};

function tab(over: Partial<Tab> = {}): Tab {
  return { sessionId: "s1", title: "orders", origin: "devbox", agent: "claude", modelText: "核心·Opus 4.5", modelPick: null, permissionMode: "acceptEdits", permissionModeText: "核心·自动接受编辑", face: switchFace(YES), ...over } as unknown as Tab;
}

const FACTS: OpenFacts = {
  recent: [],
  tmux: true,
  fork: null,
  agents: [
    {
      kind: "claude",
      text: "核心·Claude Code",
      models: [
        { value: "sonnet", text: "核心·Sonnet" },
        { value: "opus", text: "核心·Opus" },
      ],
      modelDefault: { value: "default", text: "核心·默认" },
      modelNote: "核心·换模型也改此号以后新会话的默认",
      permissionModes: [
        { value: "default", text: "核心·逐个确认" },
        { value: "acceptEdits", text: "核心·自动接受编辑" },
        { value: "plan", text: "核心·规划模式" },
      ],
    },
  ],
} as unknown as OpenFacts;

const local = (over: Partial<SwitchLocal> = {}): SwitchLocal => ({ pending: null, over: {}, ...over });
const flush = async (): Promise<void> => {
  for (let i = 0; i < 10; i++) await new Promise((r) => setTimeout(r, 0));
};

function rig(t: Tab, opts: { facts?: Promise<OpenFacts>; unavailable?: string | null } = {}) {
  let release: (r: Switched) => void = () => {};
  const host = {
    active: () => t,
    facts: vi.fn(() => opts.facts ?? Promise.resolve(FACTS)),
    send: vi.fn((_t: Tab, _k: string, _v: string) => new Promise<Switched>((r) => (release = r))),
    unavailable: vi.fn(() => opts.unavailable ?? null),
    reread: vi.fn(),
    toast: vi.fn(),
  } satisfies SwitchHost;
  const c = new SwitchChips(host);
  document.body.appendChild(c.el);
  c.render(t);
  const chip = (k: "model" | "permission") => c.el.querySelector<HTMLButtonElement>(`[data-switch="${k}"]`)!;
  return { c, host, chip, done: (r: Switched) => release(r) };
}
const menuItems = (): string[] => [...document.querySelectorAll<HTMLElement>('[role="menu"] [role^="menuitem"], [role="menu"] [role="presentation"]')].map((i) => `${i.getAttribute("aria-checked") === "true" ? "✓" : ""}${i.textContent ?? ""}`);
const choose = (label: string): void => [...document.querySelectorAll<HTMLElement>('[role="menu"] [role^="menuitem"]')].find((i) => i.textContent === label)!.click();

beforeEach(() => {
  document.body.replaceChildren();
  sent.length = 0;
  answer = () => ({});
});

describe("字：照会话事实那两格", () => {
  it("★ 写 modelText · permissionModeText；没有 ⇒ 写这颗是什么", () => {
    const { chip } = rig(tab());
    expect([chip("model").textContent, chip("permission").textContent]).toEqual(["核心·Opus 4.5", "核心·自动接受编辑"]);
    document.body.replaceChildren();
    const bare = rig(tab({ modelText: null, permissionModeText: null }));
    expect([bare.chip("model").textContent, bare.chip("permission").textContent]).toEqual([copyText("session.model.title"), copyText("session.permission.title")]);
  });
});

describe("灰不灰：照脸上的 can.model · can.permission · 那台握手说做不到的那一句", () => {
  it("★ 一样说不能 ⇒ 那一颗灰、悬停写那台那一句；另一颗照常", () => {
    const f = chipFace(tab({ face: switchFace({ model: { can: "no", why: "核心·本轮结束后可换模型" }, permission: { can: "yes" } }) }), "model", local(), null, 2);
    expect(f).toEqual({ text: "核心·Opus 4.5", enabled: false, why: "核心·本轮结束后可换模型", spinning: false });
    expect(chipFace(tab({ face: switchFace({ model: { can: "no", why: "x" }, permission: { can: "yes" } }) }), "permission", local(), null, 3).enabled).toBe(true);
    const { chip } = rig(tab({ face: switchFace({ model: { can: "no", why: "核心·本轮结束后可换模型" }, permission: { can: "yes" } }) }));
    expect(chip("model").getAttribute("aria-disabled")).toBe("true");
    expect(chip("permission").hasAttribute("aria-disabled")).toBe(false);
  });

  it("那台握手说做不到这条命令 ⇒ 灰、写那一句（压过 switch 的能）", () => {
    expect(chipFace(tab(), "permission", local(), "核心·没有 tmux", 3)).toEqual({ text: "核心·自动接受编辑", enabled: false, why: "核心·没有 tmux", spinning: false });
  });

  it("那台看不见 / 没收到过脸 ⇒ 灰、写说不清那一句；那一家清单是空的 ⇒ 只显示字", () => {
    expect(chipFace(tab({ face: null }), "model", local(), null, null)).toMatchObject({ enabled: false, why: copyText("sessionState.unseen.tooltip") });
    expect(chipFace(tab(), "model", local(), null, 0)).toEqual({ text: "核心·Opus 4.5", enabled: false, why: null, spinning: false });
  });

  it("灰着的那一颗点了不开菜单", async () => {
    const { chip, host } = rig(tab({ face: switchFace({ model: { can: "no", why: "w" }, permission: { can: "no", why: "w" } }) }));
    chip("model").click();
    await flush();
    expect(document.querySelector('[role="menu"]')).toBeNull();
    expect(host.send).not.toHaveBeenCalled();
  });
});

describe("清单与交的那条命令", () => {
  it("★ 模型：组名 ·「默认」在前 · 那一家的模型 · 底下那一行；选了交 session-model 的那一个原值", async () => {
    const { chip, host } = rig(tab());
    await flush();
    chip("model").click();
    await flush();
    expect(menuItems()).toEqual([copyText("session.model.title"), "核心·默认", "核心·Sonnet", "核心·Opus", "核心·换模型也改此号以后新会话的默认"]);
    choose("核心·Sonnet");
    expect(host.send).toHaveBeenCalledWith(expect.objectContaining({ sessionId: "s1" }), "model", "sonnet");
  });

  it("★ 模型：没在这里换过 ⇒ 勾会话事实说的那一项（modelPick）；认不出 ⇒ 一项都不勾", async () => {
    const { chip } = rig(tab({ modelPick: "opus" }));
    await flush();
    chip("model").click();
    await flush();
    expect(menuItems()).toEqual([copyText("session.model.title"), "核心·默认", "核心·Sonnet", "✓核心·Opus", "核心·换模型也改此号以后新会话的默认"]);
  });

  it("★ 权限档：勾此刻那一档（原值相等）；选了交 session-permission", async () => {
    const { chip, host } = rig(tab());
    await flush();
    chip("permission").click();
    await flush();
    expect(menuItems()).toEqual([copyText("session.permission.title"), "核心·逐个确认", "✓核心·自动接受编辑", "核心·规划模式"]);
    choose("核心·规划模式");
    expect(host.send).toHaveBeenCalledWith(expect.anything(), "permission", "plan");
  });

  it("按会话事实的 agent 认那一家；清单问不到 ⇒ 菜单里一行灰项写那一句", async () => {
    const { chip } = rig(tab(), { facts: Promise.reject(new Error("核心·够不着 devbox")) });
    await flush();
    chip("model").click();
    await flush();
    expect(menuItems()).toEqual([copyText("session.model.title"), "核心·够不着 devbox"]);
  });

  it("入参：session-model {sid, model} · session-permission {sid, mode}", async () => {
    answer = () => ({ result: "done", model: "Sonnet 4.5", mode: "plan", modeText: "核心·规划模式" });
    await switchModel("devbox", "s1", "sonnet");
    await switchPermission("devbox", "s1", "plan");
    expect(sent).toEqual([
      { op: "session-model", body: { sid: "s1", model: "sonnet" } },
      { op: "session-permission", body: { sid: "s1", mode: "plan" } },
    ]);
  });
});

describe("正在换 · 换成了 · 没换成", () => {
  it("★ 选了 ⇒ 两颗都不可点、换的那颗写选的那一项并转圈；读回 done 才换字、重问事实；事实那一格变了 ⇒ 照后端", async () => {
    const t = tab();
    const { c, chip, host, done } = rig(t);
    await flush();
    chip("model").click();
    await flush();
    choose("核心·Sonnet");
    expect(chip("model").textContent).toBe("核心·Sonnet");
    expect(chip("model").dataset.pending).toBe("true");
    expect([chip("model").getAttribute("aria-disabled"), chip("permission").getAttribute("aria-disabled")]).toEqual(["true", "true"]);
    done({ done: true, text: "Sonnet 4.5", said: "核心·已换成" });
    await flush();
    expect(chip("model").textContent).toBe("Sonnet 4.5");
    expect(chip("model").hasAttribute("aria-disabled")).toBe(false);
    expect(host.reread).toHaveBeenCalledWith("s1");
    expect(host.toast, "换成了、读回有字 ⇒ 不出 toast").not.toHaveBeenCalled();
    t.modelText = "核心·Sonnet 4.6";
    c.render(t);
    expect(chip("model").textContent, "下一份事实里那一格变了 ⇒ 照后端").toBe("核心·Sonnet 4.6");
  });

  it("★ 没成 ⇒ 回原字、toast 写那台那一句；没那一句 ⇒「结果未知」", async () => {
    const { chip, host, done } = rig(tab());
    await flush();
    chip("permission").click();
    await flush();
    choose("核心·规划模式");
    done({ done: false, text: null, said: "核心·先答完需手动" });
    await flush();
    expect(chip("permission").textContent).toBe("核心·自动接受编辑");
    expect(host.reread, "没成 ⇒ 不当换成了").not.toHaveBeenCalled();
    expect(host.toast).toHaveBeenCalledWith("核心·先答完需手动", "orders", "warn");
    expect(afterSwitch("model", { done: false, text: null, said: null })).toEqual({ text: null, toast: { title: copyText("session.model.unsure"), level: "warn" } });
    expect(afterSwitch("permission", { done: true, text: null, said: "核心·没变" })).toEqual({ text: null, toast: { title: "核心·没变", level: "info" } });
  });

  it("结局读法：被拒 ⇒ 那台那一句；发出去了没等到回话 ⇒ 不说（结果未知）；回包照 result · 读回的字 · said", async () => {
    answer = () => new ChanError({ layer: "peer", why: "refused", body: new TextEncoder().encode(JSON.stringify({ code: "no_tmux", message: "核心·没有 tmux" })) }, "核心·没有 tmux");
    expect(await switchModel("devbox", "s1", "opus")).toEqual({ done: false, text: null, said: "核心·没有 tmux" });
    answer = () => new ChanError({ layer: "hop", at: { idx: 0, tag: "wait" }, reach: "Sent", why: "Overrun" }, "壳·无应答");
    expect(await switchPermission("devbox", "s1", "plan")).toEqual({ done: false, text: null, said: null });
    answer = () => ({ result: "unsure", model: null, said: "核心·换模型结果未知" });
    expect(await switchModel("devbox", "s1", "opus")).toEqual({ done: false, text: null, said: "核心·换模型结果未知" });
    answer = () => ({ result: "done", mode: "plan", modeText: "核心·规划模式" });
    expect(await switchPermission("devbox", "s1", "plan")).toEqual({ done: true, text: "核心·规划模式", said: null });
  });
});

/**
 * 要求（用户原话）：「往每个号的 settings.json 里写 env / 这个可以变成可选, 像是生成命令一样让用户自己粘贴」——
 * 机器页「终端」栏里一项「让直接敲的 claude 也走中转（可选）」：后端只读那台的设置文件判装没装、对不对，
 * 并生成一段要合并进 env 的内容，给「复制」按钮与贴到哪、合并不要覆盖、新开会话才生效；旁边逐条写清四条代价。
 * 界面只画、只问那台后端（`relay-optin`），不读、不写那份文件；成品按形状严格收（金样 `tests/__fixtures__/relay-optin.golden.json`，后端产出同一份）。
 */
import { describe, expect, it, vi, beforeEach } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import { RelayOptinSection } from "../../../../src/frontend/ui/settings/relay-optin-section";
import { decodeRelayOptin, type RelayOptinReport } from "../../../../src/frontend/ui/relay-optin-reads";
import { setCurrentMachine, __resetMachineContextForTests } from "../../../../src/frontend/ui/settings/machine-context";
import { copyText } from "../../../../src/frontend/ui/copy-table";
import { chanReply, refusedReply, type ChanCallArgs } from "../../../test-support/chan-fake";
import { REPO_ROOT } from "../../../test-support/repo-root";
import golden from "../../../__fixtures__/relay-optin.golden.json";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
const asked = () => invokeMock.mock.calls.filter(([c]) => c === "chan_call").map(([, a]) => [(a as ChanCallArgs).origin, (a as ChanCallArgs).op]);
const tick = () => new Promise((r) => setTimeout(r, 0));
const settle = async () => {
  for (let i = 0; i < 6; i++) await tick();
};

const SNIPPET = '{\n  "env": {\n    "ANTHROPIC_BASE_URL": "http://127.0.0.1:8788/kkkk/t/claude-code/_"\n  }\n}';
const rep = (over: Partial<RelayOptinReport> = {}): RelayOptinReport => ({
  state: "absent",
  note: "",
  missing: "",
  source: "/h/.claude/settings.json",
  snippet: SNIPPET,
  listening: true,
  ...over,
});

/** 按机器答（每台一份；`reply` 可以是抛出的拒绝）。 */
function backend(by: (origin: string) => unknown) {
  invokeMock.mockImplementation(async (cmd: string, a: ChanCallArgs) => {
    if (cmd !== "chan_call" || a.op !== "relay-optin") return undefined;
    return by(a.origin);
  });
}

const part = (root: HTMLElement, p: string) => root.querySelector<HTMLElement>(`[data-part="${p}"]`);

async function opened(r: () => unknown): Promise<RelayOptinSection> {
  backend(r);
  const s = new RelayOptinSection();
  document.body.replaceChildren(s.element);
  s.element.open = true;
  s.element.dispatchEvent(new Event("toggle"));
  await settle();
  return s;
}

beforeEach(() => {
  invokeMock.mockReset();
  __resetMachineContextForTests();
});

describe("`relay-optin` 成品按形状严格收", () => {
  it("金样原样收下；多一格 / 缺一格 / 认不得的态 / 类型不对 ⇒ 抛「对不上」；已装 ⇒ 片段是 null", () => {
    expect(decodeRelayOptin(golden)).toEqual(golden);
    const bad = (mut: (g: Record<string, unknown>) => void) => {
      const g = JSON.parse(JSON.stringify(golden)) as Record<string, unknown>;
      mut(g);
      return () => decodeRelayOptin(g);
    };
    expect(bad((g) => (g.extra = 1))).toThrow(/shape mismatch/);
    expect(bad((g) => delete g.missing)).toThrow(/shape mismatch/);
    expect(bad((g) => (g.state = "maybe"))).toThrow(/shape mismatch/);
    expect(bad((g) => (g.snippet = 0))).toThrow(/shape mismatch/);
    expect(bad((g) => (g.listening = "yes"))).toThrow(/shape mismatch/);
    expect(decodeRelayOptin({ ...golden, state: "installed", snippet: null }).snippet).toBeNull();
  });

  it("读口只问 `relay-optin` 那一条，源码里没有别的通道调用与 Tauri 命令；界面那一块不碰任何读写", () => {
    const reads = readFileSync(resolve(REPO_ROOT, "src/frontend/ui/relay-optin-reads.ts"), "utf8");
    expect([...reads.matchAll(/chan\.call\([^,]+,\s*"([a-z-]+)"/g)].map((m) => m[1])).toEqual(["relay-optin"]);
    expect(reads).not.toMatch(/\binvoke\s*\(|\bcommands\./);
    const section = readFileSync(resolve(REPO_ROOT, "src/frontend/ui/settings/relay-optin-section.ts"), "utf8");
    expect(section).not.toMatch(/chan\.call\(|\binvoke\s*\(|\bcommands\./);
  });
});

describe("机器页「终端」栏：让直接敲的 claude 也走中转（可选）", () => {
  it("构造零 I/O；第一次展开才问当前这台，再收起展开不重问", async () => {
    backend(() => chanReply(rep()));
    const s = new RelayOptinSection();
    document.body.replaceChildren(s.element);
    await settle();
    expect(asked()).toEqual([]);
    expect(s.element.querySelector("summary")!.textContent).toBe(copyText("relayOptin.section.title"));
    s.element.open = true;
    s.element.dispatchEvent(new Event("toggle"));
    await settle();
    expect(asked()).toEqual([["<local>", "relay-optin"]]);
    s.element.open = false;
    s.element.dispatchEvent(new Event("toggle"));
    s.element.open = true;
    s.element.dispatchEvent(new Event("toggle"));
    await settle();
    expect(asked()).toHaveLength(1);
  });

  it("没装 ⇒ 状态一句 ＋ 待贴块（内容就是那台给的那一段 · 贴到哪 · 合并别覆盖 · 新开会话才生效 · 复制）＋ 四条代价逐条", async () => {
    const s = await opened(() => chanReply(rep()));
    const st = part(s.element, "state")!;
    expect([st.dataset.state, st.textContent]).toEqual(["absent", copyText("relayOptin.state.absent")]);
    const out = s.element.querySelector<HTMLTextAreaElement>(".paste-block-out")!;
    expect(out.value).toBe(SNIPPET);
    expect(s.element.querySelector(".paste-block-target")!.textContent).toContain(copyText("relayOptin.snippet.target", { source: "/h/.claude/settings.json" }));
    expect(s.element.querySelector(".paste-block-merge")!.textContent).toBe(copyText("relayOptin.snippet.merge"));
    expect(s.element.querySelector(".paste-block-activation")!.textContent).toContain(copyText("relayOptin.snippet.activation"));
    expect(s.element.querySelector(".paste-block-copy")).not.toBeNull();
    const costs = [...s.element.querySelectorAll<HTMLElement>("[data-part=costs] li")];
    expect(costs.map((li) => [li.dataset.cost, li.textContent])).toEqual([
      ["backend", copyText("relayOptin.cost.backend")],
      ["key", copyText("relayOptin.cost.key")],
      ["allAccounts", copyText("relayOptin.cost.allAccounts")],
      ["apiKey", copyText("relayOptin.cost.apiKey")],
    ]);
    expect(part(s.element, "not-listening")).toBeNull();
  });

  it("过期 ⇒ 说「贴的内容过期了」并给新的那一段；已装 ⇒ 说已装、没有待贴块", async () => {
    const fresh = SNIPPET.replace("kkkk", "nnnn");
    let s = await opened(() => chanReply(rep({ state: "stale", snippet: fresh })));
    expect(part(s.element, "state")!.textContent).toBe(copyText("relayOptin.state.stale"));
    expect(part(s.element, "state")!.className).toContain("is-bad");
    expect(s.element.querySelector<HTMLTextAreaElement>(".paste-block-out")!.value).toBe(fresh);
    s = await opened(() => chanReply(rep({ state: "installed", snippet: null })));
    expect(part(s.element, "state")!.textContent).toBe(copyText("relayOptin.state.installed"));
    expect(part(s.element, "state")!.className).toContain("is-ok");
    expect(s.element.querySelector(".paste-block")).toBeNull();
  });

  it("读不了 ⇒ 说那台给的原因、不说成没装；别的地址 ⇒ 另一句；中转没在跑 ⇒ 多一句；生成不了 ⇒ 说为什么、没有待贴块", async () => {
    let s = await opened(() => chanReply(rep({ state: "unreadable", note: "读不了这份文件（权限不够），装没装说不清。" })));
    expect(part(s.element, "state")!.textContent).toBe("读不了这份文件（权限不够），装没装说不清。");
    expect(part(s.element, "state")!.textContent).not.toContain(copyText("relayOptin.state.absent"));
    s = await opened(() => chanReply(rep({ state: "other" })));
    expect(part(s.element, "state")!.textContent).toBe(copyText("relayOptin.state.other"));
    s = await opened(() => chanReply(rep({ listening: false })));
    expect(part(s.element, "not-listening")!.textContent).toBe(copyText("relayOptin.relay.notListening"));
    s = await opened(() => chanReply(rep({ snippet: null, missing: "还没有钥匙" })));
    expect(part(s.element, "missing")!.textContent).toBe("还没有钥匙");
    expect(s.element.querySelector(".paste-block")).toBeNull();
  });

  it("那台答不了 ⇒ 说答不了（带原话），不说成没装", async () => {
    const s = await opened(() => {
      throw refusedReply("failed", "这台机器上没有 HOME");
    });
    const f = part(s.element, "failed")!;
    expect(f.textContent).toContain("这台机器上没有 HOME");
    expect(part(s.element, "state")).toBeNull();
  });

  it("展开着切机器 ⇒ 问新的那一台；旧那台晚到的答复不盖掉新的", async () => {
    let releaseOld: (v: unknown) => void = () => {};
    const s = await opened(() => chanReply(rep()));
    expect(asked()).toEqual([["<local>", "relay-optin"]]);
    invokeMock.mockReset();
    invokeMock.mockImplementation(async (cmd: string, a: ChanCallArgs) => {
      if (cmd !== "chan_call") return undefined;
      if (a.origin === "devbox") return new Promise((r) => (releaseOld = r));
      return chanReply(rep({ state: "installed", snippet: null }));
    });
    setCurrentMachine("devbox");
    await settle();
    setCurrentMachine("laptop");
    await settle();
    releaseOld(chanReply(rep({ state: "stale" })));
    await settle();
    expect(asked()).toEqual([
      ["devbox", "relay-optin"],
      ["laptop", "relay-optin"],
    ]);
    expect(part(s.element, "state")!.dataset.state, "旧那台的晚到答复盖掉了当前这台").toBe("installed");
  });
});

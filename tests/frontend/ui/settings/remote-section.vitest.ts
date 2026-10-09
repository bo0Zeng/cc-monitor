// F43：指纹重置按钮显隐纯逻辑。remote-section 的 DOM 主体重(拉整卡),这里只钉住
// 「有固化指纹才显示重置按钮」这条判定,防未来误改成空指纹也显示(重置无意义)。
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
// F56：写入/读取都走 config.ts；mock 掉以测 jump write→read 往返。
// S1：写入口从 writeRemoteConfig（整表覆盖，已取消导出）改为 patchRemoteConfig（局部合并）。 〔散文墓碑〕
// config 写只交补丁；替身把补丁应用到 `loadConfig` 摆的那份上，写完的整份交 `fakeCfg.saved`。
vi.mock("../../../../src/frontend/ui/config", async (orig) => (await import("../config-patch-fake")).mockedConfigModule(orig));
// S3：把整个 IPC 面 mock 成一个**会记账的 Proxy** —— 用来钉「渲染机器列表时零次
// 后端调用」。这比源码扫描强：扫描只能证明「没 import」，证明不了「渲染时没调」。
const { ipcCalls, chanOps, chanSent, ipcReplies } = vi.hoisted(() => ({
  ipcCalls: [] as string[],
  /** 经 `commands.chan_call` 发出去的那几问的 op（按发出顺序）。 */
  chanOps: [] as string[],
  /** 经 `commands.chan_call` 发出去的那几问：问谁 · op · 请求体（按发出顺序）。 */
  chanSent: [] as { origin: string; op: string; req: unknown }[],
  /** 按命令名设定返回值；没设的一律 resolve(undefined)。 */
  ipcReplies: new Map<string, unknown>(),
}));
// `onTestConnection` 在进 try 之前就 `new Channel<ConnectStage>()`（连接分阶段泳道）。
// jsdom 里没有 Tauri 宿主，那句会抛 —— 而调用点是 `() => void this.onTestConnection()`，
// 抛出去变成一条被吞掉的 unhandled rejection：**按钮点了什么都不发生，测试也看不出原因**。
vi.mock("@tauri-apps/api/core", () => ({
  Channel: class {
    onmessage: ((v: unknown) => void) | null = null;
  },
  invoke: vi.fn(),
}));
vi.mock("../../../../src/frontend/ui/ipc/commands", () => ({
  commands: new Proxy(
    {},
    {
      get: (_t, name: string) => (...args: unknown[]) => {
        ipcCalls.push(name);
        const op = name === "chan_call" ? String((args[0] as { op?: unknown } | undefined)?.op) : "";
        if (name === "chan_call") {
          chanOps.push(op);
          const a0 = args[0] as { origin: string; payload?: number[] };
          const req: unknown = a0.payload ? JSON.parse(new TextDecoder().decode(new Uint8Array(a0.payload))) : null;
          chanSent.push({ origin: a0.origin, op, req });
        }
        // 铸名那一问（`terminal-name-mint`）按帧命令名回：名字 ⇒ 那台后端的成品 `{name}`；别的 ⇒ 原样 reject（通道那一层的错）。
        // 「会打断什么」按帧命令名回：一个函数 (那台, 请求) ⇒ 那台后端的成品；没摆 ⇒ 原样 reject（问不到）。
        if (op === "machine-interrupts") {
          const answer = ipcReplies.get(op) as ((o: string, req: Record<string, unknown>) => unknown) | undefined;
          if (typeof answer !== "function") return Promise.reject(new Error("没摆"));
          const a0 = args[0] as { origin: string; payload: number[] };
          const req = JSON.parse(new TextDecoder().decode(new Uint8Array(a0.payload))) as Record<string, unknown>;
          const u = new TextEncoder().encode(JSON.stringify(answer(a0.origin, req)));
          return Promise.resolve(u.buffer.slice(u.byteOffset, u.byteOffset + u.byteLength));
        }
        if (op === "terminal-name-mint") {
          const minted = ipcReplies.get(op);
          if (typeof minted !== "string") return Promise.reject(minted);
          const u = new TextEncoder().encode(JSON.stringify({ name: minted }));
          return Promise.resolve(u.buffer.slice(u.byteOffset, u.byteOffset + u.byteLength));
        }
        // 机器表试算口：照假盘那一份现算（规则的合成版在 `config-patch-fake.ts`）。
        if (name === "machine_table_try") {
          return fakeMachineTableTry((args[0] as { edits: never[] }).edits);
        }
        const reply = ipcReplies.get(name);
        // 回一个 `Error` ⇒ 这条命令 reject（「没问到」那一形；线上是后端回 `Err`）。
        return reply instanceof Error ? Promise.reject(reply) : Promise.resolve(reply);
      },
    },
  ),
}));
// 测试连接改问本机后端（`remote-probe.ts` 经通道）⇒ 替身同一本账：记帧命令名、按名回（缺的格补成结局的空形）。
//   解码器本身由 `tests/frontend/ui/remote-probe.vitest.ts` 钉。
vi.mock("../../../../src/frontend/ui/remote-probe", () => ({
  probeMachine: () => {
    ipcCalls.push("remote-probe");
    const reply = ipcReplies.get("remote-probe");
    if (reply instanceof Error) return Promise.reject(reply);
    return Promise.resolve({ message: "", stages: [], backendGaps: [], ...(reply as object) });
  },
}));
// 「从 ~/.ssh/config 导入」那三问改问本机常驻后端（`ssh-config-reads.ts` 经通道）⇒ 替身同一本账：
//   记名（帧命令名）、按名回（`Error` ⇒ reject）。解码器本身由 `tests/frontend/ui/ssh-config-reads.vitest.ts` 对金样钉。
vi.mock("../../../../src/frontend/ui/ssh-config-reads", () => {
  // 没设的回那一问成品的空形（真模块只回解码过的值，不会回 `undefined`）。
  const ask = (op: string, empty: unknown) => () => {
    ipcCalls.push(op);
    const reply = ipcReplies.has(op) ? ipcReplies.get(op) : empty;
    return reply instanceof Error ? Promise.reject(reply) : Promise.resolve(reply);
  };
  return {
    listSshHostAliases: ask("ssh-config-aliases", []),
    resolveSshHost: ask("ssh-config-resolve", undefined),
    importSshHosts: ask("ssh-config-import", []),
  };
});
import { loadConfig } from "../../../../src/frontend/ui/config";
import { fakeCfg, fakeMachineTableTry } from "../config-patch-fake";
import { copyText } from "../../../../src/frontend/ui/copy-table";
import { dispatcher } from "../../../../src/frontend/ui/keybindings/registry";
const saveConfig = fakeCfg.saved;
import {
  shouldShowResetFingerprint,
  RemoteSection,
  LOCAL_MACHINE_PAGE_ID,
} from "../../../../src/frontend/ui/settings/remote-section";
// F12：数据层已抽到 src/frontend/ui/remote-config.ts——数据函数/类型从那里 import。
import {
  parseAddressLines,
  findHostByOrigin,
  patchRemoteConfig,
  readRemoteConfig,
  sftpEligibleHosts,
} from "../../../../src/frontend/ui/remote-config";
import type { RemoteHostConfig, RemoteConfig } from "../../../../src/frontend/ui/remote-config";
import * as remoteConfigModule from "../../../../src/frontend/ui/remote-config";
import { __setHostOsForTests } from "../../../../src/frontend/ui/settings/host-os";
// `KR59D3`：那条**有名字**的告知 —— 名字的家只有一个（`readiness.ts`），
// 判据与 DOM 上那个 `data-code` 断的是同一个串，不在这里另抄一份字面量。
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { stripComments } from "../../../test-support/strip-comments";


/** 菜单里写着 `label` 的那一项（菜单挂在 body 上）。 */
function menuItem(label: string): HTMLButtonElement {
  const it = [...document.querySelectorAll<HTMLButtonElement>('[role^="menuitem"]')].find((b) => (b.textContent ?? "").includes(label));
  if (!it) throw new Error(`菜单里没有「${label}」`);
  return it;
}

/** 开「添加机器」框（kit 对话框挂在 body 上）。 */
async function openAdd(sec: RemoteSection): Promise<HTMLElement> {
  sec.headActions()[0]!.click();
  for (let i = 0; i < 6; i++) await new Promise((r) => setTimeout(r, 0));
  return document.querySelector<HTMLElement>('[role="dialog"]')!;
}

/** 删第 i 台远端（行的 ⋯ →「从列表删除」，没有行时点卡片自己的删除）；`commit` ⇒ 点掉撤销条（到点同一条路）。 */
async function removeAt(sec: RemoteSection, i: number, commit = true): Promise<void> {
  const rows = [...sec.element.querySelectorAll<HTMLElement>(".remote-machine-row:not(.remote-machine-local)")];
  if (rows.length > 0) {
    rows[i]!.querySelector<HTMLButtonElement>("button[aria-label]")!.click();
    menuItem(copyText("machineList.menu.remove")).click();
  } else {
    sec.element.querySelectorAll<HTMLButtonElement>(".remote-machine-remove")[i]!.click();
  }
  // 删之前先问壳那台有没有端口转发（「停止转发 n」那半句），等它答回来那一行才拿掉。
  for (let k = 0; k < 4; k++) await new Promise((r) => setTimeout(r, 0));
  if (commit) {
    const close = [...document.querySelectorAll<HTMLButtonElement>("button[aria-label]")].filter(
      (b) => b.getAttribute("aria-label") === copyText("kit.toast.close"),
    );
    close.at(-1)?.click();
  }
}

describe("F43 shouldShowResetFingerprint", () => {
  it("已固化非空指纹 → 显示", () => {
    expect(shouldShowResetFingerprint("SHA256:abc")).toBe(true);
  });
  it("空 / 纯空白 → 不显示(重置无意义)", () => {
    expect(shouldShowResetFingerprint("")).toBe(false);
    expect(shouldShowResetFingerprint("   ")).toBe(false);
    expect(shouldShowResetFingerprint("\n\t")).toBe(false);
  });
});

// F08 Phase D 审计：别名生成器 UI 当年从这里迁到了 src/frontend/ui/launcher-diagnostics.ts。
// 今天它住 src/frontend/ui/settings/machine-aliases.ts（机器页「本机 → 工具 → 别名」），
// shell 文本由后端渲染；单测在 tests/frontend/ui/settings/machine-aliases.vitest.ts。

describe("F45 parseAddressLines", () => {
  it("按行 trim + 去空行", () => {
    expect(parseAddressLines("192.0.2.2\n  pi:2222 \n\n[::1]:22\n   ")).toEqual([
      "192.0.2.2",
      "pi:2222",
      "[::1]:22",
    ]);
  });
  it("空文本 → 空数组", () => {
    expect(parseAddressLines("")).toEqual([]);
    expect(parseAddressLines("   \n  ")).toEqual([]);
  });
});

describe("F54 findHostByOrigin", () => {
  const mkHost = (label: string, host: string): RemoteHostConfig => ({
    label,
    host,
    port: 22,
    user: "u",
    keyPath: "",
    hostKeyFingerprint: "",
    addresses: [],
    jump: "",
    resumeCommand: "",
    connect: true,
      });
  const hosts = [mkHost("devbox", "192.0.2.2"), mkHost("", "pi.local")];
  it("命中 label", () => {
    expect(findHostByOrigin(hosts, "devbox")?.host).toBe("192.0.2.2");
  });
  it("label 空 → 回退 host 匹配", () => {
    expect(findHostByOrigin(hosts, "pi.local")?.host).toBe("pi.local");
  });
  it("找不到 / 空列表 → null", () => {
    expect(findHostByOrigin(hosts, "nope")).toBeNull();
    expect(findHostByOrigin([], "devbox")).toBeNull();
  });
});

// E80：`describeStage` 搬去了 `machine-card.ts`（它唯一的消费者），本组从那边 import。
import { describeStage } from "../../../../src/frontend/ui/settings/machine-card";

describe("F46 describeStage", () => {
  it("各阶段 kind 有文案", () => {
    expect(describeStage({ kind: "dialing", endpoint: "h:22" }).text).toContain(copyText("machineCard.stage.dial", { endpoint: "h:22" }));
    expect(describeStage({ kind: "won", endpoint: "h:22" }), "只给一句话，图标不归它（C-W1）").toEqual({ text: copyText("machineCard.stage.won", { endpoint: "h:22" }) });
    expect(describeStage({ kind: "failed", endpoint: "h:22", reason: "x" }).text).toBe(copyText("machineCard.stage.failed", { endpoint: "h:22", reason: "x" }));
    expect(describeStage({ kind: "auth", ok: false, detail: copyText("acct.val.refusedOnly") }).text).toContain(copyText("acct.val.refusedOnly"));
    expect(describeStage({ kind: "auth", ok: true, detail: null }).text).toContain(copyText("machineCard.stage.authOk"));
    expect(describeStage({ kind: "established" }).text).toContain(copyText("machineCard.seg.ready"));
    expect(describeStage({ kind: "hostKey", endpoint: "h:22", fingerprint: "SHA256:x" }).text).toContain("SHA256:x");
  });
});

describe("F56 jump write→read 往返（D-B1 回归）", () => {
  const host = (jump: string): RemoteHostConfig => ({
    label: "devbox",
    host: "192.0.2.2",
    port: 22,
    user: "u",
    keyPath: "",
    hostKeyFingerprint: "",
    addresses: [],
    jump,
    resumeCommand: "",
    connect: true,
      });

  it("jump 写入 config 并读回不丢", async () => {
    vi.mocked(loadConfig).mockResolvedValue({});
    let saved: Record<string, unknown> = {};
    vi.mocked(saveConfig).mockImplementation(async (c: unknown) => {
      saved = c as Record<string, unknown>;
    });
    await patchRemoteConfig({ upsert: [{ key: null, value: host("bastion") }] });
    // 写入的 config 里 hosts[0] 含 jump（修前此处丢字段 → undefined，测试红）
    const written = (saved.remote as { hosts: Array<{ jump?: string }> }).hosts[0];
    expect(written.jump).toBe("bastion");
    // 读回:loadConfig 返回刚写的 → readRemoteConfig → coerceHost 保留 jump
    vi.mocked(loadConfig).mockResolvedValue(saved);
    const back = await readRemoteConfig();
    expect(back.hosts[0].jump).toBe("bastion");
  });

  it("空 jump 往返 → 空串（直连）", async () => {
    vi.mocked(loadConfig).mockResolvedValue({});
    let saved: Record<string, unknown> = {};
    vi.mocked(saveConfig).mockImplementation(async (c: unknown) => {
      saved = c as Record<string, unknown>;
    });
    await patchRemoteConfig({ upsert: [{ key: null, value: host("") }] });
    vi.mocked(loadConfig).mockResolvedValue(saved);
    const back = await readRemoteConfig();
    expect(back.hosts[0].jump).toBe("");
  });
});

describe("S4b-3 resumeCommand write→read 往返（D-B1 同源回归：新字段不丢）", () => {
  // `jump` 与那个已退役的 `daemonless` 都曾因为「加了字段但序列化清单没跟上」被静默丢掉。
  // S1 的编译期穷尽检查挡住了「漏写清单」，但挡不住「读盘那侧忘了解析」——
  // 这条往返把另一半也钉住。
  const host = (resumeCommand: string): RemoteHostConfig => ({
    label: "devbox",
    host: "192.0.2.2",
    port: 22,
    user: "u",
    keyPath: "",
    hostKeyFingerprint: "",
    addresses: [],
    jump: "",
    resumeCommand,
    connect: true,
  });

  it("填了 per-machine resume 命令，写进 config 再读回来不丢", async () => {
    vi.mocked(loadConfig).mockResolvedValue({});
    let saved: Record<string, unknown> = {};
    vi.mocked(saveConfig).mockImplementation(async (c: unknown) => {
      saved = c as Record<string, unknown>;
    });
    await patchRemoteConfig({
      upsert: [{ key: null, value: host("ccm resume --tmux") }],
    });
    vi.mocked(loadConfig).mockResolvedValue(saved);
    const back = await readRemoteConfig();
    expect(back.hosts[0]!.resumeCommand).toBe("ccm resume --tmux");
  });

  it("没填的机器读回来是空串（= 沿用全局默认），不是 undefined", async () => {
    // undefined 会让 `pickResumeCommand` 里的 `?? ""` 兜底，行为上等价；
    // 但盘上/内存里形状不一致会让后续比较（如 sameRemote 判「变没变」）出意外。
    vi.mocked(loadConfig).mockResolvedValue({
      remote: { enabled: true, hosts: [{ label: "devbox", host: "1.1.1.1" }] },
    });
    const back = await readRemoteConfig();
    expect(back.hosts[0]!.resumeCommand).toBe("");
  });
});

// 🔴 〔条 80 「不要管旧配置」〕**`KR59D3` 那一组两条整组退役了。**
//    ① 「旧 config 里的 `daemonless: true` ⇒ `legacyNoBackend` 点得出那台机器」——
//       `legacyNoBackend` 这个字段删了，断言没有对象；
//    ② 「保存一次就把盘上那个旧键写没（迁移本体）」—— 它那条
//       `expect(Object.keys(written)).not.toContain("daemonless")` 是**负向断言**：
//       那个词从全仓消失之后它**永远满足** ⇒ 留着就是一条恒绿的。
//    ⚠ 用例数 −2，逐条点名在本轮报告里。

describe("F83 sftpEligibleHosts", () => {
  const mk = (over: Partial<RemoteHostConfig>): RemoteHostConfig => ({
    label: "",
    host: "",
    port: 22,
    user: "",
    keyPath: "",
    hostKeyFingerprint: "",
    addresses: [],
    jump: "",
    resumeCommand: "",
    connect: true,
    ...over,
  });
  const cfg = (hosts: RemoteHostConfig[]): RemoteConfig => ({ hosts });

  it("空 hosts → []", () => {
    expect(sftpEligibleHosts(cfg([]))).toEqual([]);
  });
  it("缺 host 或缺 user → 排除", () => {
    const hosts = [
      mk({ host: "192.0.2.2", user: "u" }), // 全填 → 留
      mk({ host: "192.0.2.3", user: "" }), // 缺 user → 排
      mk({ host: "", user: "u" }), // 缺 host → 排
    ];
    expect(sftpEligibleHosts(cfg(hosts)).map((h) => h.host)).toEqual(["192.0.2.2"]);
  });
  it("纯空白 host/user → 排除（trim）", () => {
    const hosts = [mk({ host: "  ", user: "u" }), mk({ host: "h", user: "  " })];
    expect(sftpEligibleHosts(cfg(hosts))).toEqual([]);
  });
  it("多台全填 → 全留（保序）", () => {
    const hosts = [mk({ label: "a", host: "h1", user: "u" }), mk({ label: "b", host: "h2", user: "u" })];
    expect(sftpEligibleHosts(cfg(hosts)).map((h) => h.label)).toEqual(["a", "b"]);
  });
  it("不看 enabled（禁用远端也能纯浏览文件）", () => {
    const hosts = [mk({ host: "h", user: "u" })];
    expect(sftpEligibleHosts({ hosts })).toHaveLength(1);
  });
});

// ---------------------------------------------------------------------------
// S1：**section 层的删除基准**（`loadedKeys`）。
//
// 纯函数那几条钉的是「合并逻辑对」；这一组钉的是「section 把 patch 算对了」——
// 尤其是 remove 的基准取的是「本编辑器加载时见过的 key」，而不是「盘上全量」。
// 这条接线只有十几行，但它正是 S2 拆页后唯一挡在「静默删机器」前面的东西。
// ---------------------------------------------------------------------------
describe("S1 RemoteSection：保存走局部合并", () => {
  const mkH = (label: string, host: string): RemoteHostConfig => ({
    label,
    host,
    port: 22,
    user: "u",
    keyPath: "",
    hostKeyFingerprint: "",
    addresses: [],
    jump: "",
    resumeCommand: "",
    connect: true,
      });

  /** S4b：一个假的分页宿主，记录开了哪些页 / 跳去了哪一页。 */
  function fakePages() {
    const added: { id: string; title: string; element: HTMLElement }[] = [];
    const addedParts: (
      | { connection: HTMLElement; components: HTMLElement; terminal: HTMLElement; uninstall?: HTMLElement }
      | undefined
    )[] = [];
    const removed: string[] = [];
    const navigated: string[] = [];
    const renamed: { id: string; title: string }[] = [];
    return {
      added,
      addedParts,
      removed,
      navigated,
      renamed,
      host: {
        addMachinePage: (
          id: string,
          title: string,
          element: HTMLElement,
          parts?: { connection: HTMLElement; components: HTMLElement; terminal: HTMLElement; uninstall?: HTMLElement },
        ) => {
          added.push({ id, title, element });
          addedParts.push(parts);
        },
        removeMachinePage: (id: string) => void removed.push(id),
        navigateToMachinePage: (id: string) => void navigated.push(id),
        renameMachinePage: (id: string, title: string) => void renamed.push({ id, title }),
      },
    };
  }

  /** 起一个 section 并等它 refresh 完（构造函数里是 `void this.refresh()`）。 */
  async function mount(
    hosts: RemoteHostConfig[],
    pages?: ReturnType<typeof fakePages>["host"],
  ): Promise<RemoteSection> {
    vi.mocked(loadConfig).mockResolvedValue({
      keepMe: 1,
      remote: { enabled: true, hosts },
    } as unknown as Awaited<ReturnType<typeof loadConfig>>);
    const sec = new RemoteSection({ headless: true, pages });
    // 构造里的 refresh 是 fire-and-forget，让出事件循环等它跑完。
    await new Promise((r) => setTimeout(r, 0));
    return sec;
  }

  function writtenHosts(): RemoteHostConfig[] {
    const calls = vi.mocked(saveConfig).mock.calls;
    const last = calls[calls.length - 1]![0] as Record<string, unknown>;
    return (last.remote as RemoteConfig).hosts;
  }

  beforeEach(() => vi.resetAllMocks());
  // S9：jsdom 的 UA 是 linux。默认清掉覆盖值，用真实探测（= linux），
  // 需要别的 OS 的那条测试自己置。
  afterEach(() => __setHostOsForTests(null));

  it("删掉一张卡 ⇒ 只有那台从盘上消失，其余原样", async () => {
    const sec = await mount([mkH("a", "1.1.1.1"), mkH("b", "2.2.2.2")]);
    // 点那张卡的「删除」按钮（走的是真实 onRemove → removeCard → save 路径）。
    expect(sec.element.querySelectorAll(".remote-machine-remove")).toHaveLength(2);
    await removeAt(sec, 0);
    await new Promise((r) => setTimeout(r, 0));
    expect(saveConfig).toHaveBeenCalled();
    expect(writtenHosts().map((h) => h.label)).toEqual(["b"]);
  });

  it("删掉正有端口转发的那台 ⇒ 那一句带「停止转发 n」（数是本机后端答的）；没撤 ⇒ 到点停掉经它的那几条", async () => {
    const answer = (n: number) => (_o: string, req: Record<string, unknown>) => ({ relayedSessions: 0, relayedMaybe: 0, liveStreams: 0, forwards: req.machine ? n : 0 });
    ipcReplies.set("machine-interrupts", answer(2));
    const sec = await mount([mkH("a", "1.1.1.1"), mkH("b", "2.2.2.2")]);
    chanOps.length = 0;
    await removeAt(sec, 0, false);
    expect(document.body.textContent).toContain(copyText("machineList.remove.doneForwards", { machine: "a", n: 2 }));
    ipcReplies.set("machine-interrupts", answer(0));
    await removeAt(sec, 0, false);
    expect(document.body.textContent).toContain(copyText("machineList.remove.done", { machine: "b" }));
    ipcReplies.delete("machine-interrupts");
    // 两条撤销条都点掉（到点同一条路），不留给后面的用例。
    for (const b of [...document.querySelectorAll<HTMLButtonElement>("button[aria-label]")].filter((x) => x.getAttribute("aria-label") === copyText("kit.toast.close"))) b.click();
    for (let k = 0; k < 4; k++) await new Promise((r) => setTimeout(r, 0));
    expect(chanOps.filter((o) => o === "forward-list"), "到点没去停经它的转发").toHaveLength(1);
  });

  it("★ 删机器：撤销条到点才交本机后端清掉那台的上次值；撤销了就不清", async () => {
    const sec = await mount([mkH("a", "1.1.1.1"), mkH("b", "2.2.2.2")]);
    const forgets = () => chanSent.filter((c) => c.op === "last-seen-write").map((c) => [c.origin, c.req]);
    chanSent.length = 0;
    await removeAt(sec, 0, false);
    expect(forgets(), "撤销条还在：不清").toEqual([]);
    const undo = [...document.querySelectorAll<HTMLButtonElement>("button")].filter((b) => (b.textContent ?? "").startsWith(copyText("kit.toast.undo")));
    undo.at(-1)!.click();
    for (let k = 0; k < 4; k++) await new Promise((r) => setTimeout(r, 0));
    expect(forgets(), "撤销了：不清").toEqual([]);
    // 这个挂法没有机器页：撤回来的卡不回 DOM，眼下能点的删除只剩 b 那张。
    await removeAt(sec, 0);
    for (let k = 0; k < 4; k++) await new Promise((r) => setTimeout(r, 0));
    expect(forgets()).toEqual([["<local>", { origin: "b", forget: true }]]);
  });

  it("★ 改机器名 ⇒ 是**改**那一条，不是新增一台 + 留下孤儿", async () => {
    const sec = await mount([mkH("a", "1.1.1.1"), mkH("b", "2.2.2.2")]);
    const labelInputs = sec.element.querySelectorAll<HTMLInputElement>(
      'input[type="text"]',
    );
    // 第一张卡的第一个文本框就是 label（buildTextRow 顺序：label/host/user/…）。
    const first = labelInputs[0]!;
    first.value = "a-renamed";
    first.dispatchEvent(new Event("change"));
    await new Promise((r) => setTimeout(r, 0));
    const got = writtenHosts();
    expect(got).toHaveLength(2); // ← 分裂的话这里是 3
    expect(got.map((h) => h.label)).toEqual(["a-renamed", "b"]);
  });

  it("★ 〔FIX〕加载之后后端固化了指纹、再在页上改一格 ⇒ 盘上那份指纹不被整台盖掉", async () => {
    const sec = await mount([mkH("a", "1.1.1.1"), mkH("b", "2.2.2.2")]);
    // 加载之后，后端那一侧往盘上写了 a 的指纹（`setin` · ifEmpty）。
    const disk = (await vi.mocked(loadConfig)()) as unknown as { remote: RemoteConfig };
    disk.remote.hosts[0]!.hostKeyFingerprint = "SHA256:pinned";
    vi.mocked(loadConfig).mockResolvedValue(disk as unknown as Awaited<ReturnType<typeof loadConfig>>);
    const first = sec.element.querySelectorAll<HTMLInputElement>('input[type="text"]')[0]!;
    first.value = "a-renamed";
    first.dispatchEvent(new Event("change"));
    await new Promise((r) => setTimeout(r, 0));
    const got = writtenHosts();
    expect(got.map((h) => [h.label, h.hostKeyFingerprint])).toEqual([
      ["a-renamed", "SHA256:pinned"],
      ["b", ""],
    ]);
  });

  // ── Phase G：页 id 从「每次现算」改成「创建时定死」——两条实测复现的缺陷 ──
  //
  // 这两条在修之前**各自都有绿测试**（改名一条、删除一条），只是从没人把它们串起来
  // ——「组合未覆盖」，不是断言造假。所以这里刻意写成**两步串一起**的场景。



  it("★ 改名之后再删：列表行 / 导航项 / 盘上那条**三者一起**消失", async () => {
    const p = fakePages();
    const sec = await mount([mkH("a", "1.1.1.1"), mkH("b", "2.2.2.2")], p.host);
    // 分页模式下编辑表单在**卡片自己那一页**上，不在列表里（`card.element` 被交给
    // `addMachinePage`）—— 所以 label 输入框要从注册进去的那个 element 上取。
    const devboxPage = p.added.find((a) => a.id === "machine:a")!;
    expect(devboxPage, "devbox 那一页要注册进来了，否则下面全是空转").toBeTruthy();
    const first = devboxPage.element.querySelectorAll<HTMLInputElement>('input[type="text"]')[0]!;
    first.value = "a-renamed";
    first.dispatchEvent(new Event("change"));
    await new Promise((r) => setTimeout(r, 0));

    // 修之前：save() 把 persistedKey 改成新 origin，而页是按旧 origin 注册的
    // ⇒ removeCard 现算出 `machine:a-renamed`（不存在）⇒ 三者全留下，盘上却真删了。
    await removeAt(sec, 0);
    await new Promise((r) => setTimeout(r, 0));

    expect(writtenHosts().map((h) => h.label), "盘上真的少一台").toEqual(["b"]);
    expect(p.removed, "那一页要被注销").toContain("machine:a");
    // 按 pageId 筛，别数总行数 —— 第一行是「本机」，它本来就在，数总数会把它算进去。
    const rowIds = [...sec.element.querySelectorAll<HTMLElement>(".remote-machine-row")]
      .map((r) => r.dataset.pageId)
      .filter((i) => i !== LOCAL_MACHINE_PAGE_ID);
    expect(rowIds, "被删那台的列表行要消失，b 那行还在").toEqual(["machine:b"]);
  });

  it("★ 卡片改了名称：左栏导航那一项跟着改（不必重开设置窗）", async () => {
    const p = fakePages();
    await mount([mkH("a", "1.1.1.1")], p.host);
    const page = p.added.find((a) => a.id === "machine:a")!;
    const label = page.element.querySelectorAll<HTMLInputElement>('input[type="text"]')[0]!;
    label.value = "lx";
    label.dispatchEvent(new Event("input"));
    label.dispatchEvent(new Event("change"));
    await new Promise((r) => setTimeout(r, 0));
    expect(p.renamed.at(-1)).toEqual({ id: "machine:a", title: "lx" });
  });

  it("★ 页 id 只许在一个地方算出来（现算公式不许再出现）", () => {
    // **为什么要这条源码扫描**：上面两条行为测试盖住了「删除」与「连点添加」，
    // 但第三个现算点 `refreshMachineRow` 只在 `onStatusChanged` 里被调，
    // 而那条路要真发一次连接测试才走得到 —— 我实测过：把它改回现算，
    // 那两条行为测试**依然全绿**（变异 Q3 exit=0）。
    //
    // 与其造一个很别扭的夹具去触发它，不如直接钉住**病根的形状**：
    // 「`MACHINE_PAGE_PREFIX` 后面直接跟一个会变的表达式」这件事本身。
    // 任何一处把公式抄回去，这条就红。
    const code = stripComments(
      readFileSync(resolve(process.cwd(), "src/frontend/ui/settings/remote-section.ts"), "utf8"),
      "ts",
    );
    // 允许的两处：`LOCAL_MACHINE_PAGE_ID` 那个常量、以及 `assignPageId` 里的 `stem`。
    const occurrences = [...code.matchAll(/\$\{MACHINE_PAGE_PREFIX\}/g)].length;
    expect(
      occurrences,
      "MACHINE_PAGE_PREFIX 只该出现在 LOCAL_MACHINE_PAGE_ID 与 assignPageId 两处；" +
        "多出来的那处八成是又把页 id 现算了一遍（改名后会指向不存在的页）",
    ).toBe(2);
    // 反向自检：扫描器真读到了东西（路径写错会是空串，上面那条就恒 0≠2 假红，
    // 但这里显式钉一下更清楚）
    expect(code).toContain("assignPageId");
    // 病根形状本身：`persistedKey ?? hostKey(...)` 不许再出现在页 id 的位置上
    expect(code).not.toMatch(/\$\{MACHINE_PAGE_PREFIX\}\$\{card\./);
  });

  it("★ 两台机器名相同时删掉一台：不静默无效 —— 按键认不出是哪台 ⇒ 整批拒、说出来，盘上不动", async () => {
    // 删除基准若按**集合**算，这里会得出 remove=[]（另一张卡还占着同一个 key）⇒ 删除静默失效，必须挡住。
    // 增删也按键认元素：同一个 origin 两台 ⇒ `removein` 认出不止一台 ⇒ 整批拒（不猜是哪台），banner 说保存失败。
    const sec = await mount([mkH("dup", "1.1.1.1"), mkH("dup", "2.2.2.2")]);
    vi.mocked(saveConfig).mockClear();
    await removeAt(sec, 0);
    await new Promise((r) => setTimeout(r, 0));
    const sent = vi.mocked(fakeCfg.patches).mock.calls.at(-1)![0] as { op: string }[];
    expect(sent.map((e) => e.op)).toContain("removein");
    expect(saveConfig).not.toHaveBeenCalled();
    expect(sec.element.querySelector(".settings-banner-show")?.textContent ?? "").toContain(
      copyText("remote.save.failed"),
    );
  });

  it("config.json 里的无关顶层键不受影响", async () => {
    const sec = await mount([mkH("a", "1.1.1.1")]);
    await removeAt(sec, 0);
    await new Promise((r) => setTimeout(r, 0));
    const calls = vi.mocked(saveConfig).mock.calls;
    const last = calls[calls.length - 1]![0] as Record<string, unknown>;
    expect(last.keepMe).toBe(1);
  });

  // 读 `~/.ssh/config` 失败与「真没有别名」原先同形（空下拉 ＋「未找到」）。

  it("★ 渲染机器列表：一个后端请求都不发（1 台 3 台一样；状态灯绝不引入轮询）", async () => {
    // 红线。「打开设置时顺便把 N 台机器都探一遍」听起来不像轮询，
    // 但它是同一件事的另一种说法：一次 UI 动作扇出 N 次 ssh 往返，用户没要求过。
    ipcCalls.length = 0;
    await mount([mkH("a", "1.1.1.1")]);
    expect([...ipcCalls]).toEqual([]);
    const sec = await mount([mkH("a", "1.1.1.1"), mkH("b", "2.2.2.2"), mkH("c", "3.3.3.3")]);
    expect([...ipcCalls]).toEqual([]);
    // 正控：记账的替身真的记得到（点一次「测试连接」就有一问）——不是因为一次都没记到才「空」。
    [...sec.element.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent?.includes(copyText("machineCard.build.test")))!.click();
    for (let i = 0; i < 10; i++) await new Promise((r) => setTimeout(r, 0));
    expect(ipcCalls).toContain("remote-probe");
    ipcReplies.clear();
  });



  // 「★ S9：本机的 ccm 条目跟着 monitor 的 OS 走」那一条删了：它钉的是 monitor 跑在哪个 OS 传进 `computeGaps` 的接线，
  //   那个入参随 Windows 豁免一起删了（`readiness.ts::notApplicable` 头注第 3 条），没有被测对象。

  // 🔴 〔条 80 「不要管旧配置」〕**`KR59D3` 的产品面那条也退役了。**
  //    它断的是「喂一份带旧 `daemonless: true` 的 config ⇒ 清单上真有一条带名字的告知」，
  //    而那条告知这一拍整块删了 ⇒ 没有被测对象。⚠ 用例数 −1，逐条点名在本轮报告里。

  it("★ 渲染机器列表不按台数发后端请求", async () => {
    // §1-2：状态灯绝不引入轮询。这块是「新用户第一眼看到的东西」，
    // 更不能因为它就把 N 台机器探一遍。
    localStorage.clear();
    ipcCalls.length = 0;
    await mount([mkH("a", "1.1.1.1"), mkH("b", "2.2.2.2")], fakePages().host);
    const withTwo = [...ipcCalls];
    ipcCalls.length = 0;
    await mount([mkH("a", "1.1.1.1")], fakePages().host);
    expect(withTwo).toEqual([...ipcCalls]);
  });

  it("★ 页头右侧是「添加机器」＋ ⋯（端口转发 · 全部刷新）；全局开关与导入下拉都不在了", async () => {
    const sec = await mount([mkH("a", "1.1.1.1")]);
    const [add, more] = sec.headActions();
    expect(add!.textContent).toContain(copyText("machineList.head.add"));
    more!.click();
    expect(menuItem(copyText("machineList.menu.portForward"))).toBeTruthy();
    expect(menuItem(copyText("machineList.head.refreshAll"))).toBeTruthy();
    // 全局「启用远端模式」退场（每台自己的「连接这台」）；单个导入的下拉也退场了。
    expect(sec.element.textContent).not.toContain("启用远端模式");
  });

  it("★ 本机是第一行、没有删除按钮", async () => {
    const sec = await mount([mkH("a", "1.1.1.1")], fakePages().host);
    const rows = [...sec.element.querySelectorAll<HTMLElement>(".remote-machine-row")];
    expect(rows[0]!.classList.contains("remote-machine-local")).toBe(true);
    expect(rows[0]!.textContent).toContain(copyText("remote.cards.local"));
    // 本机删不掉（§40）：它的 ⋯ 里没有「从列表删除」；真机器那行有（反向自检）。
    const labelsOf = (row: HTMLElement): string[] => sec.menuFor(row.dataset.pageId!).map((m) => m.label);
    expect(labelsOf(rows[0]!)).not.toContain(copyText("machineList.menu.remove"));
    expect(labelsOf(rows[1]!)).toContain(copyText("machineList.menu.remove"));
  });

  it("★ 本机行不进 this.cards —— 保存写出去的机器数不变（S1 的边界）", async () => {
    // this.cards 是 S1 保存路径的输入。本机混进去 = 往用户的远端机器列表里
    // 写一台叫「本机」的假机器。
    const sec = await mount([mkH("a", "1.1.1.1"), mkH("b", "2.2.2.2")]);
    await removeAt(sec, 0);
    await new Promise((r) => setTimeout(r, 0));
    const got = writtenHosts();
    expect(got.map((h) => h.label)).toEqual(["b"]);
    expect(got.some((h) => h.label === copyText("remote.cards.local"))).toBe(false);
  });




  // ---- S4b：每台机器一页 ----

  it("★ 有分页宿主时：每台机器开一页，列表里只留一行（表单不在列表上）", async () => {
    const p = fakePages();
    const sec = await mount([mkH("a", "1.1.1.1"), mkH("b", "2.2.2.2")], p.host);
    // 本机排第一（§40：本地就是机器列表里的一行），远端跟在后面。
    expect(p.added.map((x) => x.title)).toEqual([copyText("remote.cards.local"), "a", "b"]);
    expect(p.added.slice(1).map((x) => x.id)).toEqual(["machine:a", "machine:b"]);
    // 列表里是行，不是表单：行上没有 host 输入框（那在详情页上）。
    const rows = [...sec.element.querySelectorAll<HTMLElement>(".remote-machine-row")];
    expect(rows).toHaveLength(3);
    for (const r of rows) expect(r.querySelector("input")).toBeNull();
  });

  it("★ 卡片交出「连接 / 组件」两块，且 resume 命令归组件（§5-1 要它挨着装 ccm）", async () => {
    // S4b-3a 那轮我把 resume 命令插在那个已退役的降级开关之后，commit 却说它「紧邻装/卸 ccm」——
    // 实际隔着约 120 行。这条把它钉在**组件**那一半里，不让它漂回字段区。
    const p = fakePages();
    await mount([mkH("a", "1.1.1.1")], p.host);
    // 直接拿 host 收到的 parts 断言（比翻 DOM 稳）；[0] 是本机页
    const got = p.addedParts[1];
    expect(got, "远端机器页必须带 parts").toBeTruthy();
    const inConn = got!.connection.textContent ?? "";
    const inComp = got!.components.textContent ?? "";
    expect(inConn).toContain(copyText("machineCard.field.host"));
    expect(inComp).toContain(copyText("machineCard.field.resumeCmd"));
    // 反向：恢复命令**不该**留在连接那半
    expect(inConn).not.toContain(copyText("machineCard.field.resumeCmd"));
  });

  /**
   * 🔴：**机器卡上只有三个动作**。
   *
   * 从前「组件」栏那一行挤着 8 颗按钮。今天两向钉：
   * · 「组件」栏的按钮**恰好**是 ① 部署后端（部署 · 卸载）与 ② 别名（装别名块 · 卸载别名块）那四颗；
   * · 「连接」栏的按钮**恰好**是动这条连接的那四颗（测试连接 · 推送公钥 · 文件 · 开新 Claude）
   *   ——（外加指纹那一格原有的「重置为 TOFU」）；
   * · 「ccm 助手 / ccm 启动器」这个词在两栏里一个字都不许有（用户逐字「装/卸 ccm 助手是假的」）。
   */
  it("★ MC1：组件栏只剩 ① 部署后端，② 别名在「终端」栏，连接栏是动这条连接的那几颗，「ccm 助手」一个字都不剩", async () => {
    const p = fakePages();
    await mount([mkH("a", "1.1.1.1")], p.host);
    const got = p.addedParts[1]!;
    const labels = (el: HTMLElement): string[] =>
      [...el.querySelectorAll<HTMLButtonElement>("button")]
        .filter((b) => !b.closest("details") && b.getAttribute("role") !== "switch" && b.getAttribute("aria-haspopup") !== "listbox")
        .map((b) => b.textContent ?? "");
    // 别名放「终端」栏：远端那一块与本机「终端 → 别名」同一个位置。
    // 「这台上的 cc-monitor」里归这台连接配置的：恢复命令那一行（没有按钮）＋ 底行右侧那颗「从 a 卸载…」。
    expect(labels(got.components)).toEqual([]);
    expect(got.uninstall?.textContent).toBe(copyText("machineCard.uninstall.open", { machine: "a" }));
    // ② 别名与配置文件是与本机同一个组件（`buildConfigPage`，别名那一行 `data-origin` = 这台）；接上 / 卸载在别名那一行里
    //   （卸那一颗叫「卸载 ccm」、接上之后才出现 —— 由 `machine-aliases.vitest.ts` 的远端卡那一条钉）。栏上的按钮全在那一栏里。
    expect([...got.terminal.querySelectorAll("button")].filter((b) => !b.closest(".cfg-page")).map((b) => b.textContent)).toEqual([]);
    const mgr = got.terminal.querySelector<HTMLElement>(".machine-aliases");
    expect(mgr?.dataset.origin).toBe("a");
    expect(got.terminal.textContent).toContain(copyText("machineAliases.manager.title"));
    expect(labels(got.connection).filter((t) => t !== copyText("machineCard.field.resetFingerprint")).map((t) => (t.startsWith(copyText("machineCard.conn.more")) ? "MORE" : t))).toEqual([
      copyText("machineCard.field.keyPick"),
      "MORE",
      copyText("machineCard.build.test"),
      copyText("machineCard.build.pushKey"),
    ]);
    for (const part of [got.connection, got.components, got.terminal]) {
      const txt = [part.textContent ?? "", ...[...part.querySelectorAll("[title]")].map((e) => e.getAttribute("title") ?? "")].join("\n");
      expect(txt).not.toMatch(/ccm (助手|启动器)/);
    }
  });

  it("本机页不带 parts（它没有卡片，不该被拆栏）", async () => {
    const p = fakePages();
    await mount([], p.host);
    expect(p.added[0]!.title).toBe(copyText("remote.cards.local"));
    expect(p.addedParts[0]).toBeUndefined();
  });

  it("★ 点机器名 → 跳到它那一页", async () => {
    const p = fakePages();
    const sec = await mount([mkH("a", "1.1.1.1")], p.host);
    const opens = sec.element.querySelectorAll<HTMLButtonElement>(".remote-machine-open");
    opens[1]!.click(); // [0] 是本机
    expect(p.navigated).toEqual(["machine:a"]);
    opens[0]!.click(); // 本机也点得进去
    expect(p.navigated[1]).toContain("本机");
  });

  it("★ 删掉一台 → 它那一页也被收掉（否则导航里留个指向已删机器的死项）", async () => {
    const p = fakePages();
    const sec = await mount([mkH("a", "1.1.1.1"), mkH("b", "2.2.2.2")], p.host);
    await removeAt(sec, 0);
    await new Promise((r) => setTimeout(r, 0));
    expect(p.removed).toContain("machine:a");
    // 剩下：本机 + b
    expect(
      sec.element.querySelectorAll(".remote-machine-row"),
    ).toHaveLength(2);
    // 盘上也真的少了一台（S1 的保存路径没被这次改动带偏）
    expect(writtenHosts().map((h) => h.label)).toEqual(["b"]);
  });

  it("详情页上的卡片没有折叠箭头、也没有删除按钮（删除入口在列表行）", async () => {
    const p = fakePages();
    await mount([mkH("a", "1.1.1.1")], p.host);
    const pageEl = p.added[1]!.element; // [0] 是本机页
    expect(pageEl.querySelector(".remote-machine-toggle")).toBeNull();
    expect(pageEl.querySelector(".remote-machine-remove")).toBeNull();
    // 反向自检：表单本体确实在这一页上
    expect(pageEl.querySelector("input")).not.toBeNull();
  });

  it("★ 重新加载配置时先收掉上一批机器页（否则导航里越积越多死项）", async () => {
    // `refresh()` 每次打开设置都会跑。不收旧页的话，改一次机器名就会在导航里
    // 同时留下新旧两项，而旧那项点进去是一台已经不存在的机器。
    const p = fakePages();
    const sec = await mount([mkH("a", "1.1.1.1")], p.host);
    expect(p.added.map((x) => x.title)).toEqual([copyText("remote.cards.local"), "a"]);

    vi.mocked(loadConfig).mockResolvedValue({
      remote: { enabled: true, hosts: [mkH("b", "2.2.2.2")] },
    } as unknown as Awaited<ReturnType<typeof loadConfig>>);
    await sec.refresh();

    expect(p.removed).toContain("machine:a");
    expect(p.added.map((x) => x.title)).toEqual([copyText("remote.cards.local"), "a", copyText("remote.cards.local"), "b"]);
    // 列表里也只剩新的那一台（外加恒在的本机行）
    const rows = [...sec.element.querySelectorAll<HTMLElement>(".remote-machine-row")];
    expect(rows.map((r) => r.dataset.pageId)).toEqual([
      LOCAL_MACHINE_PAGE_ID,
      "machine:b",
    ]);
  });

  it("不传分页宿主 = 老形态（卡片就地展开）—— 既有宿主不受影响", async () => {
    const sec = await mount([mkH("a", "1.1.1.1")]);
    // 本机行仍是一行（它一直都在），但远端机器不再被拆成「行 + 页」。
    expect(sec.element.querySelectorAll(".remote-machine-row")).toHaveLength(1);
    // 卡片连同表单仍在列表里，删除按钮也还在卡片上
    expect(sec.element.querySelector(".remote-machine input")).not.toBeNull();
    expect(sec.element.querySelector(".remote-machine-remove")).not.toBeNull();
  });



  /**
   * 机器 ⋯「新建会话…」开的是全产品那一个起新会话框（`new-session.ts`），机器锁定在这一台；
   * 这里一个名字都不铸、一行都不渲（终端名与那一行都在点［新建］时由那台后端出）。
   */
  it("★ 新建会话… ⇒ 开起新会话框、机器锁定在这一台，界面不自己铸名", async () => {
    document.body.innerHTML = "";
    ipcCalls.length = 0;
    chanOps.length = 0;
    const sec = await mount([mkH("a", "1.1.1.1")], fakePages().host);
    sec.menuFor("machine:a").find((m) => m.label === copyText("machineList.menu.launch"))!.onClick!();
    for (let i = 0; i < 10; i++) await new Promise((r) => setTimeout(r, 0));
    const dlg = document.querySelector<HTMLElement>('[role="dialog"]');
    expect(dlg, "起新会话框没开出来 —— 下面的断言会零命中地绿").toBeTruthy();
    expect(dlg!.textContent).toContain(copyText("newSession.title.onMachine", { machine: "a" }));
    expect(dlg!.querySelector<HTMLButtonElement>(`button[aria-label="${copyText("newSession.label.machine")}"]`)!.disabled).toBe(true);
    expect(chanOps).not.toContain("terminal-name-mint");
    expect(chanOps.some((op) => /^launch-render-/.test(op))).toBe(false);
    document.body.innerHTML = "";
  });

  // 认形状 ＋ 机器页「测试连接」那一格不露 `v=… caps=[…]` 日志行；做不到的那几类点开看。
  it("★〔FIX5 续〕测试连接那一格是人话：不含「=」「[」；这台做不到的那几类在一个点开看的格里", async () => {
    localStorage.clear();
    const noTmux = copyText("machineCard.test.gapRow", { reason: copyText("control.unavailable.noTmux", { machine: "a" }), n: "3" });
    ipcReplies.set("remote-probe", {
      sshOk: true,
      backendOk: true,
      fingerprint: null,
      endpoint: null,
      backendHello: copyText("beProbe.hello.okGaps", { build: "p5o", gaps: "3", ms: "12" }),
      backendGaps: [{ code: "no_tmux", count: 3 }],
    });
    const sec = await mount([mkH("a", "1.1.1.1")]);
    [...sec.element.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent?.includes(copyText("machineCard.build.test")))!.click();
    for (let i = 0; i < 10; i++) await new Promise((r) => setTimeout(r, 0));
    const box = sec.element.querySelector<HTMLElement>(".remote-test-result")!;
    const text = box.textContent ?? "";
    expect(text, "测试连接那一格没画出那台后端的三格 —— 下面的「不含」会零命中地绿").toContain("p5o");
    expect(text, "机器页那一格露了日志行形").not.toMatch(/[=[]/);
    const gaps = box.querySelector("details");
    expect(gaps?.querySelector("summary")?.textContent).toBe(copyText("machineCard.test.gapsSummary"));
    expect(gaps?.textContent).toContain(noTmux);
    // 正控：同一个判法认得出旧的那一形。
    expect("后端响应正常（v=1 build=b1 caps=[\"stream\"] control=ok(3ms)）").toMatch(/[=[]/);
    ipcReplies.clear();
  });


  // ── 机器表的几条写盘缺陷：回填漏一格、重名、空白卡、端口越界、卸载失败照记成功 ──
  const tick = (): Promise<void> => new Promise((r) => setTimeout(r, 0));
  const diskHosts = async (): Promise<RemoteHostConfig[]> =>
    ((await vi.mocked(loadConfig)()) as unknown as { remote: RemoteConfig }).remote.hosts;
  const field = (page: HTMLElement, placeholder: string): HTMLInputElement =>
    [...page.querySelectorAll<HTMLInputElement>("input")].find((i) => i.placeholder === placeholder)!;
  const pageOf = (p: ReturnType<typeof fakePages>, id: string): HTMLElement => p.added.find((a) => a.id === id)!.element;
  const banner = (sec: RemoteSection): string => sec.element.querySelector(".settings-banner")?.textContent ?? "";
  const change = async (input: HTMLInputElement, v: string): Promise<void> => {
    input.value = v;
    input.dispatchEvent(new Event("change"));
    for (let i = 0; i < 6; i++) await tick();
  };

  it("每台的续跑命令重开后照样回填；改别台时不被抹成空", async () => {
    const p = fakePages();
    await mount([{ ...mkH("a", "1.1.1.1"), resumeCommand: "ccm resume --tmux", connect: true }, mkH("b", "2.2.2.2")], p.host);
    const sel = pageOf(p, "machine:a").querySelector<HTMLButtonElement>("[data-role=resume-select]")!;
    expect(sel.dataset.value, "存着的那条不在候选里也列上、选中").toBe("ccm resume --tmux");
    await change(field(pageOf(p, "machine:b"), copyText("machineCard.field.userHint")), "root");
    expect((await diskHosts()).map((h) => [h.label, h.user, h.resumeCommand])).toEqual([
      ["a", "u", "ccm resume --tmux"],
      ["b", "root", ""],
    ]);
  });


  it("把一台改名成另一台的名字：就地拦住、不存；之后两台照常改得动", async () => {
    const p = fakePages();
    const sec = await mount([mkH("a", "1.1.1.1"), mkH("b", "2.2.2.2")], p.host);
    const pageB = pageOf(p, "machine:b");
    await change(pageB.querySelectorAll<HTMLInputElement>('input[type="text"]')[0]!, "a");
    expect((await diskHosts()).map((h) => h.label)).toEqual(["a", "b"]);
    expect(pageB.textContent).toContain(copyText("machineCard.field.nameTaken", { name: "a" }));
    await change(field(pageOf(p, "machine:a"), copyText("machineCard.field.userHint")), "zz");
    expect((await diskHosts()).map((h) => [h.label, h.user])).toEqual([["a", "zz"], ["b", "u"]]);
    expect(banner(sec)).toBe(copyText("remote.save.done"));
  });


  it("端口越界：就地说、不存（盘上还是原值，不是 22）", async () => {
    const p = fakePages();
    await mount([{ ...mkH("a", "1.1.1.1"), port: 2222 }], p.host);
    const pageA = pageOf(p, "machine:a");
    await change(pageA.querySelector<HTMLInputElement>('input[type="number"]')!, "70000");
    expect((await diskHosts())[0]!.port).toBe(2222);
    expect(pageA.textContent).toContain(copyText("machineCard.field.portRange"));
    // 改别的格时也不许把越界那一格兜成 22 写进去
    await change(field(pageA, copyText("machineCard.field.userHint")), "root");
    expect((await diskHosts()).map((h) => [h.user, h.port])).toEqual([["root", 2222]]);
    await change(pageA.querySelector<HTMLInputElement>('input[type="number"]')!, "2200");
    expect((await diskHosts())[0]!.port).toBe(2200);
    expect(pageA.textContent).not.toContain(copyText("machineCard.field.portRange"));
  });

  it("改名之后这一页讲的是新名字；改名时那台的默认账号 / 默认模型跟着搬", async () => {
    const p = fakePages();
    vi.mocked(loadConfig).mockResolvedValue({
      remote: { enabled: true, hosts: [mkH("a", "1.1.1.1"), mkH("b", "2.2.2.2")] },
      accounts: { byMachine: { b: { defaultName: "w", modelByAccount: { w: "opus" } } } },
    } as unknown as Awaited<ReturnType<typeof loadConfig>>);
    const sec = new RemoteSection({ headless: true, pages: p.host });
    await tick();
    expect(sec.originOfPage("machine:b")).toBe("b");
    await change(pageOf(p, "machine:b").querySelectorAll<HTMLInputElement>('input[type="text"]')[0]!, "c");
    for (let i = 0; i < 4; i++) await tick();
    expect(sec.originOfPage("machine:b"), "页 id 不变，讲的是改名之后那台").toBe("c");
    const cfg = (await vi.mocked(loadConfig)()) as unknown as { accounts: { byMachine: Record<string, unknown> } };
    expect(cfg.accounts.byMachine).toEqual({ c: { defaultName: "w", modelByAccount: { w: "opus" } } });
  });

  describe("添加机器（两栏对话框）", () => {
    const sshGroups = [
      // 「已在列表里」是后端判的（按地址 ＋ 用户 ＋ 端口）；界面照它灰，不自己比。
      { label: "a", host: "1.1.1.1", port: 22, user: "u", keyPath: null, addresses: [], jump: null, members: [{ alias: "a", host: "1.1.1.1", port: 22, proxyJump: null }], inList: true },
      { label: "gpu", host: "9.9.9.9", port: 22, user: "u", keyPath: null, addresses: ["9.9.9.8"], jump: null, members: [{ alias: "gpu", host: "9.9.9.9", port: 22, proxyJump: null }, { alias: "gpu-b", host: "9.9.9.8", port: 22, proxyJump: null }], inList: false },
    ];
    const tick = () => new Promise((r) => setTimeout(r, 0));
    const okBtn = (dlg: HTMLElement) => [...dlg.querySelectorAll<HTMLButtonElement>("button")].at(-1)!;
    beforeEach(() => {
      vi.resetAllMocks();
      document.body.replaceChildren();
    });

    it("★ 已在列表的那台灰着、勾不了；主按钮按勾上的数写「添加 1 台」；点之前一个字节都不写", async () => {
      ipcReplies.set("ssh-config-import", sshGroups);
      const sec = await mount([mkH("a", "1.1.1.1")], fakePages().host);
      vi.mocked(saveConfig).mockClear();
      const dlg = await openAdd(sec);
      const rows = [...dlg.querySelectorAll<HTMLElement>(".add-machine-row")];
      expect(rows).toHaveLength(2);
      expect(rows[0]!.dataset.inList).toBe("true");
      expect(rows[0]!.querySelector<HTMLInputElement>('input[type="checkbox"]')!.disabled).toBe(true);
      expect(okBtn(dlg).textContent).toBe(copyText("addMachine.action.ssh", { n: "1" }));
      expect(saveConfig, "框开着就写了盘").not.toHaveBeenCalled();
      ipcReplies.clear();
    });

    it("★ 撞名当场说、主按钮禁用；改掉就能加，写盘一次、框关、新那台进列表", async () => {
      ipcReplies.set("ssh-config-import", sshGroups.slice(1));
      const p = fakePages();
      const sec = await mount([mkH("gpu", "5.5.5.5")], p.host);
      vi.mocked(saveConfig).mockClear();
      const dlg = await openAdd(sec);
      expect(okBtn(dlg).getAttribute("aria-disabled"), "撞名还能点").toBe("true");
      expect(dlg.textContent).toContain(copyText("addMachine.err.taken", { name: "gpu" }));
      const name = dlg.querySelector<HTMLInputElement>(".add-machine-row input:not([type=checkbox])")!;
      name.value = "gpu-2";
      name.dispatchEvent(new Event("input"));
      for (let i = 0; i < 5; i++) await tick();
      expect(okBtn(dlg).hasAttribute("aria-disabled")).toBe(false);
      okBtn(dlg).click();
      for (let i = 0; i < 5; i++) await tick();
      expect(document.querySelector('[role="dialog"]'), "加成了框还开着").toBeNull();
      expect(writtenHosts().map((h) => h.label)).toEqual(["gpu", "gpu-2"]);
      expect(p.added.map((a) => a.id)).toContain("machine:gpu-2");
      ipcReplies.clear();
    });

    it("★ 写失败：框不关、填的都在、框顶说没存上；卡收回去（列表里不多一台）", async () => {
      ipcReplies.set("ssh-config-import", []);
      const p = fakePages();
      const sec = await mount([mkH("a", "1.1.1.1")], p.host);
      const dlg = await openAdd(sec);
      // 没有 ssh config ⇒ 自己填那一栏
      const inputs = [...dlg.querySelectorAll<HTMLInputElement>(".add-machine-grid input")];
      const [name, host, user] = inputs;
      name!.value = "box";
      host!.value = "7.7.7.7";
      user!.value = "root";
      for (const i of [name, host, user]) i!.dispatchEvent(new Event("input"));
      for (let i = 0; i < 5; i++) await tick(); // 等机器表那一道答回来
      vi.mocked(fakeCfg.patches).mockImplementationOnce(() => {
        throw new Error("disk full");
      });
      okBtn(dlg).click();
      for (let i = 0; i < 5; i++) await tick();
      expect(document.querySelector('[role="dialog"]'), "没存上框却关了").not.toBeNull();
      expect(host!.value).toBe("7.7.7.7");
      expect(dlg.textContent).toContain(copyText("addMachine.err.save"));
      expect(sec.element.querySelectorAll(".remote-machine-row:not(.remote-machine-local)")).toHaveLength(1);
      ipcReplies.clear();
    });
  });

  describe("连接这台（每台一个开关，当场生效）", () => {
    const tick = () => new Promise((r) => setTimeout(r, 0));
    it("★ 关掉 ⇒ 盘上那台记 connect:false、当场对齐那几条流；那一行空心点 ＋「已停用」＋［连接］；点［连接］⇒ 记回 true、再对齐", async () => {
      ipcCalls.length = 0;
      const p = fakePages();
      const sec = await mount([mkH("a", "1.1.1.1"), mkH("b", "2.2.2.2")], p.host);
      const page = pageOf(p, "machine:a");
      const sw = page.querySelector<HTMLButtonElement>('[role="switch"]')!;
      expect(sw.getAttribute("aria-checked")).toBe("true");
      sw.click();
      for (let i = 0; i < 6; i++) await tick();
      expect(writtenHosts().map((h) => [h.label, h.connect])).toEqual([["a", false], ["b", true]]);
      expect(ipcCalls.filter((c) => c === "remote_reconcile"), "改了没当场对齐").toHaveLength(1);
      const row = sec.element.querySelector<HTMLElement>('.remote-machine-row[data-page-id="machine:a"]')!;
      expect(row.querySelector(".remote-machine-word")!.textContent).toBe(copyText("machinePage.state.disabled"));
      expect(row.querySelector<HTMLElement>("[data-state]")!.dataset.state).toBe("exited");
      [...row.querySelectorAll<HTMLButtonElement>(".machine-problem button")].find((b) => b.textContent === copyText("machinePage.problem.connect"))!.click();
      for (let i = 0; i < 6; i++) await tick();
      expect(writtenHosts().map((h) => [h.label, h.connect])).toEqual([["a", true], ["b", true]]);
      expect(ipcCalls.filter((c) => c === "remote_reconcile")).toHaveLength(2);
      expect(row.querySelector(".remote-machine-word")!.textContent).toBe("");
    });
  });

  // Esc 一次只关最上面一层：机器页上的三个小框（批量导入预览 · 新建会话 · 端口转发）关掉自己，设置窗不跟着关。
  describe("Esc 只关最上面那个小框", () => {
    const esc = (): void => {
      (document.activeElement ?? document.body).dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", code: "Escape", bubbles: true }));
    };
    let panelEsc = vi.fn<() => void>();
    const panel = { handleEsc: () => void panelEsc() };
    beforeEach(() => {
      dispatcher.applyOverrides({});
      dispatcher.start();
      panelEsc = vi.fn<() => void>();
      dispatcher.pushOverlay(panel); // 设置窗自己是栈底那一层
    });
    afterEach(() => dispatcher.popOverlay(panel));

    it("添加机器", async () => {
      ipcReplies.set("ssh-config-import", [
        { label: "x", host: "h", port: 22, user: "u", keyPath: null, addresses: [], jump: null, members: [{ alias: "x", host: "h", port: 22, proxyJump: null }] },
      ]);
      const sec = await mount([], fakePages().host);
      document.body.appendChild(sec.element);
      sec.headActions()[0]!.click();
      await tick();
      await tick();
      expect(document.querySelector('[role="dialog"]'), "前提：添加机器框开着").toBeTruthy();
      esc();
      expect(document.querySelector('[role="dialog"]')).toBeNull();
      expect(panelEsc).not.toHaveBeenCalled();
      esc(); // 再按一次才轮到设置窗
      expect(panelEsc).toHaveBeenCalledTimes(1);
      ipcReplies.clear();
      sec.element.remove();
    });

    it("新建会话", async () => {
      const p = fakePages();
      const sec = await mount([mkH("a", "1.1.1.1")], p.host);
      sec.menuFor("machine:a").find((m) => m.label === copyText("machineList.menu.launch"))!.onClick!();
      await tick();
      expect(document.querySelector('[role="dialog"]'), "前提：起新会话框开着").toBeTruthy();
      esc();
      expect(document.querySelector('[role="dialog"]')).toBeNull();
      expect(panelEsc).not.toHaveBeenCalled();
    });

    it("端口转发", async () => {
      const sec = await mount([], fakePages().host);
      sec.headActions()[1]!.click();
      menuItem(copyText("machineList.menu.portForward")).click();
      await tick();
      const pf = document.querySelector<HTMLElement>(".pf-overlay")!;
      expect(pf.style.display, "前提：面板开着").not.toBe("none");
      esc();
      expect(pf.style.display).toBe("none");
      expect(panelEsc).not.toHaveBeenCalled();
    });
  });

  // 结果区的成败由样式类表达（图标由代码画），字只是那句原话 —— 不在字前面拼写死的 ✓ / ✗。
  it("组件动作的结果区：成败落在样式类上，字里不拼 ✓ / ✗", async () => {
    for (const [reply, cls, text] of [
      [new Error("ssh: connect refused"), "remote-test-err", "Error: ssh: connect refused"],
      ["done-ok", "remote-test-ok", "done-ok"],
    ] as const) {
      localStorage.clear();
      ipcReplies.set("uninstall_remote_backend", reply);
      const p = fakePages();
      await mount([mkH("a", "1.1.1.1")], p.host);
      (p.addedParts[1]!.uninstall as HTMLButtonElement).click();
      await tick();
      [...document.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === copyText("machineCard.uninstall.action"))!.click();
      for (let i = 0; i < 4; i++) await tick();
      const roots = [...p.added.map((x) => x.element), ...Object.values(p.addedParts[1] ?? {})];
      const lines = roots.flatMap((r) => [...r.querySelectorAll<HTMLElement>(`.remote-test-result .${cls}`)]);
      expect(lines.map((l) => l.textContent)).toContain(text);
      for (const r of roots) expect(r.textContent ?? "").not.toMatch(/[✓✗] /);
      ipcReplies.clear();
      document.body.replaceChildren();
    }
  });
});


// 要求：「不为旧配置留兼容」
// 「认不出就不显示那台、在机器页顶上一句『远端配置认不出：…』」。
describe("remote 段认不出 ⇒ 机器列表顶上说一句、一台都不显示", () => {
  beforeEach(() => vi.resetAllMocks());

  it("★ 旧的单台写法：那一句常驻显示，机器卡片零张；认得出的那份不显示它", async () => {
    type Cfg = Awaited<ReturnType<typeof loadConfig>>;
    vi.mocked(loadConfig).mockResolvedValue({
      remote: { enabled: true, host: "pi.local", user: "pi", backendPath: "/x" },
    } as unknown as Cfg);
    let sec = new RemoteSection({ headless: true });
    await new Promise((r) => setTimeout(r, 0));
    let note = sec.element.querySelector<HTMLElement>(".remote-config-unrecognized");
    expect(note, "找不到那一句的元素 —— 下面全是空真").toBeTruthy();
    expect(note!.textContent).toBe(remoteConfigModule.REMOTE_CONFIG_UNRECOGNIZED);
    expect(note!.classList.contains("settings-banner-show"), "那一句没显示出来").toBe(true);
    expect(
      sec.element.querySelectorAll(".remote-machine:not(.remote-machine-local)").length,
      "认不出还显示了机器 —— 那是在猜",
    ).toBe(0);

    vi.mocked(loadConfig).mockResolvedValue({ remote: { enabled: true, hosts: [] } } as unknown as Cfg);
    sec = new RemoteSection({ headless: true });
    await new Promise((r) => setTimeout(r, 0));
    note = sec.element.querySelector<HTMLElement>(".remote-config-unrecognized");
    expect(note!.classList.contains("settings-banner-show")).toBe(false);
    expect(note!.textContent).toBe("");
  });
});


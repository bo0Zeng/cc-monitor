// F43：指纹重置按钮显隐纯逻辑。remote-section 的 DOM 主体重(拉整卡),这里只钉住
// 「有固化指纹才显示重置按钮」这条判定,防未来误改成空指纹也显示(重置无意义)。
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
// F56：写入/读取都走 config.ts；mock 掉以测 jump write→read 往返。
// S1：写入口从 writeRemoteConfig（整表覆盖，已取消导出）改为 patchRemoteConfig（局部合并）。 〔散文墓碑〕
// 〔CFG1〕config 写只交补丁；替身把补丁应用到 `loadConfig` 摆的那份上，写完的整份交 `fakeCfg.saved`。
vi.mock("../../src/config", async (orig) => (await import("../config-patch-fake")).mockedConfigModule(orig));
// S3：把整个 IPC 面 mock 成一个**会记账的 Proxy** —— 用来钉「渲染机器列表时零次
// 后端调用」。这比源码扫描强：扫描只能证明「没 import」，证明不了「渲染时没调」。
const { ipcCalls, chanOps, ipcReplies } = vi.hoisted(() => ({
  ipcCalls: [] as string[],
  /** 〔MIG-2〕经 `commands.chan_call` 发出去的那几问的 op（按发出顺序）。 */
  chanOps: [] as string[],
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
vi.mock("../../src/ipc/commands", () => ({
  commands: new Proxy(
    {},
    {
      get: (_t, name: string) => (...args: unknown[]) => {
        ipcCalls.push(name);
        if (name === "chan_call") chanOps.push(String((args[0] as { op?: unknown } | undefined)?.op));
        const reply = ipcReplies.get(name);
        // 〔FE1〕回一个 `Error` ⇒ 这条命令 reject（「没问到」那一形；线上是后端回 `Err`）。
        return reply instanceof Error ? Promise.reject(reply) : Promise.resolve(reply);
      },
    },
  ),
}));
// 〔MIG-1 续 · `99 §2.1 ⑬`〕列 tmux 会话改问那台后端（`tmux-reads.ts` 经通道）⇒ 替身同一本账：记旧名、按旧名回（`Error` ⇒ reject）。
//   解码器本身由 `tests/tmux-reads.vitest.ts` 对金样钉。
vi.mock("../../src/tmux-reads", () => ({
  listTmux: (origin: string) => {
    const name = origin === "<local>" ? "list_local_tmux" : "list_remote_tmux";
    ipcCalls.push(name);
    const reply = ipcReplies.get(name);
    return reply instanceof Error ? Promise.reject(reply) : Promise.resolve(reply ?? []);
  },
}));
// 〔MIG-1 · `99 §2.1 ⑯`〕「从 ~/.ssh/config 导入」那三问改问本机常驻后端（`ssh-config-reads.ts` 经通道）⇒ 替身同一本账：
//   记名（帧命令名）、按名回（`Error` ⇒ reject）。解码器本身由 `tests/ssh-config-reads.vitest.ts` 对金样钉。
vi.mock("../../src/ssh-config-reads", () => {
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
import { loadConfig } from "../../src/config";
import { fakeCfg } from "../config-patch-fake";
import { copyText } from "../../src/copy-table";
const saveConfig = fakeCfg.saved;
import {
  shouldShowResetFingerprint,
  RemoteSection,
  LOCAL_MACHINE_PAGE_ID,
} from "../../src/settings/remote-section";
// F12：数据层已抽到 src/remote-config.ts——数据函数/类型从那里 import。
import {
  parseAddressLines,
  findHostByOrigin,
  patchRemoteConfig,
  readRemoteConfig,
  sftpEligibleHosts,
} from "../../src/remote-config";
import type { RemoteHostConfig, RemoteConfig } from "../../src/remote-config";
import * as remoteConfigModule from "../../src/remote-config";
// `N-F2`：`forgetMachine` 是本文件末尾那条「先证会红」用的 —— 只抹本机那一栏，
// 而不是 `localStorage.clear()`，这样「回到旧行为」这句话是按机器说的，不是按整本账说的。
import {
  recordFacet,
  readStatus,
  forgetMachine,
  LOCAL_MACHINE_KEY,
} from "../../src/settings/machine-status";
import { __setHostOsForTests } from "../../src/settings/host-os";
// `KR59D3`：那条**有名字**的告知 —— 名字的家只有一个（`readiness.ts`），
// 判据与 DOM 上那个 `data-code` 断的是同一个串，不在这里另抄一份字面量。
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { stripComments } from "../test-support/strip-comments";

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

// F08 Phase D 审计：别名生成器 UI 当年从这里迁到了 src/launcher-diagnostics.ts。
// 〔AL1 · 2026-09-24〕今天它住 src/settings/machine-aliases.ts（机器页「本机 → 工具 → 别名」），
// shell 文本由后端渲染；单测在 tests/settings/machine-aliases.vitest.ts。

describe("F45 parseAddressLines", () => {
  it("按行 trim + 去空行", () => {
    expect(parseAddressLines("10.0.0.2\n  pi:2222 \n\n[::1]:22\n   ")).toEqual([
      "10.0.0.2",
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
      });
  const hosts = [mkHost("devbox", "10.0.0.2"), mkHost("", "pi.local")];
  it("命中 label", () => {
    expect(findHostByOrigin(hosts, "devbox")?.host).toBe("10.0.0.2");
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
import { describeStage } from "../../src/settings/machine-card";

describe("F46 describeStage", () => {
  it("各阶段 kind 有图标+文案", () => {
    expect(describeStage({ kind: "dialing", endpoint: "h:22" }).text).toContain("拨号 h:22");
    expect(describeStage({ kind: "won", endpoint: "h:22" }).icon).toBe("✓");
    expect(describeStage({ kind: "failed", endpoint: "h:22", reason: "x" }).text).toContain("失败");
    expect(describeStage({ kind: "auth", ok: false, detail: "被拒" }).text).toContain("被拒");
    expect(describeStage({ kind: "auth", ok: true, detail: null }).text).toContain("鉴权通过");
    expect(describeStage({ kind: "established" }).text).toContain("就绪");
    expect(describeStage({ kind: "hostKey", endpoint: "h:22", fingerprint: "SHA256:x" }).text).toContain("SHA256:x");
  });
});

describe("F56 jump write→read 往返（D-B1 回归）", () => {
  const host = (jump: string): RemoteHostConfig => ({
    label: "devbox",
    host: "10.0.0.2",
    port: 22,
    user: "u",
    keyPath: "",
    hostKeyFingerprint: "",
    addresses: [],
    jump,
    resumeCommand: "",
      });

  it("jump 写入 config 并读回不丢", async () => {
    vi.mocked(loadConfig).mockResolvedValue({});
    let saved: Record<string, unknown> = {};
    vi.mocked(saveConfig).mockImplementation(async (c: unknown) => {
      saved = c as Record<string, unknown>;
    });
    await patchRemoteConfig({ enabled: true, upsert: [{ key: null, value: host("bastion") }] });
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
    await patchRemoteConfig({ enabled: true, upsert: [{ key: null, value: host("") }] });
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
    host: "10.0.0.2",
    port: 22,
    user: "u",
    keyPath: "",
    hostKeyFingerprint: "",
    addresses: [],
    jump: "",
    resumeCommand,
  });

  it("填了 per-machine resume 命令，写进 config 再读回来不丢", async () => {
    vi.mocked(loadConfig).mockResolvedValue({});
    let saved: Record<string, unknown> = {};
    vi.mocked(saveConfig).mockImplementation(async (c: unknown) => {
      saved = c as Record<string, unknown>;
    });
    await patchRemoteConfig({
      enabled: true,
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

// 🔴 〔步 8 · 条 80 「不要管旧配置」〕**`KR59D3` 那一组两条整组退役了。**
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
    ...over,
  });
  const cfg = (hosts: RemoteHostConfig[]): RemoteConfig => ({
    enabled: false,
    hosts,
  });

  it("空 hosts → []", () => {
    expect(sftpEligibleHosts(cfg([]))).toEqual([]);
  });
  it("缺 host 或缺 user → 排除", () => {
    const hosts = [
      mk({ host: "10.0.0.2", user: "u" }), // 全填 → 留
      mk({ host: "10.0.0.3", user: "" }), // 缺 user → 排
      mk({ host: "", user: "u" }), // 缺 host → 排
    ];
    expect(sftpEligibleHosts(cfg(hosts)).map((h) => h.host)).toEqual(["10.0.0.2"]);
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
    expect(sftpEligibleHosts({ enabled: false, hosts })).toHaveLength(1);
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
      });

  /** S4b：一个假的分页宿主，记录开了哪些页 / 跳去了哪一页。 */
  function fakePages() {
    const added: { id: string; title: string; element: HTMLElement }[] = [];
    const addedParts: (
      | { connection: HTMLElement; components: HTMLElement; tools: HTMLElement }
      | undefined
    )[] = [];
    const removed: string[] = [];
    const navigated: string[] = [];
    return {
      added,
      addedParts,
      removed,
      navigated,
      host: {
        addMachinePage: (
          id: string,
          title: string,
          element: HTMLElement,
          parts?: { connection: HTMLElement; components: HTMLElement; tools: HTMLElement },
        ) => {
          added.push({ id, title, element });
          addedParts.push(parts);
        },
        removeMachinePage: (id: string) => void removed.push(id),
        navigateToMachinePage: (id: string) => void navigated.push(id),
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
    const removeBtns = sec.element.querySelectorAll<HTMLButtonElement>(
      ".remote-machine-remove",
    );
    expect(removeBtns).toHaveLength(2);
    removeBtns[0]!.click();
    await new Promise((r) => setTimeout(r, 0));
    expect(saveConfig).toHaveBeenCalled();
    expect(writtenHosts().map((h) => h.label)).toEqual(["b"]);
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

  it("★ 〔FIX · `99 §2 ㊶`〕加载之后后端固化了指纹、再在页上改一格 ⇒ 盘上那份指纹不被整台盖掉", async () => {
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

  it("★ 连点两次「+ 添加机器」不许抛（空白卡的 origin 是空串，页 id 会撞）", async () => {
    const p = fakePages();
    const sec = await mount([], p.host);
    const add = [...sec.element.querySelectorAll<HTMLButtonElement>("button")].find(
      (b) => b.textContent === "+ 添加机器",
    )!;
    expect(add, "找不到「+ 添加机器」按钮，下面的断言就是空转").toBeTruthy();
    add.click();
    // 修之前：第二次点击命中 router 的重复注册 throw（两次算出的 id 都是 `machine:`），
    // 而 `this.cards.push` 已经执行 ⇒ 之后任何一次 save 都会把这张
    // **界面上看不见的幽灵卡**写进 config.json。
    expect(() => add.click()).not.toThrow();
    const ids = p.added.map((a) => a.id).filter((i) => i !== LOCAL_MACHINE_PAGE_ID);
    expect(ids).toHaveLength(2);
    expect(new Set(ids).size, "两张空白卡必须拿到不同的页 id").toBe(2);
  });

  it("★ 改名之后再删：列表行 / 导航项 / 盘上那条**三者一起**消失", async () => {
    const p = fakePages();
    const sec = await mount([mkH("a", "1.1.1.1"), mkH("b", "2.2.2.2")], p.host);
    // 分页模式下编辑表单在**卡片自己那一页**上，不在列表里（`card.element` 被交给
    // `addMachinePage`）—— 所以 label 输入框要从注册进去的那个 element 上取。
    const ayaPage = p.added.find((a) => a.id === "machine:a")!;
    expect(ayaPage, "devbox 那一页要注册进来了，否则下面全是空转").toBeTruthy();
    const first = ayaPage.element.querySelectorAll<HTMLInputElement>('input[type="text"]')[0]!;
    first.value = "a-renamed";
    first.dispatchEvent(new Event("change"));
    await new Promise((r) => setTimeout(r, 0));

    // 修之前：save() 把 persistedKey 改成新 origin，而页是按旧 origin 注册的
    // ⇒ removeCard 现算出 `machine:a-renamed`（不存在）⇒ 三者全留下，盘上却真删了。
    sec.element.querySelectorAll<HTMLButtonElement>(".remote-machine-remove")[0]!.click();
    await new Promise((r) => setTimeout(r, 0));

    expect(writtenHosts().map((h) => h.label), "盘上真的少一台").toEqual(["b"]);
    expect(p.removed, "那一页要被注销").toContain("machine:a");
    // 按 pageId 筛，别数总行数 —— 第一行是「本机」，它本来就在，数总数会把它算进去。
    const rowIds = [...sec.element.querySelectorAll<HTMLElement>(".remote-machine-row")]
      .map((r) => r.dataset.pageId)
      .filter((i) => i !== LOCAL_MACHINE_PAGE_ID);
    expect(rowIds, "被删那台的列表行要消失，b 那行还在").toEqual(["machine:b"]);
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
      readFileSync(resolve(process.cwd(), "src/settings/remote-section.ts"), "utf8"),
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
    // 〔FIX2 · ㊶〕增删也按键认元素：同一个 origin 两台 ⇒ `removein` 认出不止一台 ⇒ 整批拒（不猜是哪台），banner 说保存失败。
    const sec = await mount([mkH("dup", "1.1.1.1"), mkH("dup", "2.2.2.2")]);
    vi.mocked(saveConfig).mockClear();
    sec.element
      .querySelectorAll<HTMLButtonElement>(".remote-machine-remove")[0]!
      .click();
    await new Promise((r) => setTimeout(r, 0));
    const sent = vi.mocked(fakeCfg.patches).mock.calls.at(-1)![0] as { op: string }[];
    expect(sent.map((e) => e.op)).toContain("removein");
    expect(saveConfig).not.toHaveBeenCalled();
    expect(sec.element.querySelector(".settings-banner-show")?.textContent ?? "").toContain(
      copyText("remote.save.failed", { e: "" }).trim(),
    );
  });

  it("config.json 里的无关顶层键不受影响", async () => {
    const sec = await mount([mkH("a", "1.1.1.1")]);
    sec.element
      .querySelector<HTMLButtonElement>(".remote-machine-remove")!
      .click();
    await new Promise((r) => setTimeout(r, 0));
    const calls = vi.mocked(saveConfig).mock.calls;
    const last = calls[calls.length - 1]![0] as Record<string, unknown>;
    expect(last.keepMe).toBe(1);
  });

  // 〔W5-UI · 设计/70 §7 #4〕读 `~/.ssh/config` 失败与「真没有别名」原先同形（空下拉 ＋「未找到」）。
  it("导入下拉：读别名清单失败 ⇒ 说读不了（原因原样），不说「未找到」；真没有 ⇒ 说未找到（正控）", async () => {
    // 从 section 自己的 DOM 里取（不是私有字段）：那块提示原先根本没挂进 DOM —— 取字段会假绿。
    const hint = (sec: RemoteSection): string =>
      [...sec.element.querySelectorAll<HTMLElement>(".settings-hint")].map((e) => e.textContent ?? "").join("|");
    ipcReplies.set("ssh-config-aliases", new Error("perm-denied-sshcfg"));
    try {
      const bad = await mount([mkH("a", "1.1.1.1")]);
      await new Promise((r) => setTimeout(r, 0));
      expect(hint(bad)).toContain("读不了 ~/.ssh/config");
      expect(hint(bad)).toContain("perm-denied-sshcfg");
      expect(hint(bad)).not.toContain("未在 ~/.ssh/config 找到");
      ipcReplies.set("ssh-config-aliases", []);
      const none = await mount([mkH("a", "1.1.1.1")]);
      await new Promise((r) => setTimeout(r, 0));
      expect(hint(none)).toContain("未在 ~/.ssh/config 找到");
      expect(hint(none)).not.toContain("读不了");
    } finally {
      ipcReplies.delete("ssh-config-aliases");
    }
  });

  it("★ 渲染机器列表：后端调用**不随机器数增长**（状态灯绝不引入轮询）", async () => {
    // 主计划 §1-2 的红线。「打开设置时顺便把 N 台机器都探一遍」听起来不像轮询，
    // 但它是同一件事的另一种说法：一次 UI 动作扇出 N 次 ssh 往返，用户没要求过。
    //
    // 判据**不是**「零调用」—— 实测渲染时确实有一次 `ssh-config-aliases`（〔MIG-1〕问本机后端）
    //（读本机 `~/.ssh/config` 填「导入」下拉），那既不是状态探测、也不走 ssh、
    // 更不随机器数增长。红线禁的是**逐机器探测**，所以判据就写成那样：
    // **同一份调用清单，1 台和 3 台必须逐字相同。**
    ipcCalls.length = 0;
    await mount([mkH("a", "1.1.1.1")]);
    const withOne = [...ipcCalls];

    ipcCalls.length = 0;
    await mount([mkH("a", "1.1.1.1"), mkH("b", "2.2.2.2"), mkH("c", "3.3.3.3")]);
    const withThree = [...ipcCalls];

    expect(withThree).toEqual(withOne);
    // 反向自检：不是因为一次都没记到才「相同」。
    expect(withOne.length).toBeGreaterThan(0);
    // 且清单里不许出现任何逐机器探测类命令。
    for (const name of withThree) {
      expect(name).not.toMatch(/test_remote_connection|probe_|deploy_|remote_ccm/);
    }
  });

  it("★ E56「还差什么」：全新用户（账本空）只说「没测过」，一条「缺」都不说", async () => {
    // 一个刚装好、什么都没点过的人不该看到一屏红叉。
    localStorage.clear();
    const sec = await mount([mkH("a", "1.1.1.1")], fakePages().host);
    const box = sec.element.querySelector<HTMLElement>(".remote-gaps")!;
    expect(box.style.display).not.toBe("none");
    expect(box.textContent).toContain("还没测过");
    expect(box.textContent).not.toContain("确认缺");
    const items = [...box.querySelectorAll<HTMLElement>(".remote-gap")];
    expect(items.length).toBeGreaterThan(0);
    expect(items.every((i) => i.dataset.kind === "unknown")).toBe(true);
  });

  it("★ 测过且失败的那项才说「缺」，成功的项不出现", async () => {
    localStorage.clear();
    recordFacet("a", "connection", { kind: "ok", at: Date.now() });
    recordFacet("a", "backend", { kind: "fail", at: Date.now() });
    const sec = await mount([mkH("a", "1.1.1.1")], fakePages().host);
    const box = sec.element.querySelector<HTMLElement>(".remote-gaps")!;
    expect(box.textContent).toContain("确认缺");
    // **按机器筛**：清单里同时有本机的条目（本机也没测过），不区分 origin 会误判。
    const ofA = [...box.querySelectorAll<HTMLElement>('.remote-gap[data-origin="a"]')].map(
      (i) => `${i.dataset.facet}:${i.dataset.kind}`,
    );
    expect(ofA).toContain("backend:missing");
    // devbox 的 connection 测过且 ok ⇒ 它那台不该再出现这一项
    expect(ofA.some((f) => f.startsWith("connection:"))).toBe(false);
  });

  it("★ S9：本机的 ccm 条目跟着 monitor 的 OS 走（钉的是接线，不是纯函数）", async () => {
    // `readiness.vitest.ts` 已经证明 `computeGaps` 会按 `hostOs` 排掉这一项；
    // 这一条守的是**调用点真把 `hostOs()` 传进去了** —— 漏传时纯函数测试全绿。
    const localCcm = (sec: { element: HTMLElement }) =>
      [
        ...sec.element.querySelectorAll<HTMLElement>(
          `.remote-gap[data-origin="${LOCAL_MACHINE_KEY}"]`,
        ),
      ].some((i) => i.dataset.facet === "ccm");

    localStorage.clear();
    __setHostOsForTests("windows");
    expect(
      localCcm(await mount([mkH("a", "1.1.1.1")], fakePages().host)),
      "Windows 本机的启动器是「终端集成」，不该说它缺 ccm",
    ).toBe(false);

    localStorage.clear();
    __setHostOsForTests("linux");
    expect(
      localCcm(await mount([mkH("a", "1.1.1.1")], fakePages().host)),
      "Linux 本机的 ccm 是真能装的，照常算数",
    ).toBe(true);
  });

  // ───────────────────────────────────────────────────────────────────────────
  // `N-F2` `NF2D3` **最后那一跳**：`summary === null` ⇒ 这一块**整块不出现**
  //（`remote-section.ts` 里 `renderGaps` 的 `if (!summary)` 那一支）。
  //
  // # 它此前是死代码，而这里是唯一断得到它的地方
  //
  // 本机的 `acctIso` / `accounts` 在 `N-F2` 之前**全仓没有任何 `recordFacet` 生产者**
  //（唯一的写点 `accounts-section.note()` 第一行是 `if (!this.origin) return`，
  // 而本机这条路上 `origin` 恒空）⇒ 那两格恒 `unknown` ⇒ `summarizeGaps` 恒非 null
  // ⇒ 这一支**在结构上走不到**。`accounts-section.vitest.ts` 里 `N-F2` 那一族
  // 只断到 `summarizeGaps === null` 为止 —— **DOM 这一跳它够不着**。
  //
  // ⚠ 上面那条 `★ E56「还差什么」：全新用户…` 只断过它的**反面**
  //（`expect(box.style.display).not.toBe("none")`）⇒ `display:none` 那一支
  // 在本件之前是**零覆盖**。这两条就是去覆盖它。
  //
  // ⚠ 这一块的 `display` 出厂值就是 `"none"`（构造时设的）⇒ 只断「等于 none」
  // 会被「`renderGaps` 压根没跑」喂饱。所以第一条**先断它真的出现过**、
  // 且逐项等于本机那两格，那一屏就是分母本身。
  // ───────────────────────────────────────────────────────────────────────────

  /**
   * 本机那几格写绿 —— 值取各自生产写点真会写的那些。
   *
   * ⚠ `K-R59`（09-11）**从两格变成三格**：`backend` 那条「本机不适用」的豁免撤了
   *（`C7` 之后本机也有后端进程），写点是本文件被测对象自己的 `noteLocalBackend`。
   */
  function greenLocalTwo(): void {
    recordFacet(LOCAL_MACHINE_KEY, "backend", { kind: "ok", detail: "已连上" });
    recordFacet(LOCAL_MACHINE_KEY, "acctIso", { kind: "ok", detail: "已启用" });
    recordFacet(LOCAL_MACHINE_KEY, "accounts", { kind: "ok", detail: "3 个" });
  }
  const gapsBoxOf = (sec: RemoteSection): HTMLElement =>
    sec.element.querySelector<HTMLElement>(".remote-gaps")!;
  /** 这一块里逐条的 `机器/格:类别` —— 按 `data-*` 认，不按文案认。 */
  const gapKeysOf = (box: HTMLElement): string[] =>
    [...box.querySelectorAll<HTMLElement>(".remote-gap")].map(
      (i) => `${i.dataset.origin}/${i.dataset.facet}:${i.dataset.kind}`,
    );

  it("★ NF2D3 最后那一跳：本机全绿 + 零远端 ⇒ 「还差什么」整块不出现", async () => {
    // 分母先钉死，别让「一台机器都没有」蒙混过去：
    //   · 远端 **0** 台，而清单的入参是 `[LOCAL_MACHINE_KEY, ...hosts]` ⇒ 机器数 **1**；
    //   · monitor 跑在 Windows 上 ⇒ 本机的适用格**恰好**是 `backend` / `acctIso` / `accounts`
    //     三格（`connection` 不适用；`ccm` 的对应物是「终端集成」那块）。
    //     ⚠ `K-R59`：`backend` 是这一拍新算进来的那一格。
    // 下面这一屏是那个分母的**真实渲染**：它必须先真的出现、且逐项等于这两格。
    localStorage.clear();
    __setHostOsForTests("windows");
    const before = gapsBoxOf(await mount([], fakePages().host));
    expect(
      before.style.display,
      "分母塌了：这一块本来就没出现（或 renderGaps 没跑）⇒ 下面那条 none 是空真",
    ).not.toBe("none");
    expect(gapKeysOf(before)).toEqual([
      `${LOCAL_MACHINE_KEY}/backend:unknown`,
      `${LOCAL_MACHINE_KEY}/acctIso:unknown`,
      `${LOCAL_MACHINE_KEY}/accounts:unknown`,
    ]);

    // 把那几格写绿 —— 这正是各自的写点会写进去的东西。
    greenLocalTwo();
    const after = gapsBoxOf(await mount([], fakePages().host));
    expect(
      after.style.display,
      "本机全绿、零远端，那一块却还挂在落地页最上面 ⇒ 那一支仍是死代码",
    ).toBe("none");
    // 「藏起来」与「清空了」是两件事，两样都断 —— 免得将来改成只清空不隐藏（或反过来）。
    expect(gapKeysOf(after)).toEqual([]);
  });

  it("★ NF2D3 先证会红：把本机那几格改回「没测过」⇒ 那一块又出现", async () => {
    localStorage.clear();
    __setHostOsForTests("windows");
    greenLocalTwo();
    expect(gapsBoxOf(await mount([], fakePages().host)).style.display).toBe("none");

    // 只抹掉本机那一栏 = 回到 `N-F2` 之前的行为（那几格从来没人写）。
    forgetMachine(LOCAL_MACHINE_KEY);
    expect(readStatus(LOCAL_MACHINE_KEY), "账本没被抹干净，下面那条不算数").toEqual({});
    const back = gapsBoxOf(await mount([], fakePages().host));
    expect(back.style.display, "回到旧行为时那一块该又出现").not.toBe("none");
    expect(back.textContent).toContain("还没测过");
    expect(gapKeysOf(back)).toEqual([
      `${LOCAL_MACHINE_KEY}/backend:unknown`,
      `${LOCAL_MACHINE_KEY}/acctIso:unknown`,
      `${LOCAL_MACHINE_KEY}/accounts:unknown`,
    ]);
  });

  // 🔴 〔步 8 · 条 80 「不要管旧配置」〕**`KR59D3` 的产品面那条也退役了。**
  //    它断的是「喂一份带旧 `daemonless: true` 的 config ⇒ 清单上真有一条带名字的告知」，
  //    而那条告知这一拍整块删了 ⇒ 没有被测对象。⚠ 用例数 −1，逐条点名在本轮报告里。

  it("★ 渲染「还差什么」不发任何后端请求（只读账本）", async () => {
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

  it("★ 列表级控件全在**一条工具条**上，且在列表之前", async () => {
    // S4b-3b：此前它们散在列表上下两侧（导入在最上、端口转发/启用 toggle 在中间、
    // 添加按钮在列表下方），空列表提示还得写「点**下方**…或从**上方**…」——
    // 一句提示同时指两个方向，本身就是布局在报警。
    const sec = await mount([mkH("a", "1.1.1.1")]);
    const bar = sec.element.querySelector<HTMLElement>(".remote-toolbar");
    expect(bar, "工具条必须在").not.toBeNull();
    const texts = [...bar!.querySelectorAll("button, select, span")].map(
      (e) => e.textContent ?? "",
    );
    for (const t of ["+ 添加机器", "批量导入…", "端口转发…", "启用远端模式"]) {
      expect(texts.some((x) => x.includes(t)), `工具条缺「${t}」`).toBe(true);
    }
    // 导入下拉也在条上（它没有稳定文案，按 tagName 认）
    expect(bar!.querySelector("select")).not.toBeNull();
    // **顺序**：工具条在机器列表之前 —— 否则「上方工具条」那句提示又变成谎话。
    const list = sec.element.querySelector<HTMLElement>(".remote-machines")!;
    expect(
      bar!.compareDocumentPosition(list) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });

  it("★ 本机是第一行、没有删除按钮", async () => {
    const sec = await mount([mkH("a", "1.1.1.1")]);
    const rows = [...sec.element.querySelectorAll<HTMLElement>(".remote-machine")];
    expect(rows[0]!.classList.contains("remote-machine-local")).toBe(true);
    expect(rows[0]!.textContent).toContain("本机");
    // 本机删不掉 —— 这不是「暂未实现」，是它本来就不该能删（§40）。
    expect(rows[0]!.querySelector(".remote-machine-remove")).toBeNull();
    // 真机器那行照样有删除按钮（反向自检：别是选择器写错了导致恒 null）
    expect(rows[1]!.querySelector(".remote-machine-remove")).not.toBeNull();
  });

  it("★ 本机行不进 this.cards —— 保存写出去的机器数不变（S1 的边界）", async () => {
    // this.cards 是 S1 保存路径的输入。本机混进去 = 往用户的远端机器列表里
    // 写一台叫「本机」的假机器。
    const sec = await mount([mkH("a", "1.1.1.1"), mkH("b", "2.2.2.2")]);
    sec.element
      .querySelectorAll<HTMLButtonElement>(".remote-machine-remove")[0]!
      .click();
    await new Promise((r) => setTimeout(r, 0));
    const got = writtenHosts();
    expect(got.map((h) => h.label)).toEqual(["b"]);
    expect(got.some((h) => h.label === "本机")).toBe(false);
  });

  /**
   * 🔴 `KR59D1` 第 ⑤ 处载体在**界面这一侧**的落点（`K-R59` 09-11）。
   *
   * 这一条此前逐字叫「本机的后端格是「不需要」，不是「缺组件」」——
   * `buildLocalRow` 那时给 `renderStatusCells` 硬塞一个 `{ backend: { kind: "na",
   * detail: "不需要" } }` 覆盖值，理由是「`watcher.rs` 直读 jsonl，本机压根不需要后端」。
   *
   * 🔴 **那句话在 `C7`〔用 08-03〕之后就不成立了**（`local_backend.rs` 是 `C7` 的产物），
   * 而这一处**一个 `daemonless` 字样都不含** —— 与 `readiness.notApplicable` 那一支同一档。
   * ⇒ 撤掉写死值：本机那一格照实画账本。
   */
  it("🔴 KR59D1⑤：本机的后端格照实画账本 —— 不再写死一个「不需要」", async () => {
    localStorage.clear();
    const sec = await mount([]);
    const local = sec.element.querySelector<HTMLElement>(".remote-machine-local")!;
    const cell = local.querySelector<HTMLElement>('[data-facet="backend"]')!;
    expect(
      cell.classList.contains("remote-status-na"),
      "本机的后端又被写死成「不适用」了 —— 那台机器上的后端没起来，用户永远看不见",
    ).toBe(false);
    expect(cell.title).not.toContain("不需要");
    // 账本空着 ⇒ 它该说「未测过」，与本机的「连接」那格同形。
    expect(cell.classList.contains("remote-status-unknown")).toBe(true);
    // 对照：本机的「连接」仍然是不适用（`INVARIANTS §40`：本地 = 不走 ssh 的远端）——
    // 本条撤的只有后端那一格，不是把整条豁免都掀了。
    const conn = local.querySelector<HTMLElement>('[data-facet="connection"]')!;
    expect(conn.classList.contains("remote-status-unknown")).toBe(true);
    expect(conn.title).toContain("未测过");
  });

  /**
   * 🔴 `K-R59`：本机 `backend` 那一格的**写点**真的在写。
   *
   * 撤掉豁免之后它是一格适用的格子，而全仓对 `LOCAL_MACHINE_KEY` 的 `recordFacet` 写点
   * 此前只有 `accounts-section.note()`（只写 `acctIso`/`accounts`）⇒ 没有本条的话
   * 它会**恒 `unknown`**，「还差什么」那张清单对任何人都清不空。
   */
  it("🔴 K-R59：本机后端起没起来，`noteLocalBackend` 真写进账本（两个方向都断）", async () => {
    for (const [reply, want] of [
      [{ channel: true, pid: 42 }, "ok"],
      [{ channel: false }, "fail"],
    ] as const) {
      localStorage.clear();
      ipcReplies.set("backend_status", reply);
      await mount([], fakePages().host);
      expect(
        readStatus(LOCAL_MACHINE_KEY).backend?.kind,
        `本机后端 channel=${String(reply.channel)} 时账本没写对`,
      ).toBe(want);
    }
    // 🔴 第三个方向：**查不到就不写**。「答不出来」不是「没有」——
    //    替用户下一个他没做过的结论，正是本模块头注最贵的那条区分。
    localStorage.clear();
    ipcReplies.delete("backend_status");
    await mount([], fakePages().host);
    expect(readStatus(LOCAL_MACHINE_KEY).backend).toBeUndefined();
  });

  it("★ 状态条读的是账本，且带年龄（不是伪装成实时）", async () => {
    // S4b：状态条从卡片 legend 移到了**列表行**上（§2.3 里状态就是列表的一列），
    // 所以这条要在分页形态下验。
    localStorage.clear();
    recordFacet("a", "connection", { kind: "ok", at: Date.now() - 3 * 60_000 });
    const p = fakePages();
    const sec = await mount([mkH("a", "1.1.1.1")], p.host);
    // 第一行是本机（S4b-2 起它也是一行、也有自己一页），远端机器从第二行开始。
    const row = sec.element.querySelectorAll<HTMLElement>(".remote-machine-row")[1]!;
    const cell = row.querySelector<HTMLElement>('[data-facet="connection"]')!;
    expect(cell.classList.contains("remote-status-ok")).toBe(true);
    expect(cell.title).toContain("3 分钟前");
  });

  // ---- S4b：每台机器一页 ----

  it("★ 有分页宿主时：每台机器开一页，列表里只留一行（表单不在列表上）", async () => {
    const p = fakePages();
    const sec = await mount([mkH("a", "1.1.1.1"), mkH("b", "2.2.2.2")], p.host);
    // 本机排第一（§40：本地就是机器列表里的一行），远端跟在后面。
    expect(p.added.map((x) => x.title)).toEqual(["本机", "a", "b"]);
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
    expect(inConn).toContain("主机 (host)");
    expect(inComp).toContain("resume 命令 · 这台机器");
    // 反向：resume 命令**不该**留在连接那半
    expect(inConn).not.toContain("resume 命令 · 这台机器");
  });

  /**
   * 🔴 〔MC1 · 2026-09-24〕`设计/71 §13` · `设计/01 §6.7`：**机器卡上只有三个动作**。
   *
   * 从前「组件」栏那一行挤着 8 颗按钮。今天两向钉：
   * · 「组件」栏的按钮**恰好**是 ① 部署后端（部署 · 卸载）与 ② 别名（装别名块 · 卸载别名块）那四颗；
   * · 「连接」栏的按钮**恰好**是动这条连接的那四颗（测试连接 · 推送公钥 · 文件 · 开新 Claude）
   *   ——（外加指纹那一格原有的「重置为 TOFU」）；
   * · 「ccm 助手 / ccm 启动器」这个词在两栏里一个字都不许有（用户逐字「装/卸 ccm 助手是假的」）。
   */
  it("★ MC1：组件栏只剩 ① 部署后端，〔ST2〕② 别名搬到「工具」栏，连接栏是动这条连接的那几颗，「ccm 助手」一个字都不剩", async () => {
    const p = fakePages();
    await mount([mkH("a", "1.1.1.1")], p.host);
    const got = p.addedParts[1]!;
    const labels = (el: HTMLElement): string[] =>
      [...el.querySelectorAll<HTMLButtonElement>("button")]
        .filter((b) => !b.closest("details"))
        .map((b) => b.textContent ?? "");
    // 〔ST2 · 协调方转主会话裁〕别名统一放「工具」栏：远端那一块与本机「工具 → 别名」同一个位置。
    expect(labels(got.components)).toEqual(["部署后端", "卸载后端"]);
    // 〔AL2 · 第四波 4D〕② 别名是与本机同一个组件（`buildAliasManager`，`data-origin` = 这台）；装 / 卸在组件里，
    //   卸那一颗按 V134 叫「卸载 ccm」（V80 原裁）。组件是 `<details>`，栏上裸露的按钮一颗都不剩。
    expect(labels(got.tools)).toEqual([]);
    const mgr = got.tools.querySelector<HTMLElement>(".machine-aliases");
    expect(mgr?.dataset.origin).toBe("a");
    expect([...mgr!.querySelectorAll("button")].map((b) => b.textContent)).toContain("卸载 ccm");
    expect(got.tools.textContent).toContain("别名");
    expect(labels(got.connection).filter((t) => t !== "重置主机指纹")).toEqual([
      "测试连接",
      "推送公钥",
      "文件",
      "开新 Claude",
    ]);
    for (const part of [got.connection, got.components, got.tools]) {
      const txt = [part.textContent ?? "", ...[...part.querySelectorAll("[title]")].map((e) => e.getAttribute("title") ?? "")].join("\n");
      expect(txt).not.toMatch(/ccm (助手|启动器)/);
    }
  });

  it("本机页不带 parts（它没有卡片，不该被拆栏）", async () => {
    const p = fakePages();
    await mount([], p.host);
    expect(p.added[0]!.title).toBe("本机");
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
    sec.element
      .querySelectorAll<HTMLButtonElement>(".remote-machine-remove")[0]!
      .click();
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
    expect(p.added.map((x) => x.title)).toEqual(["本机", "a"]);

    vi.mocked(loadConfig).mockResolvedValue({
      remote: { enabled: true, hosts: [mkH("b", "2.2.2.2")] },
    } as unknown as Awaited<ReturnType<typeof loadConfig>>);
    await sec.refresh();

    expect(p.removed).toContain("machine:a");
    expect(p.added.map((x) => x.title)).toEqual(["本机", "a", "本机", "b"]);
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

  it("删掉一台机器会连它的状态记录一起清（下一台同名的不该继承 ✓）", async () => {
    localStorage.clear();
    recordFacet("a", "connection", { kind: "ok", at: Date.now() });
    const sec = await mount([mkH("a", "1.1.1.1")]);
    sec.element
      .querySelectorAll<HTMLButtonElement>(".remote-machine-remove")[0]!
      .click();
    await new Promise((r) => setTimeout(r, 0));
    expect(readStatus("a")).toEqual({});
  });

  it("★ SSH 不通时**不**给后端那格下结论", async () => {
    // SSH 都没通，backend 是「不知道」。记成 fail 等于替用户断言「远端没装后端」，
    // 而事实可能只是网络不通 —— 那条结论会一直挂在列表行上误导人。
    localStorage.clear();
    ipcReplies.set("test_remote_connection", {
      sshOk: false,
      backendOk: false,
      fingerprint: null,
      endpoint: null,
      backendHello: null,
    });
    const sec = await mount([mkH("a", "1.1.1.1")]);
    const btns = [...sec.element.querySelectorAll<HTMLButtonElement>("button")];
    const testBtn = btns.find((b) => b.textContent?.includes("测试连接"))!;
    testBtn.click();
    for (let i = 0; i < 10; i++) await new Promise((r) => setTimeout(r, 0));
    expect(ipcCalls).toContain("test_remote_connection");
    const st = readStatus("a");
    expect(st.connection?.kind).toBe("fail");
    expect(st.backend).toBeUndefined();
    ipcReplies.clear();
  });

  /**
   * 〔FE1〕`设计/01 §5` D4「一条都不许静默忽略」：「开新 Claude」替用户派生的默认名要过铸名口，
   * **名单没问到 ⇒ 不起、出声**。先前这里「列不出来就用空集铸名」—— 同一个 cwd 派生出同一个名字，
   * 撞上远端 `create-or-attach` 的幂等闸，静默接进第一个会话（#76）。
   * 正控：名单问到了（零会话 = 空表）⇒ 照常往下走到渲染那一跳。
   */
  // 〔MIG-2〕起会话那几问（中转地址 `launch-endpoint` → 渲染 `launch-render-*`）改问那台后端（`src/launch-render.ts`），
  //   不再是 `commands.*` 包装 ⇒ 看通道：问到了其中第一问就算「往下走到了」（本桩不答，后面几问不会发）。
  const renderAsked = (): boolean => chanOps.some((op) => /^launch-(?:endpoint|render-)/.test(op));
  const openLauncherAndStart = async (listing: unknown): Promise<void> => {
    ipcCalls.length = 0;
    chanOps.length = 0;
    ipcReplies.set("list_remote_tmux", listing);
    const sec = await mount([mkH("a", "1.1.1.1")]);
    const btns = [...sec.element.querySelectorAll<HTMLButtonElement>("button")];
    btns.find((b) => b.textContent === "开新 Claude")!.click();
    const back = document.querySelector<HTMLElement>(".launcher-back")!;
    expect(back, "「开新 Claude」的对话框没开出来 —— 下面的断言会零命中地绿").toBeTruthy();
    back.querySelector<HTMLInputElement>("input")!.value = "/home/u/proj";
    [...back.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === "开始")!.click();
    for (let i = 0; i < 10; i++) await new Promise((r) => setTimeout(r, 0));
  };

  it("★ 〔FE1〕开新 Claude：远端 tmux 名单没问到 ⇒ 不起、出声（不拿空集铸名）", async () => {
    document.body.innerHTML = "";
    await openLauncherAndStart(new Error("ssh 抖动"));
    expect(ipcCalls).toContain("list_remote_tmux");
    expect(renderAsked(), "名单没问到还往下起了").toBe(false);
    expect(ipcCalls).not.toContain("launch_remote_terminal");
    const toast = document.body.textContent ?? "";
    expect(toast).toContain("没有起会话");
    expect(toast).toContain("ssh 抖动");
    ipcReplies.clear();
    document.body.innerHTML = "";
  });

  it("正控：名单问到了（零会话 = 空表）⇒ 往下走到渲染那一跳", async () => {
    document.body.innerHTML = "";
    await openLauncherAndStart([]);
    expect(renderAsked()).toBe(true);
    expect(document.body.textContent ?? "").not.toContain("没有起会话");
    ipcReplies.clear();
    document.body.innerHTML = "";
  });

  it("SSH 通了才给后端下结论（反向对照：别是恒不记）", async () => {
    localStorage.clear();
    ipcReplies.set("test_remote_connection", {
      sshOk: true,
      backendOk: true,
      fingerprint: null,
      endpoint: null,
      backendHello: null,
    });
    const sec = await mount([mkH("a", "1.1.1.1")]);
    const btns = [...sec.element.querySelectorAll<HTMLButtonElement>("button")];
    btns.find((b) => b.textContent?.includes("测试连接"))!.click();
    await new Promise((r) => setTimeout(r, 0));
    const st = readStatus("a");
    expect(st.connection?.kind).toBe("ok");
    expect(st.backend?.kind).toBe("ok");
    ipcReplies.clear();
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// ST1「那个勾选框会自己跳」（`设计/70 §1` 截图对比 ＋ `§8` 判据 #2）：
// 「启用远端模式」在配置读回来之前**不可交互**；读失败就一直灰着（那一刻它显示的不是盘上的值）。
// ─────────────────────────────────────────────────────────────────────────────
describe("ST1：「启用远端模式」读回来之前不可点", () => {
  beforeEach(() => vi.resetAllMocks());
  const box = (sec: RemoteSection) =>
    [...sec.element.querySelectorAll<HTMLInputElement>("input[type=checkbox]")].find(
      (c) => c.parentElement?.textContent?.includes("启用远端模式"),
    )!;

  it("读回来之前灰着，读回来之后可点、且值就是盘上的值", async () => {
    type Cfg = Awaited<ReturnType<typeof loadConfig>>;
    let release!: (v: Cfg) => void;
    vi.mocked(loadConfig).mockReturnValue(
      new Promise((r) => (release = r)) as ReturnType<typeof loadConfig>,
    );
    const sec = new RemoteSection({ headless: true });
    const cb = box(sec);
    expect(cb, "找不到那个复选框 —— 下面全是空真").toBeTruthy();
    expect(cb.disabled, "配置还没读回来就能点 —— 用户点下去的是一个假状态").toBe(true);
    release({ remote: { enabled: true, hosts: [] } } as unknown as Cfg);
    await new Promise((r) => setTimeout(r, 0));
    expect(cb.disabled).toBe(false);
    expect(cb.checked).toBe(true);
  });

  it("读失败 ⇒ 一直灰着（显示的不是盘上的值，点它就是写假状态回去）", async () => {
    // ⚠ `readRemoteConfig` 自己会把 `loadConfig` 的异常吞成默认值 ⇒ 要量「读失败」这一支，
    //   得让它**本身** reject（`refresh()` 的 catch 那一格）。
    const spy = vi.spyOn(remoteConfigModule, "readRemoteConfig").mockRejectedValue(new Error("读不到"));
    const sec = new RemoteSection({ headless: true });
    await new Promise((r) => setTimeout(r, 0));
    expect(spy, "那条读口没被调 —— 下面是空真").toHaveBeenCalled();
    expect(box(sec).disabled).toBe(true);
    spy.mockRestore();
  });
});

// 〔S5 · 第四波〕要求住址：`调研/设计/99 §1` V41「不为旧配置留兼容」；主会话 09-24 裁
// 「认不出就不显示那台、在机器页顶上一句『远端配置认不出：…』」。
describe("〔S5 · V41〕remote 段认不出 ⇒ 机器列表顶上说一句、一台都不显示", () => {
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

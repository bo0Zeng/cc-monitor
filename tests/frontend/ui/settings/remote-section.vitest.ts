// F43：指纹重置按钮显隐纯逻辑。remote-section 的 DOM 主体重(拉整卡),这里只钉住
// 「有固化指纹才显示重置按钮」这条判定,防未来误改成空指纹也显示(重置无意义)。
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
// F56：写入/读取都走 config.ts；mock 掉以测 jump write→read 往返。
// S1：写入口从 writeRemoteConfig（整表覆盖，已取消导出）改为 patchRemoteConfig（局部合并）。 〔散文墓碑〕
// config 写只交补丁；替身把补丁应用到 `loadConfig` 摆的那份上，写完的整份交 `fakeCfg.saved`。
vi.mock("../../../../src/frontend/ui/config", async (orig) => (await import("../config-patch-fake")).mockedConfigModule(orig));
// S3：把整个 IPC 面 mock 成一个**会记账的 Proxy** —— 用来钉「渲染机器列表时零次
// 后端调用」。这比源码扫描强：扫描只能证明「没 import」，证明不了「渲染时没调」。
const { ipcCalls, chanOps, ipcReplies } = vi.hoisted(() => ({
  ipcCalls: [] as string[],
  /** 经 `commands.chan_call` 发出去的那几问的 op（按发出顺序）。 */
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
vi.mock("../../../../src/frontend/ui/ipc/commands", () => ({
  commands: new Proxy(
    {},
    {
      get: (_t, name: string) => (...args: unknown[]) => {
        ipcCalls.push(name);
        const op = name === "chan_call" ? String((args[0] as { op?: unknown } | undefined)?.op) : "";
        if (name === "chan_call") chanOps.push(op);
        // 铸名那一问（`terminal-name-mint`）按帧命令名回：名字 ⇒ 那台后端的成品 `{name}`；别的 ⇒ 原样 reject（通道那一层的错）。
        if (op === "terminal-name-mint") {
          const minted = ipcReplies.get(op);
          if (typeof minted !== "string") return Promise.reject(minted);
          const u = new TextEncoder().encode(JSON.stringify({ name: minted }));
          return Promise.resolve(u.buffer.slice(u.byteOffset, u.byteOffset + u.byteLength));
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
import { fakeCfg } from "../config-patch-fake";
import { copyText } from "../../../../src/frontend/ui/copy-table";
import { dispatcher } from "../../../../src/frontend/ui/keybindings/registry";
import { refusedReply } from "../../../test-support/chan-fake";
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
// `N-F2`：`forgetMachine` 是本文件末尾那条「先证会红」用的 —— 只抹本机那一栏，
// 而不是 `localStorage.clear()`，这样「回到旧行为」这句话是按机器说的，不是按整本账说的。
import {
  recordFacet,
  readStatus,
  LOCAL_MACHINE_KEY,
} from "../../../../src/frontend/ui/settings/machine-status";
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
  for (let i = 0; i < 3; i++) await new Promise((r) => setTimeout(r, 0));
  return document.querySelector<HTMLElement>('[role="dialog"]')!;
}

/** 删第 i 台远端（行的 ⋯ →「从列表删除」，没有行时点卡片自己的删除）；`commit` ⇒ 点掉撤销条（到点同一条路）。 */
function removeAt(sec: RemoteSection, i: number, commit = true): void {
  const rows = [...sec.element.querySelectorAll<HTMLElement>(".remote-machine-row:not(.remote-machine-local)")];
  if (rows.length > 0) {
    rows[i]!.querySelector<HTMLButtonElement>("button[aria-label]")!.click();
    menuItem("从列表删除").click();
  } else {
    sec.element.querySelectorAll<HTMLButtonElement>(".remote-machine-remove")[i]!.click();
  }
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
    connect: true,
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
import { describeStage } from "../../../../src/frontend/ui/settings/machine-card";

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
    host: "10.0.0.2",
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
      | { connection: HTMLElement; components: HTMLElement; terminal: HTMLElement }
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
          parts?: { connection: HTMLElement; components: HTMLElement; terminal: HTMLElement },
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
    removeAt(sec, 0);
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
    const ayaPage = p.added.find((a) => a.id === "machine:a")!;
    expect(ayaPage, "devbox 那一页要注册进来了，否则下面全是空转").toBeTruthy();
    const first = ayaPage.element.querySelectorAll<HTMLInputElement>('input[type="text"]')[0]!;
    first.value = "a-renamed";
    first.dispatchEvent(new Event("change"));
    await new Promise((r) => setTimeout(r, 0));

    // 修之前：save() 把 persistedKey 改成新 origin，而页是按旧 origin 注册的
    // ⇒ removeCard 现算出 `machine:a-renamed`（不存在）⇒ 三者全留下，盘上却真删了。
    removeAt(sec, 0);
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
    removeAt(sec, 0);
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
    removeAt(sec, 0);
    await new Promise((r) => setTimeout(r, 0));
    const calls = vi.mocked(saveConfig).mock.calls;
    const last = calls[calls.length - 1]![0] as Record<string, unknown>;
    expect(last.keepMe).toBe(1);
  });

  // 读 `~/.ssh/config` 失败与「真没有别名」原先同形（空下拉 ＋「未找到」）。

  it("★ 渲染机器列表：后端调用**不随机器数增长**（状态灯绝不引入轮询）", async () => {
    // 红线。「打开设置时顺便把 N 台机器都探一遍」听起来不像轮询，
    // 但它是同一件事的另一种说法：一次 UI 动作扇出 N 次 ssh 往返，用户没要求过。
    //
    // 判据**不是**「零调用」—— 实测渲染时确实有一次 `ssh-config-aliases`（问本机后端）
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



  // 「★ S9：本机的 ccm 条目跟着 monitor 的 OS 走」那一条删了：它钉的是 monitor 跑在哪个 OS 传进 `computeGaps` 的接线，
  //   那个入参随 Windows 豁免一起删了（`readiness.ts::notApplicable` 头注第 3 条），没有被测对象。

  // ───────────────────────────────────────────────────────────────────────────
  // `N-F2` `NF2D3` **最后那一跳**：`summary === null` ⇒ 这一块**整块不出现**
  //（`remote-section.ts` 里 `renderGaps` 的 `if (!summary)` 那一支）。
  //
  // # 它此前是死代码，而这里是唯一断得到它的地方
  //
  // 本机的 `accounts` 在 `N-F2` 之前**全仓没有任何 `recordFacet` 生产者**
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

  // 🔴 〔条 80 「不要管旧配置」〕**`KR59D3` 的产品面那条也退役了。**
  //    它断的是「喂一份带旧 `daemonless: true` 的 config ⇒ 清单上真有一条带名字的告知」，
  //    而那条告知这一拍整块删了 ⇒ 没有被测对象。⚠ 用例数 −1，逐条点名在本轮报告里。

  it("★ 渲染机器列表不发任何后端请求（只读账本）", async () => {
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
    expect(add!.textContent).toContain("添加机器");
    more!.click();
    expect(menuItem("端口转发")).toBeTruthy();
    expect(menuItem("全部刷新")).toBeTruthy();
    // 全局「启用远端模式」退场（每台自己的「连接这台」）；单个导入的下拉也退场了。
    expect(sec.element.textContent).not.toContain("启用远端模式");
  });

  it("★ 本机是第一行、没有删除按钮", async () => {
    const sec = await mount([mkH("a", "1.1.1.1")], fakePages().host);
    const rows = [...sec.element.querySelectorAll<HTMLElement>(".remote-machine-row")];
    expect(rows[0]!.classList.contains("remote-machine-local")).toBe(true);
    expect(rows[0]!.textContent).toContain("本机");
    // 本机删不掉（§40）：它的 ⋯ 里没有「从列表删除」；真机器那行有（反向自检）。
    const labelsOf = (row: HTMLElement): string[] => sec.menuFor(row.dataset.pageId!).map((m) => m.label);
    expect(labelsOf(rows[0]!)).not.toContain("从列表删除");
    expect(labelsOf(rows[1]!)).toContain("从列表删除");
  });

  it("★ 本机行不进 this.cards —— 保存写出去的机器数不变（S1 的边界）", async () => {
    // this.cards 是 S1 保存路径的输入。本机混进去 = 往用户的远端机器列表里
    // 写一台叫「本机」的假机器。
    const sec = await mount([mkH("a", "1.1.1.1"), mkH("b", "2.2.2.2")]);
    removeAt(sec, 0);
    await new Promise((r) => setTimeout(r, 0));
    const got = writtenHosts();
    expect(got.map((h) => h.label)).toEqual(["b"]);
    expect(got.some((h) => h.label === "本机")).toBe(false);
  });

  /**
   * 🔴 `K-R59`：本机 `backend` 那一格的**写点**真的在写。
   *
   * 撤掉豁免之后它是一格适用的格子，而全仓对 `LOCAL_MACHINE_KEY` 的 `recordFacet` 写点
   * 此前只有 `accounts-section.note()`（只写 `accounts`）⇒ 没有本条的话
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

  /**
   * 「本机 ccm 那一格两件都报」· 「补本机·ccm 那一格的写点」：
   * `noteLocalCcm` 照 monitor 那一侧的判定记账 —— 两件都成 ⇒ ok；有一件不成 ⇒ fail 且那句话照记；说不清 ⇒ 不写；Windows 本机不问。
   */
  it("FIX3 ㉔：本机 ccm 那一格由 `noteLocalCcm` 写，照 ok / 不写；〔WF1〕Windows 上同样问", async () => {
    __setHostOsForTests("linux");
    for (const [ok, want] of [
      [true, "ok"],
      [false, "fail"],
    ] as const) {
      localStorage.clear();
      ipcReplies.set("local_ccm_entry_status", { ok, summary: `S-${String(ok)}` });
      await mount([], fakePages().host);
      const cell = readStatus(LOCAL_MACHINE_KEY).ccm;
      expect(cell?.kind, `ok=${String(ok)} 时账本没写对`).toBe(want);
      expect(cell?.detail).toBe(`S-${String(ok)}`);
    }
    localStorage.clear();
    ipcReplies.set("local_ccm_entry_status", { ok: null, summary: "" });
    await mount([], fakePages().host);
    expect(readStatus(LOCAL_MACHINE_KEY).ccm, "说不清却写了账本").toBeUndefined();
    localStorage.clear();
    ipcCalls.length = 0;
    __setHostOsForTests("windows");
    ipcReplies.set("local_ccm_entry_status", { ok: true, summary: "x" });
    await mount([], fakePages().host);
    // Windows 本机同样问、同样记（新开的 PowerShell 里敲 `ccm` 走到哪）。
    expect(ipcCalls).toContain("local_ccm_entry_status");
    expect(readStatus(LOCAL_MACHINE_KEY).ccm?.kind).toBe("ok");
    ipcReplies.delete("local_ccm_entry_status");
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
    expect(inConn).toContain("地址");
    expect(inComp).toContain("恢复命令");
    // 反向：恢复命令**不该**留在连接那半
    expect(inConn).not.toContain("恢复命令");
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
        .filter((b) => !b.closest("details") && b.getAttribute("role") !== "switch")
        .map((b) => b.textContent ?? "");
    // 别名放「终端」栏：远端那一块与本机「终端 → 别名」同一个位置。
    expect(labels(got.components)).toEqual(["更新", "从这台卸载…"]);
    // ② 别名是与本机同一个组件（`buildAliasManager`，`data-origin` = 这台）；接入 / 卸载在组件里（卸那一颗叫「卸载 ccm」、
    //   接入之后才出现 —— 由 `machine-aliases.vitest.ts` 的远端卡那一条钉）。组件是 `<details>`，栏上裸露的按钮一颗都不剩。
    expect(labels(got.terminal)).toEqual([]);
    const mgr = got.terminal.querySelector<HTMLElement>(".machine-aliases");
    expect(mgr?.dataset.origin).toBe("a");
    expect(got.terminal.textContent).toContain("别名");
    expect(labels(got.connection).filter((t) => t !== "忘记…")).toEqual([
      "更多：备用地址 · 跳板机",
      "测试连接",
      "推送公钥…",
    ]);
    for (const part of [got.connection, got.components, got.terminal]) {
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
    removeAt(sec, 0);
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
    removeAt(sec, 0);
    await new Promise((r) => setTimeout(r, 0));
    expect(readStatus("a")).toEqual({});
  });

  it("★ SSH 不通时**不**给后端那格下结论", async () => {
    // SSH 都没通，backend 是「不知道」。记成 fail 等于替用户断言「远端没装后端」，
    // 而事实可能只是网络不通 —— 那条结论会一直挂在列表行上误导人。
    localStorage.clear();
    ipcReplies.set("remote-probe", {
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
    expect(ipcCalls).toContain("remote-probe");
    const st = readStatus("a");
    expect(st.connection?.kind).toBe("fail");
    expect(st.backend).toBeUndefined();
    ipcReplies.clear();
  });

  /**
   * 「一条都不许静默忽略」：「开新 Claude」替用户派生的默认名要过铸名口，
   * **铸不出 ⇒ 不起、出声**。先前这里「列不出来就用空集铸名」—— 同一个 cwd 派生出同一个名字，
   * 撞上远端 `create-or-attach` 的幂等闸，静默接进第一个会话（#76）。铸名口是那台后端的 `terminal-name-mint`。
   * 正控：那台铸了名字 ⇒ 照常往下走到渲染那一跳。
   */
  // 起会话那几问（中转地址 `launch-endpoint` → 渲染 `launch-render-*`）改问那台后端（`src/frontend/ui/launch-render.ts`），
  //   不再是 `commands.*` 包装 ⇒ 看通道：问到了其中第一问就算「往下走到了」（本桩不答，后面几问不会发）。
  const renderAsked = (): boolean => chanOps.some((op) => /^launch-(?:endpoint|render-)/.test(op));
  const openLauncherAndStart = async (minted: unknown): Promise<void> => {
    ipcCalls.length = 0;
    chanOps.length = 0;
    ipcReplies.set("terminal-name-mint", minted);
    const sec = await mount([mkH("a", "1.1.1.1")], fakePages().host);
    sec.menuFor("machine:a").find((m) => m.label === "新建会话…")!.onClick!();
    const back = document.querySelector<HTMLElement>(".launcher-back")!;
    expect(back, "「开新 Claude」的对话框没开出来 —— 下面的断言会零命中地绿").toBeTruthy();
    back.querySelector<HTMLInputElement>("input")!.value = "/home/u/proj";
    [...back.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === "开始")!.click();
    for (let i = 0; i < 10; i++) await new Promise((r) => setTimeout(r, 0));
  };

  it("★ 〔FE1〕开新 Claude：那台铸不出名字 ⇒ 不起、出声（不自己拼一个不避让的名字）", async () => {
    document.body.innerHTML = "";
    await openLauncherAndStart(refusedReply("invalid_args", "ssh 抖动"));
    expect(chanOps).toContain("terminal-name-mint");
    expect(renderAsked(), "铸不出名字还往下起了").toBe(false);
    expect(ipcCalls).not.toContain("launch_remote_terminal");
    const toast = document.body.textContent ?? "";
    expect(toast).toContain("没有起会话");
    expect(toast).toContain("ssh 抖动");
    ipcReplies.clear();
    document.body.innerHTML = "";
  });

  it("正控：那台铸了名字 ⇒ 往下走到渲染那一跳", async () => {
    document.body.innerHTML = "";
    await openLauncherAndStart("proj-cc");
    expect(renderAsked()).toBe(true);
    expect(document.body.textContent ?? "").not.toContain("没有起会话");
    ipcReplies.clear();
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
      backendHello: copyText("beProbe.hello.ok", { build: "p5o", usable: "40", gaps: "3", ms: "12" }),
      backendGaps: [{ code: "no_tmux", count: 3 }],
    });
    const sec = await mount([mkH("a", "1.1.1.1")]);
    [...sec.element.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent?.includes("测试连接"))!.click();
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

  it("SSH 通了才给后端下结论（反向对照：别是恒不记）", async () => {
    localStorage.clear();
    ipcReplies.set("remote-probe", {
      sshOk: true,
      backendOk: true,
      fingerprint: null,
      endpoint: null,
      backendHello: null,
    });
    const sec = await mount([mkH("a", "1.1.1.1")]);
    const btns = [...sec.element.querySelectorAll<HTMLButtonElement>("button")];
    btns.find((b) => b.textContent?.includes("测试连接"))!.click();
    for (let i = 0; i < 10; i++) await new Promise((r) => setTimeout(r, 0)); // 先读一次已保存的机器、再问本机后端
    const st = readStatus("a");
    expect(st.connection?.kind).toBe("ok");
    expect(st.backend?.kind).toBe("ok");
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
    await tick();
    await tick();
  };

  it("每台的续跑命令重开后照样回填；改别台时不被抹成空", async () => {
    const p = fakePages();
    await mount([{ ...mkH("a", "1.1.1.1"), resumeCommand: "ccm resume --tmux", connect: true }, mkH("b", "2.2.2.2")], p.host);
    expect(field(pageOf(p, "machine:a"), copyText("machineCard.field.resumeCmdHint")).value).toBe("ccm resume --tmux");
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
      { label: "a", host: "1.1.1.1", port: 22, user: "u", keyPath: null, addresses: [], jump: null, members: [{ alias: "a", host: "1.1.1.1", port: 22, proxyJump: null }] },
      { label: "gpu", host: "9.9.9.9", port: 22, user: "u", keyPath: null, addresses: ["9.9.9.8"], jump: null, members: [{ alias: "gpu", host: "9.9.9.9", port: 22, proxyJump: null }, { alias: "gpu-b", host: "9.9.9.8", port: 22, proxyJump: null }] },
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
      expect(okBtn(dlg).textContent).toBe("添加 1 台");
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
      expect(dlg.textContent).toContain("gpu 已存在");
      const name = dlg.querySelector<HTMLInputElement>(".add-machine-row input:not([type=checkbox])")!;
      name.value = "gpu-2";
      name.dispatchEvent(new Event("input"));
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
      expect(row.querySelector(".remote-machine-word")!.textContent).toBe("已停用");
      expect(row.querySelector<HTMLElement>("[data-state]")!.dataset.state).toBe("exited");
      [...row.querySelectorAll<HTMLButtonElement>(".machine-problem button")].find((b) => b.textContent === "连接")!.click();
      for (let i = 0; i < 6; i++) await tick();
      expect(writtenHosts().map((h) => [h.label, h.connect])).toEqual([["a", true], ["b", true]]);
      expect(ipcCalls.filter((c) => c === "remote_reconcile")).toHaveLength(2);
      expect(row.querySelector(".remote-machine-word")!.textContent).toBe("");
    });
  });

  // Esc 一次只关最上面一层：机器页上的三个小框（批量导入预览 · 开新 Claude · 端口转发）关掉自己，设置窗不跟着关。
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

    it("开新 Claude", async () => {
      const p = fakePages();
      const sec = await mount([mkH("a", "1.1.1.1")], p.host);
      sec.menuFor("machine:a").find((m) => m.label === "新建会话…")!.onClick!();
      expect(document.querySelector(".launcher-back"), "前提：对话框开着").toBeTruthy();
      esc();
      expect(document.querySelector(".launcher-back")).toBeNull();
      expect(panelEsc).not.toHaveBeenCalled();
    });

    it("端口转发", async () => {
      const sec = await mount([], fakePages().host);
      sec.headActions()[1]!.click();
      menuItem("端口转发").click();
      await tick();
      const pf = document.querySelector<HTMLElement>(".pf-overlay")!;
      expect(pf.style.display, "前提：面板开着").not.toBe("none");
      esc();
      expect(pf.style.display).toBe("none");
      expect(panelEsc).not.toHaveBeenCalled();
    });
  });

  it("卸载后端失败：列表那一格不记成「已卸载」", async () => {
    localStorage.clear();
    ipcReplies.set("uninstall_remote_backend", new Error("ssh: connect refused"));
    const p = fakePages();
    await mount([mkH("a", "1.1.1.1")], p.host);
    const pageA = pageOf(p, "machine:a");
    [...pageA.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === copyText("machineCard.deploy.uninstall"))!.click();
    await tick();
    [...document.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === copyText("machineCard.uninstall.action"))!.click();
    for (let i = 0; i < 4; i++) await tick();
    expect(readStatus("a").backend?.detail).not.toBe(copyText("machineCard.status.uninstalled"));
    ipcReplies.clear();
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


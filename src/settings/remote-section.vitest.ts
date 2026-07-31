// F43：指纹重置按钮显隐纯逻辑。remote-section 的 DOM 主体重(拉整卡),这里只钉住
// 「有固化指纹才显示重置按钮」这条判定,防未来误改成空指纹也显示(重置无意义)。
import { describe, it, expect, vi, beforeEach } from "vitest";
// F56：写入/读取都走 config.ts；mock 掉以测 jump write→read 往返。
// S1：写入口从 writeRemoteConfig（整表覆盖，已取消导出）改为 patchRemoteConfig（局部合并）。
vi.mock("../config", () => ({ loadConfig: vi.fn(), saveConfig: vi.fn() }));
// S3：把整个 IPC 面 mock 成一个**会记账的 Proxy** —— 用来钉「渲染机器列表时零次
// 后端调用」。这比源码扫描强：扫描只能证明「没 import」，证明不了「渲染时没调」。
const { ipcCalls, ipcReplies } = vi.hoisted(() => ({
  ipcCalls: [] as string[],
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
vi.mock("../ipc/commands", () => ({
  commands: new Proxy(
    {},
    {
      get: (_t, name: string) => (...args: unknown[]) => {
        ipcCalls.push(name);
        void args;
        return Promise.resolve(ipcReplies.get(name));
      },
    },
  ),
}));
import { loadConfig, saveConfig } from "../config";
import {
  shouldShowResetFingerprint,
  describeStage,
  RemoteSection,
  LOCAL_MACHINE_PAGE_ID,
} from "./remote-section";
// F12：数据层已抽到 src/remote-config.ts——数据函数/类型从那里 import。
import {
  parseAddressLines,
  findHostByOrigin,
  patchRemoteConfig,
  readRemoteConfig,
  sftpEligibleHosts,
} from "../remote-config";
import type { RemoteHostConfig, RemoteConfig } from "../remote-config";
import { recordFacet, readStatus } from "./machine-status";

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

// F08 Phase D 审计：buildAliasLine/别名生成器 UI 已迁到 src/launcher-diagnostics.ts（与它
// 诊断的对象——远端 resume 命令输入框——放在同一处设置分组，紧挨着，不再按主机重复渲染）；
// 相关单测随之搬到 src/launcher-diagnostics.vitest.ts。

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
    daemonPath: "",
    hostKeyFingerprint: "",
    addresses: [],
    jump: "",
    daemonless: false,
    resumeCommand: "",
      });
  const hosts = [mkHost("aya", "10.0.0.2"), mkHost("", "pi.local")];
  it("命中 label", () => {
    expect(findHostByOrigin(hosts, "aya")?.host).toBe("10.0.0.2");
  });
  it("label 空 → 回退 host 匹配", () => {
    expect(findHostByOrigin(hosts, "pi.local")?.host).toBe("pi.local");
  });
  it("找不到 / 空列表 → null", () => {
    expect(findHostByOrigin(hosts, "nope")).toBeNull();
    expect(findHostByOrigin([], "aya")).toBeNull();
  });
});

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
    label: "aya",
    host: "10.0.0.2",
    port: 22,
    user: "u",
    keyPath: "",
    daemonPath: "/d",
    hostKeyFingerprint: "",
    addresses: [],
    jump,
    daemonless: false,
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
  // `jump` 与 `daemonless` 都曾因为「加了字段但序列化清单没跟上」被静默丢掉。
  // S1 的编译期穷尽检查挡住了「漏写清单」，但挡不住「读盘那侧忘了解析」——
  // 这条往返把另一半也钉住。
  const host = (resumeCommand: string): RemoteHostConfig => ({
    label: "aya",
    host: "10.0.0.2",
    port: 22,
    user: "u",
    keyPath: "",
    daemonPath: "/d",
    hostKeyFingerprint: "",
    addresses: [],
    jump: "",
    daemonless: false,
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
      remote: { enabled: true, hosts: [{ label: "aya", host: "1.1.1.1" }] },
    });
    const back = await readRemoteConfig();
    expect(back.hosts[0]!.resumeCommand).toBe("");
  });
});

describe("F59 daemonless write→read 往返（D-B1 同源回归：布尔字段不丢）", () => {
  const host = (daemonless: boolean): RemoteHostConfig => ({
    label: "aya",
    host: "10.0.0.2",
    port: 22,
    user: "u",
    keyPath: "",
    daemonPath: "/d",
    hostKeyFingerprint: "",
    addresses: [],
    jump: "",
    daemonless,
    resumeCommand: "",
  });

  it("daemonless=true 写入 config 并读回不丢", async () => {
    vi.mocked(loadConfig).mockResolvedValue({});
    let saved: Record<string, unknown> = {};
    vi.mocked(saveConfig).mockImplementation(async (c: unknown) => {
      saved = c as Record<string, unknown>;
    });
    await patchRemoteConfig({ enabled: true, upsert: [{ key: null, value: host(true) }] });
    // 写入的 config 里 hosts[0] 含 daemonless（漏写 → undefined，测试红，同 F56 D-B1）
    const written = (saved.remote as { hosts: Array<{ daemonless?: boolean }> }).hosts[0];
    expect(written.daemonless).toBe(true);
    // 读回:coerceHost 保留布尔
    vi.mocked(loadConfig).mockResolvedValue(saved);
    const back = await readRemoteConfig();
    expect(back.hosts[0].daemonless).toBe(true);
  });

  it("缺省 daemonless → false（旧配置零迁移）", async () => {
    // 旧 config：hosts[0] 无 daemonless 键 → coerceHost 回退 false。
    vi.mocked(loadConfig).mockResolvedValue({
      remote: {
        enabled: true,
        hosts: [{ host: "10.0.0.2", user: "u", daemonPath: "/d" }],
      },
    });
    const back = await readRemoteConfig();
    expect(back.hosts[0].daemonless).toBe(false);
  });
});

describe("F83 sftpEligibleHosts", () => {
  const mk = (over: Partial<RemoteHostConfig>): RemoteHostConfig => ({
    label: "",
    host: "",
    port: 22,
    user: "",
    keyPath: "",
    daemonPath: "",
    hostKeyFingerprint: "",
    addresses: [],
    jump: "",
    daemonless: false,
    resumeCommand: "",
    ...over,
  });
  const cfg = (hosts: RemoteHostConfig[]): RemoteConfig => ({ enabled: false, hosts });

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
    daemonPath: "/d",
    hostKeyFingerprint: "",
    addresses: [],
    jump: "",
    daemonless: false,
    resumeCommand: "",
      });

  /** S4b：一个假的分页宿主，记录开了哪些页 / 跳去了哪一页。 */
  function fakePages() {
    const added: { id: string; title: string; element: HTMLElement }[] = [];
    const addedParts: (
      | { connection: HTMLElement; components: HTMLElement }
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
          parts?: { connection: HTMLElement; components: HTMLElement },
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

  it("★ 两台机器名相同时删掉一台：真的少一台（不是静默无效）", async () => {
    // 删除基准若按**集合**算，这里会得出 remove=[]（另一张卡还占着同一个 key）
    // ⇒ 删除静默失效。老的整表覆盖写法没这个问题，所以这是必须挡住的回归。
    const sec = await mount([mkH("dup", "1.1.1.1"), mkH("dup", "2.2.2.2")]);
    sec.element
      .querySelectorAll<HTMLButtonElement>(".remote-machine-remove")[0]!
      .click();
    await new Promise((r) => setTimeout(r, 0));
    const got = writtenHosts();
    expect(got).toHaveLength(1);
    expect(got[0]?.host).toBe("2.2.2.2");
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

  it("★ 渲染机器列表：后端调用**不随机器数增长**（状态灯绝不引入轮询）", async () => {
    // 主计划 §1-2 的红线。「打开设置时顺便把 N 台机器都探一遍」听起来不像轮询，
    // 但它是同一件事的另一种说法：一次 UI 动作扇出 N 次 ssh 往返，用户没要求过。
    //
    // 判据**不是**「零调用」—— 实测渲染时确实有一次 `list_ssh_host_aliases`
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
    recordFacet("a", "daemon", { kind: "fail", at: Date.now() });
    const sec = await mount([mkH("a", "1.1.1.1")], fakePages().host);
    const box = sec.element.querySelector<HTMLElement>(".remote-gaps")!;
    expect(box.textContent).toContain("确认缺");
    // **按机器筛**：清单里同时有本机的条目（本机也没测过），不区分 origin 会误判。
    const ofA = [...box.querySelectorAll<HTMLElement>('.remote-gap[data-origin="a"]')].map(
      (i) => `${i.dataset.facet}:${i.dataset.kind}`,
    );
    expect(ofA).toContain("daemon:missing");
    // aya 的 connection 测过且 ok ⇒ 它那台不该再出现这一项
    expect(ofA.some((f) => f.startsWith("connection:"))).toBe(false);
  });

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

  it("本机的 daemon 格是「不需要」，不是「缺组件」", async () => {
    const sec = await mount([]);
    const local = sec.element.querySelector<HTMLElement>(".remote-machine-local")!;
    const cell = local.querySelector<HTMLElement>('[data-facet="daemon"]')!;
    expect(cell.classList.contains("remote-status-na")).toBe(true);
    expect(cell.title).toContain("不需要");
    // 对照：没测过的格子是 unknown，不是 na —— 两者混同会让用户以为本机缺组件。
    const conn = local.querySelector<HTMLElement>('[data-facet="connection"]')!;
    expect(conn.classList.contains("remote-status-unknown")).toBe(true);
    expect(conn.title).toContain("未测过");
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
    // S4b-3a 那轮我把 resume 命令插在 daemonless 之后，commit 却说它「紧邻装/卸 ccm」——
    // 实际隔着约 120 行。这条把它钉在**组件**那一半里，不让它漂回字段区。
    const p = fakePages();
    await mount([mkH("a", "1.1.1.1")], p.host);
    // 直接拿 host 收到的 parts 断言（比翻 DOM 稳）；[0] 是本机页
    const got = p.addedParts[1];
    expect(got, "远端机器页必须带 parts").toBeTruthy();
    const inConn = got!.connection.textContent ?? "";
    const inComp = got!.components.textContent ?? "";
    expect(inConn).toContain("主机 (host)");
    expect(inComp).toContain("resume 命令（这台机器）");
    expect(inComp).toContain("装 ccm 启动器");
    // 反向：resume 命令**不该**留在连接那半
    expect(inConn).not.toContain("resume 命令（这台机器）");
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

  it("★ SSH 不通时**不**给 daemon 那格下结论", async () => {
    // SSH 都没通，daemon 是「不知道」。记成 fail 等于替用户断言「远端没装 daemon」，
    // 而事实可能只是网络不通 —— 那条结论会一直挂在列表行上误导人。
    localStorage.clear();
    ipcReplies.set("test_remote_connection", {
      sshOk: false,
      daemonOk: false,
      fingerprint: null,
      endpoint: null,
      daemonHello: null,
    });
    const sec = await mount([mkH("a", "1.1.1.1")]);
    const btns = [...sec.element.querySelectorAll<HTMLButtonElement>("button")];
    const testBtn = btns.find((b) => b.textContent?.includes("测试连接"))!;
    testBtn.click();
    for (let i = 0; i < 10; i++) await new Promise((r) => setTimeout(r, 0));
    expect(ipcCalls).toContain("test_remote_connection");
    const st = readStatus("a");
    expect(st.connection?.kind).toBe("fail");
    expect(st.daemon).toBeUndefined();
    ipcReplies.clear();
  });

  it("SSH 通了才给 daemon 下结论（反向对照：别是恒不记）", async () => {
    localStorage.clear();
    ipcReplies.set("test_remote_connection", {
      sshOk: true,
      daemonOk: true,
      fingerprint: null,
      endpoint: null,
      daemonHello: null,
    });
    const sec = await mount([mkH("a", "1.1.1.1")]);
    const btns = [...sec.element.querySelectorAll<HTMLButtonElement>("button")];
    btns.find((b) => b.textContent?.includes("测试连接"))!.click();
    await new Promise((r) => setTimeout(r, 0));
    const st = readStatus("a");
    expect(st.connection?.kind).toBe("ok");
    expect(st.daemon?.kind).toBe("ok");
    ipcReplies.clear();
  });
});

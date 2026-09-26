/**
 * S5 / E56：「还差什么」的纯逻辑。
 *
 * 最要紧的一条不是「能不能列出缺件」，而是**「缺」与「不知道」不许混**——
 * 一个刚装好、什么都没点过的新用户，不该看到一屏红叉。
 */
import { describe, it, expect } from "vitest";
import {
  computeGaps,
  summarizeGaps,
  describeGap,
  GAP_HEAD,
  type Gap,
} from "../../src/settings/readiness";
import { LOCAL_MACHINE_KEY, type MachineStatus } from "../../src/settings/machine-status";

const T = 1_700_000_000_000;
const none = (): MachineStatus => ({});

function statusMap(m: Record<string, MachineStatus>) {
  return (o: string) => m[o] ?? {};
}

describe("computeGaps", () => {
  it("★ 全新用户（账本全空）→ 全部是 unknown，**一条 missing 都没有**", () => {
    const gaps = computeGaps({ origins: ["devbox"], statusOf: none });
    expect(gaps.length).toBeGreaterThan(0);
    expect(gaps.every((g) => g.kind === "unknown")).toBe(true);
    // 说「缺」就是替他下了一个他没做过的结论。
    expect(gaps.some((g) => g.kind === "missing")).toBe(false);
  });

  it("★ 测过且失败 → missing；测过且成功 → 根本不出现", () => {
    const gaps = computeGaps({
      origins: ["devbox"],
      statusOf: statusMap({
        devbox: {
          connection: { kind: "ok", at: T },
          backend: { kind: "fail", at: T },
        },
      }),
    });
    expect(gaps.find((g) => g.facet === "connection")).toBeUndefined();
    const d = gaps.find((g) => g.facet === "backend")!;
    expect(d.kind).toBe("missing");
    expect(d.severity).toBe("blocking");
  });

  /**
   * 🔴 `KR59D1` 第 ⑤ 处载体的**死值验落点**（`K-R59` 09-11，定框 `K35`）。
   *
   * # 这一条翻的是哪一面
   *
   * 它此前逐字叫「★ 本机的后端不算缺（不适用 ≠ 缺）」，断的是
   * `computeGaps({origins:[LOCAL_MACHINE_KEY]})` **不产出** `backend` 那一格 ——
   * 依据是 `notApplicable` 里 `facet === "backend"` 那一支，理由写在头注
   * 「本机不需要后端（`watcher.rs` 直读 jsonl）」。
   *
   * 那句话在 `C7`〔用 08-03〕之后就**不成立**了：`C7` 逐字「没有 daemonless，
   * 使用软件就要有后端 ⇒ **本机**也要有后端进程」，`local_backend.rs` 是它的产物。
   * ⇒ 本机后端**今天真的存在**，而这张「还差什么」的清单当时**永远不会告诉用户它没起来**。
   *
   * # 🔴 为什么这一条不许写成 `grep daemonless`
   *
   * **那一支里一个 `daemonless` 字样都没有。** `KR59D1` 的失效方向逐字写着：
   * 「只数 `grep daemonless == 0` —— 那只证明**名字**没了，不证明**那一档**没了」。
   * 本条断的是**行为**：喂一个 `backend` 那格空着的本机账本，那一格**必须出现在清单里**，
   * 而且必须是 `blocking`。把那一支塞回 `notApplicable`（哪怕换个名字写），本条立刻红。
   */
  it("🔴 KR59D1⑤：本机的 backend **算一格** —— 没起来这件事清单必须说得出来", () => {
    const gaps = computeGaps({ origins: [LOCAL_MACHINE_KEY], statusOf: none });
    const backend = gaps.filter((g) => g.facet === "backend");
    expect(
      backend.length,
      "本机的后端又被算成「不适用」了 —— `C7` 之后本机也有后端进程，" +
        "这一格空着必须说得出来（`KR59D1` 第 ⑤ 处载体）",
    ).toBe(1);
    expect(backend[0]?.origin).toBe(LOCAL_MACHINE_KEY);
    expect(backend[0]?.severity).toBe("blocking");
    // 反向自检：本机的**其它**项照常出现（不是整台被跳过了）
    expect(gaps.some((g) => g.facet === "ccm")).toBe(true);
    // 反向自检：账本里写了 `ok` 就该消失 —— 本条断的是「算不算一格」，不是「恒红一条」。
    const green = computeGaps({
      origins: [LOCAL_MACHINE_KEY],
      statusOf: () => ({ backend: { kind: "ok", at: T } }),
    });
    expect(green.some((g) => g.facet === "backend")).toBe(false);
  });

  it("★ 本机的「连接」不算缺 —— 本地 = 不走 ssh 的远端（INVARIANTS §40）", () => {
    // Phase G 逮到的真 bug：漏这一条，落地页顶上会永久挂着一条 **blocking** 的
    // 「本机 · 连接：未测过 —— 连不上这台机器，它上面的会话都看不到」。
    // 那句话对本机是假的，是清单里最重的一级，而且没有任何按钮能消掉它。
    const gaps = computeGaps({ origins: [LOCAL_MACHINE_KEY], statusOf: none });
    expect(gaps.some((g) => g.facet === "connection")).toBe(false);
    // ⚠ `K-R59` 订正：这里原来还断「本机不该有任何 blocking 条目」，
    //    依据是「backend 与 connection 是仅有的两条 blocking，而本机两条都不适用」。
    //    今天本机的 `backend` **算一格** ⇒ 那句话不再成立。
    //    换成**逐格点名**：本机剩下的 blocking 恰好只有后端一条。
    expect(gaps.filter((g) => g.severity === "blocking").map((g) => g.facet)).toEqual([
      "backend",
    ]);
    // 反向：远端的连接照常算数
    expect(
      computeGaps({ origins: ["devbox"], statusOf: none }).some(
        (g) => g.facet === "connection",
      ),
    ).toBe(true);
  });

  it("★ S9：Windows 本机的 ccm 不算缺（它的对应物是「终端集成」那块）", () => {
    const gaps = computeGaps({
      origins: [LOCAL_MACHINE_KEY],
      statusOf: none,
      hostOs: "windows",
    });
    // 不排掉的话，Windows 用户会在这张专为新用户做的清单上读到
    // 一条「本机缺 cc 命令」—— 而那条在他机器上无从补起。
    expect(gaps.some((g) => g.facet === "ccm")).toBe(false);
    // 反向自检：本机**其它**项照常出现（不是整台被跳过了）
    expect(gaps.some((g) => g.facet === "acctIso")).toBe(true);
  });

  it("★ S9：非 Windows 本机的 ccm 照常算数（bash 的 cc 在这些机器上是真能装的）", () => {
    for (const os of ["linux", "macos", "unknown"] as const) {
      const gaps = computeGaps({
        origins: [LOCAL_MACHINE_KEY],
        statusOf: none,
        hostOs: os,
      });
      expect(gaps.some((g) => g.facet === "ccm"), os).toBe(true);
    }
    // 不传 hostOs 也按「照常算数」处理（省略 ≠ Windows）
    expect(
      computeGaps({ origins: [LOCAL_MACHINE_KEY], statusOf: none }).some(
        (g) => g.facet === "ccm",
      ),
    ).toBe(true);
  });

  it("★ S9：OS 门只管本机 —— 远端机器的 ccm 在 Windows 上照常算数", () => {
    // 远端是不是 POSIX 跟 monitor 跑在哪没关系（远端一律走 ccm）。
    const gaps = computeGaps({
      origins: ["devbox"],
      statusOf: none,
      hostOs: "windows",
    });
    expect(gaps.some((g) => g.facet === "ccm")).toBe(true);
  });

  it("账本里显式记成 na 的也不算缺", () => {
    const gaps = computeGaps({
      origins: ["devbox"],
      statusOf: statusMap({ devbox: { backend: { kind: "na", at: T } } }),
    });
    expect(gaps.some((g) => g.facet === "backend")).toBe(false);
  });

  // 🔴 〔步 8 · 条 80 「不要管旧配置」〕**`KR59D3` 那条整条退役了。**
  //    它断的是「盘上还带着旧 `daemonless: true` 的主机要给一条指名的告知」，
  //    而那条告知（`NO_BACKEND_GAP_CODE` / `NO_BACKEND_CONSEQUENCE`）与它背后的
  //    `legacyNoBackend` 入参这一拍整块删了 ⇒ **它已经没有被测对象**。
  //    ⚠ 用例数 −1，逐条点名在本轮报告里。

  it("★ blocking 排在 optional 前面，且顺序稳定", () => {
    const gaps = computeGaps({ origins: ["devbox", "nano"], statusOf: none });
    const sev = gaps.map((g) => g.severity);
    expect(sev.indexOf("optional")).toBeGreaterThan(sev.lastIndexOf("blocking"));
    // 稳定：同样输入再算一次，逐项相同（不受账本键枚举顺序影响）
    expect(computeGaps({ origins: ["devbox", "nano"], statusOf: none })).toEqual(gaps);
  });

  it("全都 ok → 空列表（调用方据此整块不渲染）", () => {
    const all: MachineStatus = {
      connection: { kind: "ok", at: T },
      backend: { kind: "ok", at: T },
      ccm: { kind: "ok", at: T },
      acctIso: { kind: "ok", at: T },
      accounts: { kind: "ok", at: T },
    };
    expect(computeGaps({ origins: ["devbox"], statusOf: () => all })).toEqual([]);
  });
});

/**
 * `N-F2` `NF2D3`：**「全绿就整块不出现」那一支从死代码变成走得到。**
 *
 * 它此前是死代码，不是因为这个纯函数错了 —— 恰恰相反，`computeGaps` 一直就把本机
 * 算进去、`notApplicable` 当时也只排掉本机的 `backend` / `connection`。死的是**写点**：
 * 本机的 `acctIso` / `accounts` 全仓没有任何 `recordFacet` 生产者
 * ⇒ 恒 `unknown` ⇒ `summarizeGaps` 恒非 null。
 *
 * ⇒ 本族断的是**这个纯函数这一侧的地板**：给一本「本机全绿」的账本，它必须真的
 * 返回空、`summarizeGaps` 必须真的返回 `null`。写点那一侧由
 * `accounts-section.vitest.ts` 的 `N-F2` 那一族用**真的一次面板运行**接上。
 */
describe("N-F2 NF2D3：本机全绿 + 没有远端 ⇒ 那张清单该整块消失", () => {
  /**
   * 本机那条路上真会被写绿的格子。
   *
   * ⚠ `K-R59`（09-11）**从两格变成三格**：`backend` 那一格此前被 `notApplicable` 排掉
   * （理由「本机不需要后端」，`C7` 之后不成立），今天它算数了，写点是
   * `remote-section.ts::noteLocalBackend`（问一次本机那把手，`backend-section` 在同一个
   * 面板上早就在问同一个命令）。
   */
  const localGreen: MachineStatus = {
    backend: { kind: "ok", at: T },
    acctIso: { kind: "ok", at: T },
    accounts: { kind: "ok", at: T },
  };

  it("★ Windows 本机：适用格恰好是那三格，写绿之后 summarizeGaps 返回 null", () => {
    // 分母（`NF2D3` 的 acceptor 逐字要求，防「一台机器都没有」蒙混）：
    //   · 机器数 = 1，**不是空清单**；
    //   · 先断这台机在这个 OS 上的适用格集合非空、且恰好是我们要写绿的那几格。
    const origins = [LOCAL_MACHINE_KEY];
    expect(origins.length).toBe(1);
    const before = computeGaps({ origins, statusOf: none, hostOs: "windows" });
    expect(
      before.map((g) => g.facet),
      "适用格不是这三格 —— 那下面这条 null 就不是本件买来的",
    ).toEqual(["backend", "acctIso", "accounts"]);

    const after = computeGaps({ origins, statusOf: () => localGreen, hostOs: "windows" });
    expect(after).toEqual([]);
    expect(summarizeGaps(after)).toBeNull();
  });

  it("★ 先证会红：把那几格改回「没测过」⇒ 清单又出现，且写的是「还没测过」", () => {
    const gaps = computeGaps({
      origins: [LOCAL_MACHINE_KEY],
      statusOf: none, // = 本件之前的行为：那几格从来没人写
      hostOs: "windows",
    });
    expect(gaps.map((g) => `${g.facet}:${g.kind}`)).toEqual([
      "backend:unknown",
      "acctIso:unknown",
      "accounts:unknown",
    ]);
    const s = summarizeGaps(gaps);
    // 〔W5-VIS · 缺口三〕后端那一格是必需的，另两格可选 —— 摘要按轻重分开说。
    expect(s).toBe("必需：1 项还没测过；可选：2 项还没测过");
  });

  it("★ 诚实边界：非 Windows 本机还剩 `ccm` 一格 —— 本机的 ccm 至今没有任何写点", () => {
    // 这一条**不是**在断本件做完了，正相反：它把本件**没**买到的那一格钉在明处。
    // `machine-card` 那三格（connection / backend / ccm）的写点全都按远端 host key 记账
    //（`this.persistedKey ?? hostKey(this.collect())`）⇒ 全仓对 `LOCAL_MACHINE_KEY`
    // 的 ccm 写点是 **0 个**。于是在 Linux / macOS 上，本机那一栏就算这两格全绿，
    // 清单里仍会剩一条「本机 · ccm：未测过」。
    // ⇒ 「整块消失」今天只在 **Windows 本机 + 零远端** 这一格上真的走得到。
    const rest = computeGaps({
      origins: [LOCAL_MACHINE_KEY],
      statusOf: () => localGreen,
      hostOs: "linux",
    });
    expect(rest.map((g) => g.facet)).toEqual(["ccm"]);
    expect(summarizeGaps(rest)).not.toBeNull();
    // 把 ccm 也记上就空了 —— 剩下的确实只有它这一格，不是别的没补齐。
    expect(
      computeGaps({
        origins: [LOCAL_MACHINE_KEY],
        statusOf: () => ({ ...localGreen, ccm: { kind: "ok" as const, at: T } }),
        hostOs: "linux",
      }),
    ).toEqual([]);
  });
});

describe("summarizeGaps —— 措辞必须区分「缺」与「没测过」", () => {
  const mk = (kind: Gap["kind"]): Gap => ({
    origin: "devbox",
    facet: "ccm",
    kind,
    consequence: "x",
    severity: "optional",
  });

  it("两类都有时分开说", () => {
    expect(summarizeGaps([mk("missing"), mk("unknown"), mk("unknown")])).toBe(
      "可选：1 项确认缺，2 项还没测过",
    );
  });

  it("只有没测过时**不说「缺」**", () => {
    const s = summarizeGaps([mk("unknown"), mk("unknown")])!;
    expect(s).toBe("可选：2 项还没测过");
    expect(s).not.toContain("缺");
  });

  /**
   * 〔W5-VIS · `设计/15 §4.5` 缺口三〕逐字：「**摘要不分严重度** —— blocking 的后端与 optional 的 ccm
   * 在摘要那行读起来一模一样（信息在，只是摘要没说）」。四种组合逐格相等（两档各自的缺 / 没测过分开数，
   * 只有一档就只说那一档，必需那一档排在前面）。
   */
  it("★ W5-VIS 缺口三：摘要按轻重分开说（必需在前，只有一档就只说一档）", () => {
    const at = (severity: Gap["severity"], kind: Gap["kind"]): Gap => ({
      origin: "devbox",
      facet: severity === "blocking" ? "backend" : "ccm",
      kind,
      consequence: "x",
      severity,
    });
    const table: [Gap[], string][] = [
      [[at("blocking", "missing")], "必需：1 项确认缺"],
      [[at("optional", "unknown")], "可选：1 项还没测过"],
      [
        [at("optional", "unknown"), at("blocking", "unknown"), at("blocking", "missing")],
        "必需：1 项确认缺，1 项还没测过；可选：1 项还没测过",
      ],
      [
        [at("blocking", "unknown"), at("optional", "missing"), at("optional", "unknown")],
        "必需：1 项还没测过；可选：1 项确认缺，1 项还没测过",
      ],
    ];
    for (const [gaps, want] of table) expect(summarizeGaps(gaps)).toBe(want);
    // 正控：旧形（不分轻重）说不出这几格 —— 一档里全是必需时也不许读成「可选」。
    expect(summarizeGaps([at("blocking", "missing")])).not.toContain("可选");
  });

  it("空列表 → null", () => {
    expect(summarizeGaps([])).toBeNull();
  });
});

describe("describeGap", () => {
  it("本机显示成「本机」，并带上后果（不只是一个 ✗）", () => {
    const t = describeGap({
      origin: LOCAL_MACHINE_KEY,
      facet: "ccm",
      kind: "missing",
      consequence: "终端里没有 cc 命令",
      severity: "optional",
    });
    expect(t).toContain("本机");
    expect(t).toContain("缺");
    expect(t).toContain("终端里没有 cc 命令");
  });

  it("没测过的那条不写「缺」", () => {
    const t = describeGap({
      origin: "devbox",
      facet: "backend",
      kind: "unknown",
      consequence: "c",
      severity: "blocking",
    });
    expect(t).toContain("未测过");
    expect(t).not.toContain("缺");
  });

  // 🔴 〔`K-R65`〕这一对词今天**有第二个读者**（配置面审计那一页）⇒ 它必须只有一个住址。
  // 上面两条断的是字面「缺」/「未测过」；本条断的是**那两个字面真的来自 `GAP_HEAD`** ——
  // 把 `GAP_HEAD` 改掉而 `describeGap` 里又抄了一份，上面两条照绿，本条红。
  it("「缺」/「未测过」这两个字只有一个住址（GAP_HEAD）", () => {
    const mk = (kind: Gap["kind"]): Gap => ({
      origin: "devbox",
      facet: "backend",
      kind,
      consequence: "c",
      severity: "blocking",
    });
    // 反向自检：两个头词不一样（一样的话下面等于空真）
    expect(GAP_HEAD.missing).not.toBe(GAP_HEAD.unknown);
    for (const kind of ["missing", "unknown"] as const) {
      expect(describeGap(mk(kind))).toContain(`${GAP_HEAD[kind]} ——`);
    }
  });
});

/**
 * S5 / E56：「还差什么」的纯逻辑。
 *
 * 最要紧的一条不是「能不能列出缺件」，而是**「缺」与「不知道」不许混**——
 * 一个刚装好、什么都没点过的新用户，不该看到一屏红叉。
 */
import { describe, it, expect } from "vitest";
import { computeGaps, summarizeGaps, describeGap, type Gap } from "./readiness";
import { LOCAL_MACHINE_KEY, type MachineStatus } from "./machine-status";

const T = 1_700_000_000_000;
const none = (): MachineStatus => ({});

function statusMap(m: Record<string, MachineStatus>) {
  return (o: string) => m[o] ?? {};
}

describe("computeGaps", () => {
  it("★ 全新用户（账本全空）→ 全部是 unknown，**一条 missing 都没有**", () => {
    const gaps = computeGaps({ origins: ["aya"], statusOf: none });
    expect(gaps.length).toBeGreaterThan(0);
    expect(gaps.every((g) => g.kind === "unknown")).toBe(true);
    // 说「缺」就是替他下了一个他没做过的结论。
    expect(gaps.some((g) => g.kind === "missing")).toBe(false);
  });

  it("★ 测过且失败 → missing；测过且成功 → 根本不出现", () => {
    const gaps = computeGaps({
      origins: ["aya"],
      statusOf: statusMap({
        aya: {
          connection: { kind: "ok", at: T },
          daemon: { kind: "fail", at: T },
        },
      }),
    });
    expect(gaps.find((g) => g.facet === "connection")).toBeUndefined();
    const d = gaps.find((g) => g.facet === "daemon")!;
    expect(d.kind).toBe("missing");
    expect(d.severity).toBe("blocking");
  });

  it("★ 本机的 daemon 不算缺（不适用 ≠ 缺）", () => {
    const gaps = computeGaps({ origins: [LOCAL_MACHINE_KEY], statusOf: none });
    expect(gaps.some((g) => g.facet === "daemon")).toBe(false);
    // 反向自检：本机的**其它**项照常出现（不是整台被跳过了）
    expect(gaps.some((g) => g.facet === "ccm")).toBe(true);
  });

  it("★ 本机的「连接」不算缺 —— 本地 = 不走 ssh 的远端（INVARIANTS §40）", () => {
    // Phase G 逮到的真 bug：漏这一条，落地页顶上会永久挂着一条 **blocking** 的
    // 「本机 · 连接：未测过 —— 连不上这台机器，它上面的会话都看不到」。
    // 那句话对本机是假的，是清单里最重的一级，而且没有任何按钮能消掉它。
    const gaps = computeGaps({ origins: [LOCAL_MACHINE_KEY], statusOf: none });
    expect(gaps.some((g) => g.facet === "connection")).toBe(false);
    // 本机不该有任何 blocking 条目（daemon 与 connection 是仅有的两条 blocking）
    expect(gaps.some((g) => g.severity === "blocking")).toBe(false);
    // 反向：远端的连接照常算数
    expect(
      computeGaps({ origins: ["aya"], statusOf: none }).some(
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
      origins: ["aya"],
      statusOf: none,
      hostOs: "windows",
    });
    expect(gaps.some((g) => g.facet === "ccm")).toBe(true);
  });

  it("账本里显式记成 na 的也不算缺", () => {
    const gaps = computeGaps({
      origins: ["aya"],
      statusOf: statusMap({ aya: { daemon: { kind: "na", at: T } } }),
    });
    expect(gaps.some((g) => g.facet === "daemon")).toBe(false);
  });

  it("★ daemonless 的机器不该被说「缺 daemon」（那是用户显式选的降级）", () => {
    const gaps = computeGaps({
      origins: ["aya"],
      statusOf: none,
      isDaemonless: (o) => o === "aya",
    });
    expect(gaps.some((g) => g.facet === "daemon")).toBe(false);
    // 反向：不 daemonless 的机器仍然要说
    const g2 = computeGaps({ origins: ["aya"], statusOf: none });
    expect(g2.some((g) => g.facet === "daemon")).toBe(true);
  });

  it("★ blocking 排在 optional 前面，且顺序稳定", () => {
    const gaps = computeGaps({ origins: ["aya", "nano"], statusOf: none });
    const sev = gaps.map((g) => g.severity);
    expect(sev.indexOf("optional")).toBeGreaterThan(sev.lastIndexOf("blocking"));
    // 稳定：同样输入再算一次，逐项相同（不受账本键枚举顺序影响）
    expect(computeGaps({ origins: ["aya", "nano"], statusOf: none })).toEqual(gaps);
  });

  it("全都 ok → 空列表（调用方据此整块不渲染）", () => {
    const all: MachineStatus = {
      connection: { kind: "ok", at: T },
      daemon: { kind: "ok", at: T },
      ccm: { kind: "ok", at: T },
      acctIso: { kind: "ok", at: T },
      accounts: { kind: "ok", at: T },
    };
    expect(computeGaps({ origins: ["aya"], statusOf: () => all })).toEqual([]);
  });
});

/**
 * `N-F2` `NF2D3`：**「全绿就整块不出现」那一支从死代码变成走得到。**
 *
 * 它此前是死代码，不是因为这个纯函数错了 —— 恰恰相反，`computeGaps` 一直就把本机
 * 算进去、`notApplicable` 也一直只排掉本机的 `daemon` / `connection`。死的是**写点**：
 * 本机的 `acctIso` / `accounts` 全仓没有任何 `recordFacet` 生产者
 * ⇒ 恒 `unknown` ⇒ `summarizeGaps` 恒非 null。
 *
 * ⇒ 本族断的是**这个纯函数这一侧的地板**：给一本「本机全绿」的账本，它必须真的
 * 返回空、`summarizeGaps` 必须真的返回 `null`。写点那一侧由
 * `accounts-section.vitest.ts` 的 `N-F2` 那一族用**真的一次面板运行**接上。
 */
describe("N-F2 NF2D3：本机全绿 + 没有远端 ⇒ 那张清单该整块消失", () => {
  /** 本机那条路上真会被写绿的两格（`accounts-section.note()` 的两个 facet）。 */
  const localGreen: MachineStatus = {
    acctIso: { kind: "ok", at: T },
    accounts: { kind: "ok", at: T },
  };

  it("★ Windows 本机：适用格恰好是那两格，写绿之后 summarizeGaps 返回 null", () => {
    // 分母（`NF2D3` 的 acceptor 逐字要求，防「一台机器都没有」蒙混）：
    //   · 机器数 = 1，**不是空清单**；
    //   · 先断这台机在这个 OS 上的适用格集合非空、且恰好是我们要写绿的那两格。
    const origins = [LOCAL_MACHINE_KEY];
    expect(origins.length).toBe(1);
    const before = computeGaps({ origins, statusOf: none, hostOs: "windows" });
    expect(
      before.map((g) => g.facet),
      "适用格不是这两格 —— 那下面这条 null 就不是本件买来的",
    ).toEqual(["acctIso", "accounts"]);

    const after = computeGaps({ origins, statusOf: () => localGreen, hostOs: "windows" });
    expect(after).toEqual([]);
    expect(summarizeGaps(after)).toBeNull();
  });

  it("★ 先证会红：把那两格改回「没测过」⇒ 清单又出现，且写的是「还没测过」", () => {
    const gaps = computeGaps({
      origins: [LOCAL_MACHINE_KEY],
      statusOf: none, // = 本件之前的行为：那两格从来没人写
      hostOs: "windows",
    });
    expect(gaps.map((g) => `${g.facet}:${g.kind}`)).toEqual([
      "acctIso:unknown",
      "accounts:unknown",
    ]);
    const s = summarizeGaps(gaps);
    expect(s).toBe("2 项还没测过");
  });

  it("★ 诚实边界：非 Windows 本机还剩 `ccm` 一格 —— 本机的 ccm 至今没有任何写点", () => {
    // 这一条**不是**在断本件做完了，正相反：它把本件**没**买到的那一格钉在明处。
    // `machine-card` 那三格（connection / daemon / ccm）的写点全都按远端 host key 记账
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
    origin: "aya",
    facet: "ccm",
    kind,
    consequence: "x",
    severity: "optional",
  });

  it("两类都有时分开说", () => {
    expect(summarizeGaps([mk("missing"), mk("unknown"), mk("unknown")])).toBe(
      "1 项确认缺，2 项还没测过",
    );
  });

  it("只有没测过时**不说「缺」**", () => {
    const s = summarizeGaps([mk("unknown"), mk("unknown")])!;
    expect(s).toBe("2 项还没测过");
    expect(s).not.toContain("缺");
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
      origin: "aya",
      facet: "daemon",
      kind: "unknown",
      consequence: "c",
      severity: "blocking",
    });
    expect(t).toContain("未测过");
    expect(t).not.toContain("缺");
  });
});

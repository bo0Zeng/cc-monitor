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
  NO_BACKEND_GAP_CODE,
  NO_BACKEND_CONSEQUENCE,
  type Gap,
} from "./readiness";
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

  /**
   * 🔴 `KR59D1` 第 ⑤ 处载体的**死值验落点**（`K-R59` 09-11，定框 `K35`）。
   *
   * # 这一条翻的是哪一面
   *
   * 它此前逐字叫「★ 本机的 daemon 不算缺（不适用 ≠ 缺）」，断的是
   * `computeGaps({origins:[LOCAL_MACHINE_KEY]})` **不产出** `daemon` 那一格 ——
   * 依据是 `notApplicable` 里 `facet === "daemon"` 那一支，理由写在头注
   * 「本机不需要 daemon（`watcher.rs` 直读 jsonl）」。
   *
   * 那句话在 `C7`〔用 08-03〕之后就**不成立**了：`C7` 逐字「没有 daemonless，
   * 使用软件就要有后端 ⇒ **本机**也要有后端进程」，`local_backend.rs` 是它的产物。
   * ⇒ 本机后端**今天真的存在**，而这张「还差什么」的清单当时**永远不会告诉用户它没起来**。
   *
   * # 🔴 为什么这一条不许写成 `grep daemonless`
   *
   * **那一支里一个 `daemonless` 字样都没有。** `KR59D1` 的失效方向逐字写着：
   * 「只数 `grep daemonless == 0` —— 那只证明**名字**没了，不证明**那一档**没了」。
   * 本条断的是**行为**：喂一个 `daemon` 那格空着的本机账本，那一格**必须出现在清单里**，
   * 而且必须是 `blocking`。把那一支塞回 `notApplicable`（哪怕换个名字写），本条立刻红。
   */
  it("🔴 KR59D1⑤：本机的 daemon **算一格** —— 没起来这件事清单必须说得出来", () => {
    const gaps = computeGaps({ origins: [LOCAL_MACHINE_KEY], statusOf: none });
    const daemon = gaps.filter((g) => g.facet === "daemon");
    expect(
      daemon.length,
      "本机的 daemon 又被算成「不适用」了 —— `C7` 之后本机也有后端进程，" +
        "这一格空着必须说得出来（`KR59D1` 第 ⑤ 处载体）",
    ).toBe(1);
    expect(daemon[0]?.origin).toBe(LOCAL_MACHINE_KEY);
    expect(daemon[0]?.severity).toBe("blocking");
    // 反向自检：本机的**其它**项照常出现（不是整台被跳过了）
    expect(gaps.some((g) => g.facet === "ccm")).toBe(true);
    // 反向自检：账本里写了 `ok` 就该消失 —— 本条断的是「算不算一格」，不是「恒红一条」。
    const green = computeGaps({
      origins: [LOCAL_MACHINE_KEY],
      statusOf: () => ({ daemon: { kind: "ok", at: T } }),
    });
    expect(green.some((g) => g.facet === "daemon")).toBe(false);
  });

  it("★ 本机的「连接」不算缺 —— 本地 = 不走 ssh 的远端（INVARIANTS §40）", () => {
    // Phase G 逮到的真 bug：漏这一条，落地页顶上会永久挂着一条 **blocking** 的
    // 「本机 · 连接：未测过 —— 连不上这台机器，它上面的会话都看不到」。
    // 那句话对本机是假的，是清单里最重的一级，而且没有任何按钮能消掉它。
    const gaps = computeGaps({ origins: [LOCAL_MACHINE_KEY], statusOf: none });
    expect(gaps.some((g) => g.facet === "connection")).toBe(false);
    // ⚠ `K-R59` 订正：这里原来还断「本机不该有任何 blocking 条目」，
    //    依据是「daemon 与 connection 是仅有的两条 blocking，而本机两条都不适用」。
    //    今天本机的 `daemon` **算一格** ⇒ 那句话不再成立。
    //    换成**逐格点名**：本机剩下的 blocking 恰好只有 daemon 一条。
    expect(gaps.filter((g) => g.severity === "blocking").map((g) => g.facet)).toEqual([
      "daemon",
    ]);
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

  /**
   * 🔴 `KR59D3`：旧配置里那个 `true` **不许被静默吞掉**。
   *
   * 这一条此前逐字叫「★ daemonless 的机器不该被说「缺 daemon」（那是用户显式选的降级）」，
   * 断的是注入 `isDaemonless` 之后那台机器的 `daemon` 一格**消失**。
   * `K35` 把那一档删了 ⇒ 语义整个翻面：那台机器从此**要连后端**，
   * 而它连不上的时候用户该看见的是「你这台机器本来就是按不装后端配的，去装」，
   * **不是**一句通用的「没测过」。
   */
  it("🔴 KR59D3：盘上还带着旧「不装后端」开关的主机 ⇒ 一条**指名的**告知", () => {
    const gaps = computeGaps({
      origins: ["aya"],
      statusOf: none,
      legacyNoBackend: (o) => o === "aya",
    });
    const daemon = gaps.filter((g) => g.facet === "daemon");
    expect(daemon).toHaveLength(1);
    expect(daemon[0]?.code, "那条告知没有名字 —— 形状抄 `K-P2` 的唯一失败面").toBe(
      NO_BACKEND_GAP_CODE,
    );
    // **有名字还不够**：它得说清「为什么」+「下一步」，而且是 blocking。
    expect(daemon[0]?.consequence).toBe(NO_BACKEND_CONSEQUENCE);
    expect(daemon[0]?.severity).toBe("blocking");
    // 「确认没有」而不是「没测过」—— 盘上那份配置就是证据。
    expect(daemon[0]?.kind).toBe("missing");
    // ⚠ **它压过账本**：旧路径把这台机器的两格记成 `na`（「用户显式选的降级」），
    //   那正是要被撤掉的那句话；账本说 `na` 也照样告知。
    const over = computeGaps({
      origins: ["aya"],
      statusOf: () => ({ daemon: { kind: "na", at: T } }),
      legacyNoBackend: () => true,
    });
    expect(over.filter((g) => g.facet === "daemon")[0]?.code).toBe(NO_BACKEND_GAP_CODE);
    // 反向自检①：没有旧开关的机器**不带**这个名字（不是恒挂一条）。
    const plain = computeGaps({ origins: ["aya"], statusOf: none });
    expect(plain.some((g) => g.code === NO_BACKEND_GAP_CODE)).toBe(false);
    expect(plain.some((g) => g.facet === "daemon")).toBe(true);
    // 反向自检②：那条告知**看得见** —— 显示文案里含它的「下一步」。
    expect(describeGap(daemon[0]!)).toContain("装上后端");
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
 * 算进去、`notApplicable` 当时也只排掉本机的 `daemon` / `connection`。死的是**写点**：
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
   * ⚠ `K-R59`（09-11）**从两格变成三格**：`daemon` 那一格此前被 `notApplicable` 排掉
   * （理由「本机不需要 daemon」，`C7` 之后不成立），今天它算数了，写点是
   * `remote-section.ts::noteLocalBackend`（问一次本机那把手，`daemon-section` 在同一个
   * 面板上早就在问同一个命令）。
   */
  const localGreen: MachineStatus = {
    daemon: { kind: "ok", at: T },
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
    ).toEqual(["daemon", "acctIso", "accounts"]);

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
      "daemon:unknown",
      "acctIso:unknown",
      "accounts:unknown",
    ]);
    const s = summarizeGaps(gaps);
    expect(s).toBe("3 项还没测过");
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

/**
 * S1：远端配置的**局部合并**（`applyRemoteHostsPatch`）。
 *
 * 为什么这几条值得写：整表覆盖今天之所以不出事，是因为 `RemoteSection.collect()`
 * 恰好映射了**全部**卡片 —— **正确性来自 UI 的巧合，不是来自构造**。S2 把机器拆成
 * 一页一台之后，同样的保存动作会把其余机器静默删光。下面每一条钉的都是
 * 「即使调用方只提交一部分，也不会伤到别人」这个性质。
 *
 * 纯函数、不碰文件系统，故不需要 mock `config.ts`。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
vi.mock("./config", () => ({ loadConfig: vi.fn(), saveConfig: vi.fn() }));
import { loadConfig, saveConfig } from "./config";
import {
  applyRemoteHostsPatch,
  hostKey,
  patchRemoteConfig,
  pickResumeCommand,
  type RemoteConfig,
  type RemoteHostConfig,
} from "./remote-config";

function mk(over: Partial<RemoteHostConfig> = {}): RemoteHostConfig {
  return {
    label: "",
    host: "h",
    port: 22,
    user: "pi",
    keyPath: "",
    daemonPath: "/usr/local/bin/ccmd",
    hostKeyFingerprint: "",
    addresses: [],
    jump: "",
    daemonless: false,
    resumeCommand: "",
    ...over,
  };
}

const A = mk({ label: "alpha", host: "10.0.0.1", user: "ua", jump: "gw" });
const B = mk({ label: "beta", host: "10.0.0.2", user: "ub", daemonless: true });
const C = mk({ label: "", host: "10.0.0.3", user: "uc" }); // label 空 ⇒ key = host

const base: RemoteConfig = { enabled: true, hosts: [A, B, C] };

describe("hostKey", () => {
  it("label 非空取 label，否则取 host（与 findHostByOrigin 同口径）", () => {
    expect(hostKey(A)).toBe("alpha");
    expect(hostKey(C)).toBe("10.0.0.3");
    // 纯空白 label 不算数（否则 key 会是一串空格，和后端 origin 对不上）
    expect(hostKey(mk({ label: "   ", host: "x" }))).toBe("x");
  });
});

describe("applyRemoteHostsPatch", () => {
  it("★ 只提交一台时，其余机器**逐字段原样**留下", () => {
    const out = applyRemoteHostsPatch(base, {
      upsert: [{ key: "beta", value: { ...B, user: "changed" } }],
    });
    expect(out.hosts).toHaveLength(3);
    // 深比较，**不是只比长度** —— 只比长度的话「改坏了某台的字段」也能蒙混过关。
    expect(out.hosts[0]).toEqual(A);
    expect(out.hosts[2]).toEqual(C);
    expect(out.hosts[1]?.user).toBe("changed");
    // 顺带钉住：没被 patch 的那两条连**对象引用**都没换（证明真的没碰）。
    expect(out.hosts[0]).toBe(A);
    expect(out.hosts[2]).toBe(C);
  });

  it("★ 改 label（= 换 origin）是**改那一条**，不是新增 + 留孤儿", () => {
    const renamed = { ...A, label: "alpha-renamed" };
    const out = applyRemoteHostsPatch(base, {
      upsert: [{ key: "alpha", value: renamed }],
    });
    expect(out.hosts).toHaveLength(3);
    expect(out.hosts.map(hostKey)).toEqual(["alpha-renamed", "beta", "10.0.0.3"]);
    // 位置也保住（改名不该把机器甩到列表末尾）
    expect(out.hosts[0]).toEqual(renamed);
  });

  it("remove 按 origin 删，且只删点名的那台", () => {
    const out = applyRemoteHostsPatch(base, { remove: ["beta"] });
    expect(out.hosts.map(hostKey)).toEqual(["alpha", "10.0.0.3"]);
    expect(out.hosts[0]).toBe(A);
  });

  it("key=null ⇒ 追加新机器", () => {
    const d = mk({ label: "delta", host: "10.0.0.4" });
    const out = applyRemoteHostsPatch(base, { upsert: [{ key: null, value: d }] });
    expect(out.hosts.map(hostKey)).toEqual(["alpha", "beta", "10.0.0.3", "delta"]);
  });

  it("key 在盘上找不到 ⇒ 按新增处理（比静默丢弃安全）", () => {
    // 场景：这台在别处被删了/改名了，而本编辑器手上还是旧 key。
    // 丢弃的话用户这次编辑就白做了且**毫无提示**；追加至少东西还在。
    const ghost = mk({ label: "ghost", host: "10.0.0.9" });
    const out = applyRemoteHostsPatch(base, {
      upsert: [{ key: "已经不存在的-key", value: ghost }],
    });
    expect(out.hosts).toHaveLength(4);
    expect(hostKey(out.hosts[3]!)).toBe("ghost");
  });

  it("先 remove 后 upsert：删掉 A 同时新增一台也叫 A ⇒ 是替换，不是删掉新的那个", () => {
    const newA = mk({ label: "alpha", host: "192.168.1.1", user: "brand-new" });
    const out = applyRemoteHostsPatch(base, {
      remove: ["alpha"],
      upsert: [{ key: null, value: newA }],
    });
    expect(out.hosts).toHaveLength(3);
    const alpha = out.hosts.find((h) => hostKey(h) === "alpha");
    expect(alpha?.user).toBe("brand-new");
  });

  it("★ 两台 origin 相同：两条 upsert 各改各的，不互相踩", () => {
    // origin 重复本身是无效配置（整个系统拿 origin 当机器身份，见 BACKLOG E44），
    // 但无效配置不该表现为「静默吞掉用户的编辑」。没有「已消费下标」那一步的话，
    // 第二条 upsert 会再次命中第一条已被替换过的位置 ⇒ 第一条编辑凭空消失。
    const d1 = mk({ label: "dup", host: "1.1.1.1", user: "one" });
    const d2 = mk({ label: "dup", host: "2.2.2.2", user: "two" });
    const out = applyRemoteHostsPatch(
      { enabled: true, hosts: [d1, d2] },
      {
        upsert: [
          { key: "dup", value: { ...d1, user: "one-edited" } },
          { key: "dup", value: { ...d2, user: "two-edited" } },
        ],
      },
    );
    expect(out.hosts).toHaveLength(2);
    expect(out.hosts.map((h) => h.user)).toEqual(["one-edited", "two-edited"]);
  });

  it("enabled 缺省 = 不动；给了就改", () => {
    expect(applyRemoteHostsPatch(base, {}).enabled).toBe(true);
    expect(applyRemoteHostsPatch(base, { enabled: false }).enabled).toBe(false);
    expect(
      applyRemoteHostsPatch({ enabled: false, hosts: [] }, { enabled: true })
        .enabled,
    ).toBe(true);
  });

  it("空 patch 不改变任何东西（幂等基线）", () => {
    const out = applyRemoteHostsPatch(base, {});
    expect(out).toEqual(base);
  });

  it("不就地改入参（调用方手上那份 original 还要用来算下一次 diff）", () => {
    const snapshot = JSON.parse(JSON.stringify(base));
    applyRemoteHostsPatch(base, {
      remove: ["beta"],
      upsert: [{ key: "alpha", value: { ...A, user: "x" } }],
    });
    expect(base).toEqual(snapshot);
  });
});

describe("S1 patchRemoteConfig（走完整 read → 合并 → 序列化 → 落盘）", () => {
  beforeEach(() => vi.resetAllMocks());

  it("★ 只 upsert 一台：其余机器与 config.json 里的**无关顶层键**都原样留下", () => {
    // 纯函数那几条钉的是合并逻辑；这一条钉的是**穿过序列化那一层之后**也没丢东西
    //（`serializeHost` 按字段清单挑字段，漏一个就是静默丢失——这个 bug 类已经咬过两次）。
    vi.mocked(loadConfig).mockResolvedValue({
      // 无关顶层键：writeRemoteConfig 承诺「不动其他字段」，这里把承诺钉住。
      theme: "dark",
      someOtherSection: { a: 1 },
      remote: { enabled: true, hosts: [A, B, C] },
    } as unknown as Awaited<ReturnType<typeof loadConfig>>);

    return patchRemoteConfig({
      upsert: [{ key: "beta", value: { ...B, user: "changed" } }],
    }).then(() => {
      expect(saveConfig).toHaveBeenCalledTimes(1);
      const written = vi.mocked(saveConfig).mock.calls[0]![0] as Record<
        string,
        unknown
      >;
      expect(written.theme).toBe("dark");
      expect(written.someOtherSection).toEqual({ a: 1 });
      const remote = written.remote as RemoteConfig;
      expect(remote.hosts).toHaveLength(3);
      // 深比较：A / C 的**每个字段**都还在（不是只比条数）。
      expect(remote.hosts[0]).toEqual(A);
      // C 的 label 原本是空串，read-modify-write 之后被固化成 host —— **既有行为**，
      // 不是本功能引入的：`coerceHost` 读的时候就把空 label 回退成 host
      //（注释自陈是为了与 Rust `origin_label` 一致），写回去自然带着它。
      // origin 两种形态都算出同一个值（`label.trim() || host`）⇒ 功能等价；
      // 但盘上确实会多出一个用户没填过的 label，这里把这个事实钉住而不是假装没有。
      expect(remote.hosts[2]).toEqual({ ...C, label: C.host });
      expect(remote.hosts[1]?.user).toBe("changed");
      // 被改的那台其余字段也不能丢（`daemonless: true` 是 B 独有的非默认值）。
      expect(remote.hosts[1]?.daemonless).toBe(true);
    });
  });

  it("落盘的每台机器都带齐 RemoteHostConfig 的全部字段（序列化不丢字段）", () => {
    vi.mocked(loadConfig).mockResolvedValue({
      remote: { enabled: true, hosts: [A] },
    } as unknown as Awaited<ReturnType<typeof loadConfig>>);
    return patchRemoteConfig({}).then(() => {
      const written = vi.mocked(saveConfig).mock.calls[0]![0] as Record<
        string,
        unknown
      >;
      const h = (written.remote as RemoteConfig).hosts[0]!;
      // 键集合与类型逐一对齐——用 A 自己的键当期望，A 是按 RemoteHostConfig 造的。
      expect(Object.keys(h).sort()).toEqual(Object.keys(A).sort());
    });
  });
});

describe("S4b-3 pickResumeCommand —— per-machine 优先，全局兜底", () => {
  it("这台机器填了就用它的", () => {
    expect(pickResumeCommand(mk({ resumeCommand: "ccm resume" }), "claude -r")).toBe(
      "ccm resume",
    );
  });

  it("★ 没填 / 只填了空白 / 这台机器压根查不到 → 一律回退全局默认", () => {
    // 这是「不做数据迁移」的落点：没填过的机器行为**一字不变**。
    expect(pickResumeCommand(mk({ resumeCommand: "" }), "claude -r")).toBe("claude -r");
    expect(pickResumeCommand(mk({ resumeCommand: "   " }), "claude -r")).toBe("claude -r");
    expect(pickResumeCommand(null, "claude -r")).toBe("claude -r");
  });

  it("per-machine 值两端空白会被 trim（用户手滑不该产出带空格的命令）", () => {
    expect(pickResumeCommand(mk({ resumeCommand: "  ccm resume  " }), "x")).toBe(
      "ccm resume",
    );
  });

  it("★ 两台机器各用各的（这正是全局单值表达不出来的那件事）", () => {
    // A 机装了 ccm、B 机没装 —— 全局单值时这两台只能共用一条命令。
    const a = mk({ label: "aya", resumeCommand: "ccm resume" });
    const b = mk({ label: "nano", resumeCommand: "" });
    expect(pickResumeCommand(a, "claude -r")).toBe("ccm resume");
    expect(pickResumeCommand(b, "claude -r")).toBe("claude -r");
  });
});

describe("S1：整表覆盖那条路必须**不可达**", () => {
  it("writeRemoteConfig 不得被 export", () => {
    // 这是 S1 的核心安全性质：局部合并再对，只要还有一个导出的整表覆盖入口，
    // S2 拆页时随手一调就会静默删机器。不导出 ⇒ 类型层面不可达，不靠人记着别用。
    const src = readFileSync(resolve(__dirname, "remote-config.ts"), "utf8");
    // 反向自检：函数确实还在这个文件里（不是因为改名了才"没导出"）。
    expect(src).toContain("async function writeRemoteConfig");
    expect(src).not.toContain("export async function writeRemoteConfig");
    expect(src).not.toMatch(/export\s*\{[^}]*\bwriteRemoteConfig\b/);
  });
});

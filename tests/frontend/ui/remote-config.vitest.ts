/**
 * S1：远端配置的**局部合并**（〔FIX2 续〕今天全按键认元素：`remoteHostsEdits`）。
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
// 〔CFG1〕config 写只交补丁；替身把补丁应用到 `loadConfig` 摆的那份上，写完的整份交 `fakeCfg.saved`。
vi.mock("../../../src/frontend/ui/config", async (orig) => (await import("./config-patch-fake")).mockedConfigModule(orig));
import { loadConfig } from "../../../src/frontend/ui/config";
import { fakeCfg } from "./config-patch-fake";
const saveConfig = fakeCfg.saved;
import { srcDirOf } from "../../test-support/repo-root";
import {
  hostKey,
  patchRemoteConfig,
  pickResumeCommand,
  readRemoteConfig,
  REMOTE_CONFIG_UNRECOGNIZED,
  type RemoteConfig,
  type RemoteHostConfig,
} from "../../../src/frontend/ui/remote-config";

function mk(over: Partial<RemoteHostConfig> = {}): RemoteHostConfig {
  return {
    label: "",
    host: "h",
    port: 22,
    user: "pi",
    keyPath: "",
    hostKeyFingerprint: "",
    addresses: [],
    jump: "",
    resumeCommand: "",
    ...over,
  };
}

const A = mk({ label: "alpha", host: "10.0.0.1", user: "ua", jump: "gw" });
const B = mk({ label: "beta", host: "10.0.0.2", user: "ub", jump: "gw2" });
const C = mk({ label: "", host: "10.0.0.3", user: "uc" }); // label 空 ⇒ key = host


describe("hostKey", () => {
  it("label 非空取 label，否则取 host（与 findHostByOrigin 同口径）", () => {
    expect(hostKey(A)).toBe("alpha");
    expect(hostKey(C)).toBe("10.0.0.3");
    // 纯空白 label 不算数（否则 key 会是一串空格，和后端 origin 对不上）
    expect(hostKey(mk({ label: "   ", host: "x" }))).toBe("x");
  });
});

// 〔FIX2 续 · `设计/99 §2 ㊶`〕设置页增删机器也按键认元素：「固化那一写与设置页同写 `remote.hosts` 有毫秒级丢更新窗口」。
//   假盘与 Rust 写口跑同一份金样（`config-patch-fake.vitest.ts`）⇒ 这里看到的盘上终态就是 Rust 会落的那份。
describe("〔FIX2 续 · ㊶〕增 / 删一台 ⇒ insertin / removein，不整段写 remote", () => {
  beforeEach(() => vi.resetAllMocks());
  const pinnedB = { ...B, hostKeyFingerprint: "SHA256:pinned" }; // 加载之后后端刚固化的
  const disk = (hosts: RemoteHostConfig[]) =>
    vi.mocked(loadConfig).mockResolvedValue({
      theme: "dark",
      remote: { enabled: true, hosts },
    } as unknown as Awaited<ReturnType<typeof loadConfig>>);
  const lastEdits = () => vi.mocked(fakeCfg.patches).mock.calls.at(-1)![0] as { op: string }[];
  const lastHosts = () => (vi.mocked(saveConfig).mock.calls.at(-1)![0] as { remote: RemoteConfig }).remote.hosts;

  it("★ 增一台 ⇒ 恰好一条 insertin 追加到末尾（字段全带）；别的机器连刚固化的指纹一格不动", async () => {
    disk([A, pinnedB, C]);
    const d = mk({ label: "delta", host: "10.0.0.4" });
    await patchRemoteConfig({ upsert: [{ key: null, value: d }] });
    expect(lastEdits().map((e) => e.op)).toEqual(["insertin"]);
    expect(lastHosts()).toEqual([A, pinnedB, C, d]);
  });

  it("★ 删一台 ⇒ 恰好一条 removein；别的机器连刚固化的指纹一格不动", async () => {
    disk([A, pinnedB, C]);
    await patchRemoteConfig({ remove: ["alpha"] });
    expect(lastEdits().map((e) => e.op)).toEqual(["removein"]);
    expect(lastHosts()).toEqual([pinnedB, C]);
  });

  it("删掉 A、同时新增一台也叫 A ⇒ 先删后插，是替换", async () => {
    disk([A, pinnedB]);
    const newA = mk({ label: "alpha", host: "192.168.1.1", user: "brand-new" });
    await patchRemoteConfig({ remove: ["alpha"], upsert: [{ key: null, value: newA }] });
    expect(lastEdits().map((e) => e.op)).toEqual(["removein", "insertin"]);
    expect(lastHosts()).toEqual([pinnedB, newA]);
  });

  it("★ 增的那个 origin 盘上已有 / 删的那台盘上没了 / 那个 origin 不止一台 ⇒ 整批拒，盘上不动", async () => {
    disk([A, pinnedB]);
    await expect(patchRemoteConfig({ upsert: [{ key: null, value: { ...B, user: "x" } }] })).rejects.toThrow();
    await expect(patchRemoteConfig({ enabled: false, remove: ["gone"] })).rejects.toThrow();
    const dup = mk({ label: "dup", host: "1.1.1.1" });
    disk([dup, { ...dup, host: "2.2.2.2" }]);
    await expect(patchRemoteConfig({ upsert: [{ key: "dup", value: { ...dup, user: "x" } }] })).rejects.toThrow();
    expect(saveConfig).not.toHaveBeenCalled();
  });

  it("没动任何东西 ⇒ 一条补丁都不交", async () => {
    disk([A]);
    await patchRemoteConfig({ upsert: [{ key: "alpha", was: A, value: A }] });
    await patchRemoteConfig({});
    expect(fakeCfg.patches).not.toHaveBeenCalled();
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
  it("〔FIX2 续 · ㊶〕整段写 `remote` 的补丁在 remote-config.ts 里零处（增删改全按键认元素）", () => {
    // 〔CFG1〕从前叫 `writeRemoteConfig`（读整份 → 换 `remote` → 整份写），后来只出一条 `set ["remote"]` 补丁。 〔散文墓碑〕
    const whole = /setAt\(\s*\[\s*"remote"\s*\]\s*,|patchConfigFrom\(/;
    expect(whole.test('setAt(["remote"], {})'), "正控：认得出整段写那一形").toBe(true);
    expect(whole.test('setAt(["remote", "enabled"], true)'), "反控：写 enabled 那一格不是整段").toBe(false);
    const src = readFileSync(resolve(srcDirOf(__dirname), "remote-config.ts"), "utf8").replace(/\/\/.*$|\/\*[\s\S]*?\*\//gm, "");
    expect(src).not.toMatch(whole);
  });

  it("〔CFG1〕读盘失败 ⇒ `patchRemoteConfig` 抛、一个字节不写（从前读失败回空表，再把空表写回去 ⇒ 机器全没）", async () => {
    vi.resetAllMocks();
    // 第一次读失败、之后读得到（瞬时失败）：旧写法第一次那一读被吞成空表，随后照样写 ⇒ B 被删掉。
    vi.mocked(loadConfig)
      .mockRejectedValueOnce(new Error("盘坏了"))
      .mockResolvedValue({ remote: { enabled: true, hosts: [B] } } as unknown as Awaited<
        ReturnType<typeof loadConfig>
      >);
    await expect(patchRemoteConfig({ upsert: [{ key: null, value: A }] })).rejects.toThrow("盘坏了");
    expect(saveConfig).not.toHaveBeenCalled();
  });
});

/**
 * 〔S5 · 第四波〕要求住址：`调研/设计/99 §1` V41「不为旧配置留兼容」· D4（不许把认不出静默当成空）。
 *
 * `remote` 段没有 `hosts` 列表（旧的单台写法 / `hosts` 写成别的类型 / 只有 `enabled`）⇒ **认不出**：
 * 一台都不给（不猜那是哪台）、并带上那一句。对照：`hosts: []` 是合法的零台、没有 `remote` 段是「没配」，两者都不带那一句。
 * Rust 那一侧同一个判准：`tests/frontend/shell/lib_remote_config_tests.rs::a_remote_section_without_a_hosts_array_is_refused_not_emptied`。
 */
describe("〔S5 · V41〕remote 段认不出", () => {
  beforeEach(() => vi.resetAllMocks());

  it("★ 没有 hosts 列表 ⇒ 一台都不给，并说认不出", async () => {
    for (const remote of [
      { enabled: true, host: "pi.local", user: "pi", backendPath: "/x" },
      { enabled: true, hosts: { host: "pi.local" } },
      { enabled: true },
    ]) {
      vi.mocked(loadConfig).mockResolvedValue({ remote } as unknown as Awaited<
        ReturnType<typeof loadConfig>
      >);
      const got = await readRemoteConfig();
      expect(got.hosts, JSON.stringify(remote)).toEqual([]);
      expect(got.unrecognized, JSON.stringify(remote)).toBe(REMOTE_CONFIG_UNRECOGNIZED);
    }
    expect(REMOTE_CONFIG_UNRECOGNIZED.startsWith("认不出远端配置")).toBe(true);
  });

  it("对照：hosts: [] 与没有 remote 段都不带那一句", async () => {
    for (const cfg of [{ remote: { enabled: true, hosts: [] } }, {}]) {
      vi.mocked(loadConfig).mockResolvedValue(
        cfg as unknown as Awaited<ReturnType<typeof loadConfig>>,
      );
      const got = await readRemoteConfig();
      expect(got.hosts).toEqual([]);
      expect(got.unrecognized, JSON.stringify(cfg)).toBeUndefined();
    }
  });
});

// 〔FIX · `设计/99 §2 ㊶` 第一问〕守的要求（逐字）：「固化那一写与设置页同写 `remote.hosts` 有毫秒级丢更新窗口」。
describe("〔FIX · ㊶〕设置页改一台 ⇒ 按格 setin，不整台盖", () => {
  beforeEach(() => vi.resetAllMocks());

  it("★ 加载之后后端刚固化了指纹、设置页改了别的格 ⇒ 指纹留着、只动那几格；改名先写 label；那台没了 ⇒ 整批拒", async () => {
    const pinned = { ...A, hostKeyFingerprint: "SHA256:pinned" };
    vi.mocked(loadConfig).mockResolvedValue({
      theme: "dark",
      remote: { enabled: true, hosts: [pinned, B, C] },
    } as unknown as Awaited<ReturnType<typeof loadConfig>>);
    // 表单加载时 A 还没有指纹（`was`），用户改了 user 与 label。
    await patchRemoteConfig({
      enabled: true,
      upsert: [{ key: "alpha", was: A, value: { ...A, user: "new", label: "alpha2" } }],
    });
    const edits = vi.mocked(fakeCfg.patches).mock.calls.at(-1)![0] as { op: string; field?: string }[];
    expect(edits.map((e) => (e.op === "setin" ? e.field : e.op))).toEqual(["set", "label", "user"]);
    const hosts = (vi.mocked(saveConfig).mock.calls.at(-1)![0] as { remote: RemoteConfig }).remote.hosts;
    expect(hosts[0]).toEqual({ ...pinned, user: "new", label: "alpha2" });
    expect(hosts.slice(1)).toEqual([B, C]);
    // 空 label 的那台改 host：先把 label 写成表单值（origin 与整台写那一形相同），host 排最后。
    const c = { ...C, label: C.host };
    await patchRemoteConfig({ upsert: [{ key: C.host, was: c, value: { ...c, host: "10.0.0.9" } }] });
    const e2 = vi.mocked(fakeCfg.patches).mock.calls.at(-1)![0] as { field?: string }[];
    expect(e2.map((e) => e.field)).toEqual(["label", "host"]);
    // 盘上那台已被改名 / 删掉 ⇒ 整批拒（不再「找不到就当新增」）。
    await expect(
      patchRemoteConfig({ upsert: [{ key: "gone", was: { ...B, label: "gone" }, value: { ...B, label: "gone", user: "x" } }] }),
    ).rejects.toThrow();
  });
});

/**
 * S1：远端配置的**局部合并**（今天全按键认元素：`remoteHostsEdits`）。
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
// config 写只交补丁；替身把补丁应用到 `loadConfig` 摆的那份上，写完的整份交 `fakeCfg.saved`。
vi.mock("../../../src/frontend/ui/config", async (orig) => (await import("./config-patch-fake")).mockedConfigModule(orig));
// 机器表住本机后端：`machines-read` / `-try` / `-set` 的替身把表摆在同一块假盘的 `remote.hosts` 里。
vi.mock("../../../src/frontend/ui/ipc/chan-caller", async (orig) => (await import("./config-patch-fake")).mockedChanCaller(orig));
import { loadConfig } from "../../../src/frontend/ui/config";
import { fakeCfg } from "./config-patch-fake";
const saveConfig = fakeCfg.saved;
import { srcDirOf } from "../../test-support/repo-root";
import {
  hostKey,
  patchRemoteConfig,
  remoteConfigOf,
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
    addresses: [],
    jump: "",
    connect: true,
    ...over,
  };
}

const A = mk({ label: "alpha", host: "192.0.2.1", user: "ua", jump: "gw" });
const B = mk({ label: "beta", host: "192.0.2.2", user: "ub", jump: "gw2" });
const C = mk({ label: "", host: "192.0.2.3", user: "uc" }); // label 空 ⇒ key = host


describe("hostKey", () => {
  it("label 非空取 label，否则取 host（与 findHostByOrigin 同口径）", () => {
    expect(hostKey(A)).toBe("alpha");
    expect(hostKey(C)).toBe("192.0.2.3");
    // 纯空白 label 不算数（否则 key 会是一串空格，和后端 origin 对不上）
    expect(hostKey(mk({ label: "   ", host: "x" }))).toBe("x");
  });
});

// 设置页增删机器也按键认元素：「固化那一写与设置页同写 `remote.hosts` 有毫秒级丢更新窗口」。
//   假盘与 Rust 写口跑同一份金样（`config-patch-fake.vitest.ts`）⇒ 这里看到的盘上终态就是 Rust 会落的那份。
describe("〔FIX2 续 · ㊶〕增 / 删一台 ⇒ insertin / removein，不整段写 remote", () => {
  beforeEach(() => vi.resetAllMocks());
  const pinnedB = { ...B, keyPath: "/k/disk" }; // 加载之后别处刚改的
  const disk = (hosts: RemoteHostConfig[]) =>
    vi.mocked(loadConfig).mockResolvedValue({
      theme: "dark",
      remote: { enabled: true, hosts },
    } as unknown as Awaited<ReturnType<typeof loadConfig>>);
  const lastEdits = () => vi.mocked(fakeCfg.patches).mock.calls.at(-1)![0] as { op: string }[];
  const lastHosts = () => (vi.mocked(saveConfig).mock.calls.at(-1)![0] as { remote: RemoteConfig }).remote.hosts;

  it("★ 增一台 ⇒ 恰好一条 insertin 追加到末尾（字段全带）；别的机器连别处刚改的那一格都不动", async () => {
    disk([A, pinnedB, C]);
    const d = mk({ label: "delta", host: "192.0.2.4" });
    await patchRemoteConfig({ upsert: [{ key: null, value: d }] });
    expect(lastEdits().map((e) => e.op)).toEqual(["insertin"]);
    expect(lastHosts()).toEqual([A, pinnedB, C, d]);
  });

  it("★ 删一台 ⇒ 恰好一条 removein；别的机器连别处刚改的那一格都不动", async () => {
    disk([A, pinnedB, C]);
    await patchRemoteConfig({ remove: ["alpha"] });
    expect(lastEdits().map((e) => e.op)).toEqual(["removein"]);
    expect(lastHosts()).toEqual([pinnedB, C]);
  });

  it("删掉 A、同时新增一台也叫 A ⇒ 先删后插，是替换", async () => {
    disk([A, pinnedB]);
    const newA = mk({ label: "alpha", host: "203.0.113.1", user: "brand-new" });
    await patchRemoteConfig({ remove: ["alpha"], upsert: [{ key: null, value: newA }] });
    expect(lastEdits().map((e) => e.op)).toEqual(["removein", "insertin"]);
    expect(lastHosts()).toEqual([pinnedB, newA]);
  });

  it("★ 增的那个 origin 盘上已有 / 删的那台盘上没了 / 那个 origin 不止一台 ⇒ 整批拒，盘上不动", async () => {
    disk([A, pinnedB]);
    await expect(patchRemoteConfig({ upsert: [{ key: null, value: { ...B, user: "x" } }] })).rejects.toThrow();
    await expect(patchRemoteConfig({ remove: ["gone"] })).rejects.toThrow();
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

describe("S1：整表覆盖那条路必须**不可达**", () => {
  it("〔FIX2 续 · ㊶〕整段写 `remote` 的补丁在 remote-config.ts 里零处（增删改全按键认元素）", () => {
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
 * D4（不许把认不出静默当成空）：本机后端那份机器表读不懂 ⇒ 一台都不给（不猜那是哪台）、带上后端写好的那一句；
 * 没有那份 / 零台 ⇒ 都不带那一句。读不懂的判准在后端（`machines_tests::an_unreadable_table_is_reported_and_never_overwritten`）。
 */
describe("机器表读不懂", () => {
  it("★ unreadable ⇒ 一台都不给，并照抄后端那一句；absent / ok 零台 ⇒ 不带", () => {
    const got = remoteConfigOf({ state: "unreadable", reason: "读不懂那一句", detail: null, path: null, hosts: [], hostKeys: {} });
    expect(got).toEqual({ hosts: [], unrecognized: "读不懂那一句" });
    for (const state of ["absent", "ok"] as const) {
      expect(remoteConfigOf({ state, reason: null, detail: null, path: null, hosts: [], hostKeys: { gpu: ["SHA256:x"] } })).toEqual({
        hosts: [],
        hostKeys: { gpu: ["SHA256:x"] },
      });
    }
  });
});

// 〔第一问〕守的要求（逐字）：「固化那一写与设置页同写 `remote.hosts` 有毫秒级丢更新窗口」。
describe("〔FIX · ㊶〕设置页改一台 ⇒ 按格 setin，不整台盖", () => {
  beforeEach(() => vi.resetAllMocks());

  it("★ 加载之后别处改了一格、设置页改了别的格 ⇒ 那一格留着、只动那几格；改名先写 label；那台没了 ⇒ 整批拒", async () => {
    const pinned = { ...A, keyPath: "/k/disk" };
    vi.mocked(loadConfig).mockResolvedValue({
      theme: "dark",
      remote: { enabled: true, hosts: [pinned, B, C] },
    } as unknown as Awaited<ReturnType<typeof loadConfig>>);
    // 表单加载时 A 还是旧的钥匙路径（`was`），用户改了 user 与 label。
    await patchRemoteConfig({
      upsert: [{ key: "alpha", was: A, value: { ...A, user: "new", label: "alpha2" } }],
    });
    const edits = vi.mocked(fakeCfg.patches).mock.calls.at(-1)![0] as { op: string; field?: string }[];
    expect(edits.map((e) => (e.op === "setin" ? e.field : e.op))).toEqual(["label", "user"]);
    const hosts = (vi.mocked(saveConfig).mock.calls.at(-1)![0] as { remote: RemoteConfig }).remote.hosts;
    expect(hosts[0]).toEqual({ ...pinned, user: "new", label: "alpha2" });
    expect(hosts.slice(1)).toEqual([B, C]);
    // 空 label 的那台改 host：先把 label 写成表单值（origin 与整台写那一形相同），host 排最后。
    const c = { ...C, label: C.host };
    await patchRemoteConfig({ upsert: [{ key: C.host, was: c, value: { ...c, host: "192.0.2.9" } }] });
    const e2 = vi.mocked(fakeCfg.patches).mock.calls.at(-1)![0] as { field?: string }[];
    expect(e2.map((e) => e.field)).toEqual(["label", "host"]);
    // 盘上那台已被改名 / 删掉 ⇒ 整批拒（不再「找不到就当新增」）。
    await expect(
      patchRemoteConfig({ upsert: [{ key: "gone", was: { ...B, label: "gone" }, value: { ...B, label: "gone", user: "x" } }] }),
    ).rejects.toThrow();
  });
});

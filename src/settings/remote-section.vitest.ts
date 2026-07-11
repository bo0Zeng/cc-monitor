// F43：指纹重置按钮显隐纯逻辑。remote-section 的 DOM 主体重(拉整卡),这里只钉住
// 「有固化指纹才显示重置按钮」这条判定,防未来误改成空指纹也显示(重置无意义)。
import { describe, it, expect, vi } from "vitest";
// F56：writeRemoteConfig/readRemoteConfig 走 config.ts；mock 掉以测 jump write→read 往返。
vi.mock("../config", () => ({ loadConfig: vi.fn(), saveConfig: vi.fn() }));
import { loadConfig, saveConfig } from "../config";
import {
  shouldShowResetFingerprint,
  parseAddressLines,
  describeStage,
  findHostByOrigin,
  writeRemoteConfig,
  readRemoteConfig,
} from "./remote-section";
import type { RemoteHostConfig } from "./remote-section";

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
  });

  it("jump 写入 config 并读回不丢", async () => {
    vi.mocked(loadConfig).mockResolvedValue({});
    let saved: Record<string, unknown> = {};
    vi.mocked(saveConfig).mockImplementation(async (c: unknown) => {
      saved = c as Record<string, unknown>;
    });
    await writeRemoteConfig({ enabled: true, hosts: [host("bastion")] });
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
    await writeRemoteConfig({ enabled: true, hosts: [host("")] });
    vi.mocked(loadConfig).mockResolvedValue(saved);
    const back = await readRemoteConfig();
    expect(back.hosts[0].jump).toBe("");
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
  });

  it("daemonless=true 写入 config 并读回不丢", async () => {
    vi.mocked(loadConfig).mockResolvedValue({});
    let saved: Record<string, unknown> = {};
    vi.mocked(saveConfig).mockImplementation(async (c: unknown) => {
      saved = c as Record<string, unknown>;
    });
    await writeRemoteConfig({ enabled: true, hosts: [host(true)] });
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

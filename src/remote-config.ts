// F12：远端配置**数据层**（从 settings/remote-section.ts 抽出，治分层倒挂——数据层原住在 1801 行
// UI 模块里、被 tabs/account-chip/cards/main/port-forward 等非 UI 模块依赖）。本模块**纯数据**：
// config.json `remote` 段的类型 + 读写 CRUD + 反查/筛选纯函数，无 DOM、无 UI 依赖。行为与抽出前逐字节等价。
import { loadConfig, saveConfig } from "./config";

/**
 * 单台远端机器配置（config.json `remote.hosts[]` 的元素）。**key 必须与 Rust reader 一致**。
 * `label` 是该台的稳定身份（origin tag，多机 #30）：Tab 前缀 / 历史分组 / 选台 key。
 * 留空时后端回退用 host。port 缺省 22；keyPath / hostKeyFingerprint 可选。
 */
export interface RemoteHostConfig {
  label: string;
  host: string;
  port: number;
  user: string;
  keyPath: string;
  daemonPath: string;
  hostKeyFingerprint: string;
  /**
   * Batch14-F45：备用地址（happy-eyeballs 竞发）。每项 `host` / `host:port` /
   * `[IPv6]:port` / 裸 IPv6。首选地址仍是 `host` 字段。空数组 = 仅用 host。
   */
  addresses: string[];
  /**
   * Batch14-F56：跳板 ProxyJump——填另一台已配置主机的 `label`（空=直连）。经该跳板机隧道连本机
   * （数据源侧 russh direct-tcpip；拉起侧 ssh `-J`）。fail-closed：跳板缺失/连不上即报错不直连。
   */
  jump: string;
  /**
   * Batch14-F59：daemonless 降级读取——true 时该主机**不部署/不连 daemon**，走纯 SSH exec
   * `find`+`tail -c +offset` 轮询读会话 jsonl（能力子集：无 bg kind / 无运行状态灯 / 无拥塞信号 /
   * 仅显示最近活跃会话）。false（默认）= 正常 daemon 数据源路径。
   */
  daemonless: boolean;
}

/** F45：多行文本 ↔ 地址数组（trim + 去空行）。UI 用 textarea，config/IPC 用数组。 */
export function parseAddressLines(text: string): string[] {
  return text
    .split("\n")
    .map((l) => l.trim())
    .filter((l) => l.length > 0);
}

/** config.json `remote` 段：全局 enabled + 机器列表。 */
export interface RemoteConfig {
  enabled: boolean;
  hosts: RemoteHostConfig[];
}

/**
 * F83（#39）:可打开 SFTP 的远端主机——`host` 与 `user` 都非空（`openSftpPanel` 的前置，
 * 见 remote-section「文件」按钮同款校验）。顶栏 SFTP 入口据此决定 0 台提示 / 1 台直开 / 多台选单。
 * 纯函数（不看 `enabled`：即使远端数据源没启用，也能纯浏览某台的文件）。
 */
export function sftpEligibleHosts(cfg: RemoteConfig): RemoteHostConfig[] {
  return cfg.hosts.filter((h) => h.host.trim() !== "" && h.user.trim() !== "");
}

export const HOST_DEFAULTS: RemoteHostConfig = {
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
};

/** F45：容忍 config 里 addresses 为数组 / 换行文本 / 缺失。 */
function coerceAddresses(v: unknown): string[] {
  if (Array.isArray(v)) {
    return v
      .filter((x): x is string => typeof x === "string")
      .map((x) => x.trim())
      .filter(Boolean);
  }
  if (typeof v === "string") return parseAddressLines(v);
  return [];
}

function coerceHost(obj: Record<string, unknown>): RemoteHostConfig {
  const str = (k: string, d: string) => (typeof obj[k] === "string" ? (obj[k] as string) : d);
  const host = str("host", HOST_DEFAULTS.host);
  return {
    // label 缺省回退 host（与 Rust origin_label 一致：空 label → host）
    label: str("label", "") || host,
    host,
    port:
      typeof obj.port === "number" && Number.isFinite(obj.port)
        ? (obj.port as number)
        : HOST_DEFAULTS.port,
    user: str("user", HOST_DEFAULTS.user),
    keyPath: str("keyPath", HOST_DEFAULTS.keyPath),
    daemonPath: str("daemonPath", HOST_DEFAULTS.daemonPath),
    hostKeyFingerprint: str("hostKeyFingerprint", HOST_DEFAULTS.hostKeyFingerprint),
    addresses: coerceAddresses(obj.addresses),
    jump: str("jump", HOST_DEFAULTS.jump),
    daemonless:
      typeof obj.daemonless === "boolean" ? obj.daemonless : HOST_DEFAULTS.daemonless,
  };
}

/**
 * 读 config.json 的 `remote` 段 → RemoteConfig。**向后兼容**：有 `hosts` 数组 → 逐台读；
 * 无 `hosts` 但有 `host`（旧单对象）→ 归一成 1 台；都没有 → 空列表。永不抛。
 */
export async function readRemoteConfig(): Promise<RemoteConfig> {
  try {
    const cfg = (await loadConfig()) as Record<string, unknown>;
    const r = cfg.remote;
    if (r === null || typeof r !== "object") return { enabled: false, hosts: [] };
    const obj = r as Record<string, unknown>;
    const enabled = typeof obj.enabled === "boolean" ? obj.enabled : false;

    let hosts: RemoteHostConfig[] = [];
    if (Array.isArray(obj.hosts)) {
      hosts = obj.hosts
        .filter((h): h is Record<string, unknown> => h !== null && typeof h === "object")
        .map(coerceHost);
    } else if (typeof obj.host === "string" && obj.host) {
      // 旧单对象形态：把 remote 自身当一台。
      hosts = [coerceHost(obj)];
    }
    return { enabled, hosts };
  } catch (e) {
    console.warn("readRemoteConfig failed:", e);
    return { enabled: false, hosts: [] };
  }
}

/**
 * F54:在主机列表里按 origin 反查(纯函数,便于单测)。origin = 主机 `label` 非空则 label
 * 否则 host,等价于后端 `origin_label()` / launcher 的 `label||host`(**纯空白 label 除外**:
 * 这里 `.trim()` 更稳健,后端不 trim——纯空白 label 属退化配置,退化时反查落空→调用方优雅 toast)。
 * 找不到 → null。
 */
export function findHostByOrigin(
  hosts: RemoteHostConfig[],
  origin: string,
): RemoteHostConfig | null {
  return hosts.find((h) => (h.label.trim() || h.host) === origin) ?? null;
}

/**
 * F54:按 origin(会话来源标识)反查完整 RemoteHostConfig。找不到(主机被删/改名)→ null。
 */
export async function resolveRemoteConfigByOrigin(
  origin: string,
): Promise<RemoteHostConfig | null> {
  return findHostByOrigin((await readRemoteConfig()).hosts, origin);
}

/**
 * 把 RemoteConfig MERGE 进 config.json 顶层的 `remote` 键，不动其他字段。
 * 写成 `{ enabled, hosts: [...] }`（升级旧单对象形态）；key 是 camelCase，与 Rust
 * `lib.rs::load_remote_configs` 读的键严格一致。
 */
export async function writeRemoteConfig(next: RemoteConfig): Promise<void> {
  const cfg = (await loadConfig()) as Record<string, unknown>;
  cfg.remote = {
    enabled: next.enabled,
    hosts: next.hosts.map((h) => ({
      label: h.label,
      host: h.host,
      port: h.port,
      user: h.user,
      keyPath: h.keyPath,
      daemonPath: h.daemonPath,
      hostKeyFingerprint: h.hostKeyFingerprint,
      addresses: h.addresses,
      jump: h.jump, // F56：跳板 label（D-B1:此前漏写 → 设置卡填的跳板被静默丢弃）
      daemonless: h.daemonless, // F59：daemonless 降级开关（同 D-B1 教训：枚举字段必逐个写全，防静默丢失）
    })),
  };
  await saveConfig(cfg);
}

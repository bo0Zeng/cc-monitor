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
  /**
   * S4b-3（主计划 §5-1）：**这台机器**的 resume 启动命令。空 = 用全局默认
   *（`behavior.resumeCommandRemote`）。
   *
   * 为什么必须 per-machine：「装 ccm 助手」是**每台机器一个按钮**，而 resume 命令
   * 此前是**全局单值** ⇒ A 机装了 ccm、B 机没装时，今天的数据模型根本表达不出来。
   * 结果是一个结构性陷阱：装完 ccm 却忘了改 resume 命令（那两处此前还隔着两个顶层组）。
   *
   * 保留全局值当**回退**而不是做数据迁移：迁移会静默改写用户已有的设置，
   * 而回退让「没填过 = 沿用今天的行为」，零意外。
   */
  resumeCommand: string;
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
  resumeCommand: "",
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
    resumeCommand: str("resumeCommand", HOST_DEFAULTS.resumeCommand),
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
  return hosts.find((h) => hostKey(h) === origin) ?? null;
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
 * S1：落盘时要序列化的字段清单 —— **单一事实来源**。
 *
 * ★ 为什么值得单独立一个清单 + 编译期检查：这里原本是逐字段手抄的对象字面量，
 * 而**手抄清单已经咬过两次**，两次都是「`RemoteHostConfig` 加了字段、这里没跟上
 * ⇒ 每次保存都静默丢掉它」：
 * - `jump`（F56 D-B1）：「设置卡填的跳板被静默丢弃」——用户填了，存不下来。
 * - `daemonless`（F59）：补的时候注释里写着「同 D-B1 教训：枚举字段必逐个写全」。
 *
 * 两次都是**事后**补的。下面那个 `MissingField` 检查把它变成**编译期**问题：
 * 加字段而漏改这里，`tsc` 直接红，且错误信息里点名缺的是哪个字段。
 * （比写测试强：测试要有人想起来写；类型检查每次编译都跑。）
 */
const REMOTE_HOST_FIELDS = [
  "label",
  "host",
  "port",
  "user",
  "keyPath",
  "daemonPath",
  "hostKeyFingerprint",
  "addresses",
  "jump",
  "daemonless",
  "resumeCommand",
] as const satisfies readonly (keyof RemoteHostConfig)[];

/** 上面清单**漏掉**的字段（应为 `never`）。 */
type MissingField = Exclude<
  keyof RemoteHostConfig,
  (typeof REMOTE_HOST_FIELDS)[number]
>;
// 漏了字段这一行就红，且 TS 的错误信息会把缺的字段名打出来。
const _noMissingField: MissingField extends never ? true : MissingField = true;
void _noMissingField;

/** 按 [`REMOTE_HOST_FIELDS`] 挑字段落盘（不逐字段手抄，杜绝静默丢失）。 */
function serializeHost(h: RemoteHostConfig): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const k of REMOTE_HOST_FIELDS) out[k] = h[k];
  return out;
}

/**
 * 把 RemoteConfig MERGE 进 config.json 顶层的 `remote` 键，不动其他字段。
 * 写成 `{ enabled, hosts: [...] }`（升级旧单对象形态）；key 是 camelCase，与 Rust
 * `lib.rs::load_remote_configs` 读的键严格一致。
 *
 * ★ S1 起这是**数据层内部的「序列化 + 落盘」单一出口**，`remote` 键的盘上形状只有
 * 这里知道。**刻意不 export**：它是整表覆盖语义，一旦调用方手上只有部分机器
 *（S2 把机器拆成一页一台之后就是这样）就会把其余的静默删光。
 * 不导出 = 这条footgun 在**类型层面不可达**，不靠「记得别用它」这种纪律。
 * 对外只有 [`patchRemoteConfig`]（局部合并，安全性质来自构造）。
 */
async function writeRemoteConfig(next: RemoteConfig): Promise<void> {
  const cfg = (await loadConfig()) as Record<string, unknown>;
  cfg.remote = {
    enabled: next.enabled,
    hosts: next.hosts.map(serializeHost),
  };
  await saveConfig(cfg);
}

/** S1：一台机器在盘上的定位键 = 它的 origin（与 [`findHostByOrigin`] 同口径）。 */
/**
 * S4b-3（主计划 §5-1）：某台机器该用哪条 resume 启动命令。**纯函数。**
 *
 * **per-machine 优先，全局兜底。** 全局值（`behavior.resumeCommandRemote`）从「唯一真相」
 * 降级为「默认值」—— 这样没填过 per-machine 的机器行为**一字不变**，不需要数据迁移
 *（迁移会静默改写用户已有的设置）。
 *
 * 为什么必须 per-machine：「装 ccm 助手」是**每台机器一个按钮**，而 resume 命令此前是
 * 全局单值 ⇒ A 机装了 ccm、B 机没装时，数据模型根本表达不出来。于是「装完 ccm 却忘了改
 * resume 命令」是个**结构性陷阱**（那两处此前还隔着两个顶层组）。
 */
export function pickResumeCommand(
  host: RemoteHostConfig | null,
  globalDefault: string,
): string {
  return (host?.resumeCommand ?? "").trim() || globalDefault;
}

/** [`pickResumeCommand`] 的 IO 包装：按 origin 查这台机器，再决定用哪条命令。 */
export async function resolveResumeCommand(
  origin: string,
  globalDefault: string,
): Promise<string> {
  return pickResumeCommand(
    await resolveRemoteConfigByOrigin(origin),
    globalDefault,
  );
}

export function hostKey(h: RemoteHostConfig): string {
  return h.label.trim() || h.host;
}

/**
 * S1：一次**局部**修改。没被提到的机器，`applyRemoteHostsPatch` 根本不碰。
 */
export interface RemoteHostsPatch {
  /** 全局开关。缺省 = 不动。 */
  enabled?: boolean;
  /**
   * 要写入的机器。`key` = 这条记录**在盘上当前的 origin**；`null` = 新增（追加到末尾）。
   * key 在盘上找不到（被别处删了/改了）**也按新增处理** —— 比静默丢弃安全。
   */
  upsert?: { key: string | null; value: RemoteHostConfig }[];
  /** 要删除的机器，按 origin。 */
  remove?: string[];
}

/**
 * S1 **纯函数**（可单测，不碰文件系统）：把 patch 应用到一份现有配置上。
 *
 * ★ 安全性质来自**构造**，不是来自调用方守纪律：patch 里没提到的 host，这里根本不读
 * 也不写，原对象引用直接留在结果里。S2 把机器卡片拆成一页一台之后，那一页只提交自己
 * 那几台，其余机器天然安全 —— 而在整表覆盖的老实现下，同样的调用会把它们**静默删光**。
 *
 * 顺序语义：先 `remove` 后 `upsert`。这样「删掉 A、同时新增一台也叫 A」是**替换**，
 * 不是「新增后被删」。
 */
export function applyRemoteHostsPatch(
  cur: RemoteConfig,
  patch: RemoteHostsPatch,
): RemoteConfig {
  const removeSet = new Set(patch.remove ?? []);
  const hosts = cur.hosts.filter((h) => !removeSet.has(hostKey(h)));

  // 匹配过的下标不再复用。**没有这一步，两台 origin 相同的机器会互相踩**：
  // 第二条 upsert 会再次命中第一条已经被替换过的位置，前一条编辑凭空消失。
  // （origin 重复本身是无效配置——整个系统都拿 origin 当机器身份——但
  // 「无效配置」不该变成「静默吞掉用户的编辑」。见 BACKLOG E44。）
  const used = new Set<number>();
  for (const { key, value } of patch.upsert ?? []) {
    const idx =
      key === null
        ? -1
        : hosts.findIndex((h, i) => !used.has(i) && hostKey(h) === key);
    if (idx >= 0) {
      hosts[idx] = value;
      used.add(idx);
    } else {
      hosts.push(value);
    }
  }

  return {
    enabled: patch.enabled ?? cur.enabled,
    hosts,
  };
}

/**
 * S1：[`applyRemoteHostsPatch`] 的 IO 包装 —— read-modify-write。
 * UI 侧唯一的写入口。
 */
export async function patchRemoteConfig(
  patch: RemoteHostsPatch,
): Promise<void> {
  const cur = await readRemoteConfig();
  await writeRemoteConfig(applyRemoteHostsPatch(cur, patch));
}

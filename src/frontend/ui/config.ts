// config.json 读写桥：读直通 Rust `load_config`；写只有 `patch_config` 一个口（schema-agnostic，
// 后端按 serde_json::Value 透传，所有字段语义收敛在前端各模块）。
//
// 写只交「改哪几条路径」（[`ConfigEdit`]），不交整份：主窗与设置窗是两个 realm，读-改-写整份一交错就把别人的键盖掉。
// Rust `config.rs::patch_config_at` 在一把进程级锁里现读盘、逐条应用 ⇒ 谁写的键谁的值留在盘上
// （判据：`tests/frontend/ui/config-lost-update.vitest.ts` · `tests/frontend/shell/config_tests.rs`）。
//
// 未知键不静默：用户手写的键各模块认不出就一个字都不说，用户只看到「我明明写了，它没生效」。
// 钉在唯一的读盘口上：每一次 `loadConfig()` 都拿盘上的顶层键去对 [`KNOWN_CONFIG_KEYS`]，对不上的记下来，
// 由 `settings/unknown-keys-notice.ts` 画成一条**常驻**提示条（`INVARIANTS §12`：
// 状态性警告不许只活在 hover / 日志里）。
import { commands } from "./ipc/commands";
import type { ConfigEdit } from "./generated/ConfigEdit";

export type { ConfigEdit };

export type Config = Record<string, unknown>;

/**
 * 🔴 **config.json 顶层键的唯一登记。**
 *
 * 每一行后面那个模块是它**今天的主人** —— 键的语义只在那一个文件里解释
 *（`config.rs` 是 schema-agnostic 的，Rust 侧只透传）。
 *
 * ⚠ **加了新键却忘了加在这里**，后果不是静默：app 会当它是未知键，
 * 在设置里指名道姓地喊出来。吵，但不瞎 —— 这正是本表要的失效方向。
 *
 * ⚠ 射程：只管**顶层**。子对象里的键（`remote.hosts[].keyPath`、`theme.bg` …）
 * 由各自的主人自己解释，本表一个都不认、也不该认。
 */
/** 第 7 批搬走的那几格的主人：迁移那一段（壳读到本机 `prefs-read` 以后删这几个键，原文另存 `config.json.pre-7`）。下一版删迁移时这几行一起删。 */
const PREFS_MIGRATION = "src/frontend/shell/src/prefs_migration.rs";

export const CONFIG_KEY_OWNERS = {
  // src/frontend/ui/theme.ts
  theme: "src/frontend/ui/theme.ts",
  // src/frontend/ui/keybindings/store.ts
  keybindings: "src/frontend/ui/keybindings/store.ts",
  // ── 第 7 批搬走的那几格（这台的偏好住各台后端 `prefs.json`）：只剩迁移那一段读、迁完由壳删（原文留 `config.json.pre-7`）──
  // `accounts.byMachine.<origin>.modelByAccount`（每号模型）。
  accounts: PREFS_MIGRATION,
  // Claude 数据目录（搬进本机后端 `prefs.json` 的 `agentHomes`）。
  claudeDir: PREFS_MIGRATION,
  // 机器表（搬进本机后端 `machines.json`：后端换形那一段只读它，本机 `machines-read` 说 `ok` 之后由壳删）。
  remote: PREFS_MIGRATION,
  // src/frontend/ui/tab-bar-state.ts
  tabBar: "src/frontend/ui/tab-bar-state.ts",
  // src/frontend/ui/tab-collections.ts
  tabCollections: "src/frontend/ui/tab-collections.ts",
  // src/frontend/ui/session-head.ts：会话头上放哪几颗按钮（`sessionHead.buttons`，这扇界面自己的偏好；没有 ＝ 默认）。
  sessionHead: "src/frontend/ui/session-head.ts",
  // 上下文上限表。
  contextLimits: PREFS_MIGRATION,
  // ── src/frontend/ui/behavior.ts 那一族 ─────────────────────────────────────────────
  autoFollowUserActive: "src/frontend/ui/behavior.ts",
  bringMonitorToFrontOnUserActive: "src/frontend/ui/behavior.ts",
  showBgSessions: "src/frontend/ui/behavior.ts",
  // 用过的恢复命令（只是这扇界面的输入历史；生效值住各台后端）。
  resumeCommandPresets: "src/frontend/ui/behavior.ts",
  // 通用页那一格恢复命令 · 恢复默认进不进 tmux（搬走的那一族）。
  resumeCommand: PREFS_MIGRATION,
  resumeInTmux: PREFS_MIGRATION,
  notifyTurnEnd: "src/frontend/ui/behavior.ts",
  notifyNeeds: "src/frontend/ui/behavior.ts",
  // 本机那一格恢复命令（搬走的那一族）。
  localResumeCommand: PREFS_MIGRATION,
  // src/frontend/ui/settings/terminal-row.ts 写（设置 → 通用 → 终端）；壳的平台层 `platform/terminal.rs` 开终端时读（只读）。
  terminal: "src/frontend/ui/settings/terminal-row.ts",
  // src/frontend/shell/src/logging.rs —— **Rust 写的**顶层键（设置页「诊断」经 `set_diagnostics_config`）。
  // 漏登记的话，用户存过一次诊断设置，「认不出的键」提示条就会把 `diagnostics` 点名（假警报）。
  diagnostics: "src/frontend/shell/src/logging.rs",
} as const satisfies Readonly<Record<string, string>>;

/** config.json 的顶层键。写口的路径首段只收这些 ⇒ 写一个没登记的顶层键**编不过**。 */
export type ConfigKey = keyof typeof CONFIG_KEY_OWNERS;

/** [`CONFIG_KEY_OWNERS`] 的键集合。判据与运行期共用同一份，不另抄第二份。 */
export const KNOWN_CONFIG_KEYS = Object.keys(CONFIG_KEY_OWNERS) as readonly ConfigKey[];

/**
 * 一份配置里有哪些 app 不认识的顶层键 —— **纯函数**，好让判据直接打在这里。
 *
 * 顺序按它在盘上出现的顺序（`Object.keys` 的插入序），这样用户在提示条上读到的
 * 顺序与他自己文件里的顺序一致。
 *
 * ⚠ 非对象（`null` / 数组 / 数字）一律当「没有未知键」：那不是「多写了一个键」，
 * 是整份配置坏了，属于 `load_config` 的 parse 失败面，不归本函数说。
 */
export function unknownKeysIn(cfg: unknown): string[] {
  if (!cfg || typeof cfg !== "object" || Array.isArray(cfg)) return [];
  const known = new Set<string>(KNOWN_CONFIG_KEYS);
  return Object.keys(cfg as Record<string, unknown>).filter((k) => !known.has(k));
}

/**
 * 最近一次 `loadConfig()` 读到的未知键。
 *
 * 为什么是模块级快照而不是每次现算：提示条与读盘发生在不同的时刻（用户开设置时
 * app 早就读过配置了），而**「盘上那一刻是什么」才是要报的事实**。提示条自己也会
 * 再 `loadConfig()` 一次刷新它，两条路更新的是同一格。
 */
let lastUnknownKeys: readonly string[] = [];

/** 最近一次读盘时，配置里有哪些 app 不认识的顶层键。空数组 = 没有。 */
export function unknownConfigKeys(): string[] {
  return [...lastUnknownKeys];
}

/** 仅供测试：把那格快照清掉（模块级状态会跨用例串味）。 */
export function __resetUnknownConfigKeysForTests(): void {
  lastUnknownKeys = [];
}

export async function loadConfig(): Promise<Config> {
  const cfg = await commands.load_config();
  lastUnknownKeys = unknownKeysIn(cfg);
  if (lastUnknownKeys.length > 0) {
    // 这一行不是「出声」那一半（日志里的话用户看不见）；出声的是设置里那条常驻条。
    console.warn("[config] 认不出的顶层键：", [...lastUnknownKeys].join(" "));
  }
  return cfg;
}

/** 路径首段是登记过的顶层键，其后是逐层子键。 */
export type ConfigPath = readonly [ConfigKey, ...string[]];

/** 一条「把这个路径设成这个值」。值必须能 JSON 化；`undefined` 是写者的错（线上会整条丢字段）⇒ 抛。 */
export function setAt(path: ConfigPath, value: unknown): ConfigEdit {
  if (value === undefined) throw new Error(`setAt(${path.join(".")}): value is undefined`);
  return { op: "set", path: [...path], value };
}

/** 一条「删掉这个路径」。路上就没有 ⇒ Rust 那边什么都不做。 */
export function removeAt(path: ConfigPath): ConfigEdit {
  return { op: "remove", path: [...path] };
}

/**
 * 本 realm 的写队列：同一个窗口里先发的写先落盘，不靠 IPC 到达顺序
 *（同一模块连发两次同一个键，盘上必是后发的那次）。失败不堵后面的写。
 */
let queue: Promise<unknown> = Promise.resolve();

function enqueue<T>(job: () => Promise<T>): Promise<T> {
  const run = queue.then(job, job);
  queue = run.catch(() => undefined);
  return run;
}

/** 🔴 **写 config.json 的唯一口。** 只交改哪几条路径。 */
export function patchConfig(edits: readonly ConfigEdit[]): Promise<void> {
  const copy = [...edits];
  return enqueue(() => commands.patch_config({ edits: copy }));
}

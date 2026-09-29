// config.json 读写桥：读直通 Rust `load_config`；写只有 `patch_config` 一个口（schema-agnostic，
// 后端按 serde_json::Value 透传，所有字段语义收敛在前端各模块）。
//
// 🔴 〔CFG1 · 4D〕**写只交「改哪几条路径」，不交整份。** 从前 9 个模块各自「读整份 → 改自己的键 →
// `saveConfig(整份)`」，主窗（tab 栏）与设置窗是两个 realm，两次读-改-写一交错，后写的整份就把先写的键 〔散文墓碑〕
// 盖掉（E §E1：拖放同拍发分组 ＋ 顺序，分组那次没落盘）。现在 [`patchConfig`] 只交 [`ConfigEdit`]，
// Rust `config.rs::patch_config_at` 在一把进程级锁里现读盘、逐条应用 ⇒ 谁写的键谁的值留在盘上
// （`设计/30 §4`「各自只写自己那个键」）。判据：`tests/frontend/ui/config-lost-update.vitest.ts` · `tests/frontend/shell/config_tests.rs`。
//
// 🔴 〔`设计/99 §2.5 P12` 2026-09-21〕**「未知键静默忽略」在这里止住。**
//
// 本仓的头号病形逐字是「『关掉了』与『过了』在终端上一模一样」。config.json 上它长这样：
// 用户手写一个键 → 各模块只按自己认得的键名取值 → 认不出的**一个字都不说**
// ⇒ 用户看到的是「我明明写了，它没生效」，而 app 这一侧**连它存在都不知道**。
//
// 治法不是在每个读者那里各加一句（那是 N 个住址、必漂），是钉在**唯一的读盘口**上：
// 每一次 `loadConfig()` 都拿盘上的顶层键去对 [`KNOWN_CONFIG_KEYS`]，对不上的记下来，
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
export const CONFIG_KEY_OWNERS = {
  // src/frontend/ui/theme.ts
  theme: "src/frontend/ui/theme.ts",
  // src/frontend/ui/paths.ts
  claudeDir: "src/frontend/ui/paths.ts",
  // src/frontend/ui/keybindings/store.ts
  keybindings: "src/frontend/ui/keybindings/store.ts",
  // src/frontend/ui/accounts.ts
  accounts: "src/frontend/ui/accounts.ts",
  // 〔B2 · 条 66〕原来这里有 `backendPolicy`（src/frontend/ui/backend-policy.ts）—— 「退出行为」那个值
  //   搬到了后端所在那台机器上（`设计/01 §3.3b ②`「monitor 的 config 里不许再留一份」）⇒ 这一键退役。
  //   盘上还留着它的旧 config.json 会被下面那条「不认识的键」提示条点名 —— 那是对的：它确实没人读了。
  // src/frontend/ui/remote-config.ts（Rust 侧 `lib.rs::load_remote_configs` 也读它，只读）
  remote: "src/frontend/ui/remote-config.ts",
  // src/frontend/ui/tab-bar-state.ts
  tabBar: "src/frontend/ui/tab-bar-state.ts",
  // src/frontend/ui/tab-collections.ts
  tabCollections: "src/frontend/ui/tab-collections.ts",
  // src/frontend/ui/usage-hud.ts 读（经 `views/context-limit.ts::readContextLimits`）；〔FIX4 · `70 §10` 第 8 条〕设置页「外观 → 高级」写
  //   （`settings/context-limits-section.ts`），不再只能手改 config.json。
  contextLimits: "src/frontend/ui/usage-hud.ts",
  // ── src/frontend/ui/behavior.ts 那一族 ─────────────────────────────────────────────
  autoFollowUserActive: "src/frontend/ui/behavior.ts",
  bringMonitorToFrontOnUserActive: "src/frontend/ui/behavior.ts",
  showBgSessions: "src/frontend/ui/behavior.ts",
  resumeCommandLocal: "src/frontend/ui/behavior.ts",
  resumeCommandRemote: "src/frontend/ui/behavior.ts",
  resumeCommandLocalPresets: "src/frontend/ui/behavior.ts",
  resumeCommandRemotePresets: "src/frontend/ui/behavior.ts",
  notifyTurnEnd: "src/frontend/ui/behavior.ts",
  // 〔LR2〕`forceLaunchPayloadRenderer` 退役（`src/frontend/ui/behavior.ts` 那段注释写了为什么）⇒ 这一键删掉，盘上还写着它就当未知键点名。
  // src/frontend/shell/src/logging.rs —— **Rust 写的**顶层键（设置页「诊断」经 `set_diagnostics_config`）。
  // 〔CFG1〕从前漏登记：用户存过一次诊断设置，「认不出的键」提示条就把 `diagnostics` 点名（假警报）。
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
    // ⚠ 这一行**不是**「出声」那一半 —— 日志里的话用户看不见，而看不见的告知与
    //   没告知在终端上一模一样（P12 要治的就是这个）。真正出声的是设置里那条常驻条。
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

/**
 * 要先看旧值才算得出新值的写者用这个（`remote` 段按机器增删改）：在本 realm 的写队列里**现读** →
 * `build(cfg)` 出补丁 → 写。读失败 ⇒ 抛（**不**当空配置再写回 —— 那会把别的机器写没）。
 */
export function patchConfigFrom(
  build: (cfg: Config) => readonly ConfigEdit[],
): Promise<void> {
  return enqueue(async () => {
    const edits = build(await loadConfig());
    if (edits.length > 0) await commands.patch_config({ edits: [...edits] });
  });
}

// P2s（定框 C8）：**每台机一份 daemon 策略** —— 今天只有一条「monitor 退出时是否结束它」。
//
// # 持久化归这里，生效值归 Rust
//
// `config.rs` 头注逐字「Rust 端**不解释配置内容**（schema 在前端定义）」。
// 若 Rust 也往 config.json 里读写，同一个文件就有**两个写者**，
// 前端「读—改—写整份」的那一刻会拿一份陈旧副本把 Rust 刚写的键覆盖掉。
// ⇒ 这里存盘，改动时与启动时把生效值**推**给 Rust（`set_daemon_kill_on_exit`）。
//
// # ⚠ 「不结束」不等于「继续跑」
//
// 实测（08-11，P2s §0a）：daemon 是纯 stdio 子进程，monitor 一退读端就断，
// 它在 **153 毫秒**内自己 broken-pipe 退出。所以这条策略的真实语义是
// **「立刻结束」与「让它自己退」之差**，不是「后台常驻」。
// **UI 文案不许写「daemon 继续运行」**（P2s-Y5）。真要常驻见待决 U7。

import { commands } from "./ipc/commands";
import { loadConfig, saveConfig, type Config } from "./config";

/**
 * 本机在 origin 这套命名里的名字。
 *
 * ⚠ **跨语言常量**：Rust 侧是 `inbound_client::LOCAL_ORIGIN`。两边漂了**不会报错** ——
 * 本机开关会去操作一个谁都没登记过的 origin，设了没反应且不报错。
 * 由 Rust 侧一条判据逐字对拍（`the_local_origin_is_the_same_string_on_both_sides`）。
 */
export const LOCAL_ORIGIN = "<local>";

/** 缺省：**不结束**（C8③ 的前半句 —— 那半是站得住的）。 */
export const DEFAULT_KILL_ON_EXIT = false;

/** config.json 里的键。 */
const KEY = "daemonPolicy";

export type DaemonPolicy = Record<string, boolean>;

/** 从整份 config 里取策略表；缺席/形状不对都退回空表（**不抛**——开关坏了不该拖垮设置页）。 */
export function readPolicy(cfg: Config): DaemonPolicy {
  const raw = cfg[KEY];
  if (typeof raw !== "object" || raw === null || Array.isArray(raw)) return {};
  const out: DaemonPolicy = {};
  for (const [k, v] of Object.entries(raw as Record<string, unknown>)) {
    if (typeof v === "boolean") out[k] = v;
  }
  return out;
}

/** 这台机退出时结不结束它。未登记 ⇒ 缺省。 */
export function killOnExit(policy: DaemonPolicy, origin: string): boolean {
  return policy[origin] ?? DEFAULT_KILL_ON_EXIT;
}

/** 把整表推给 Rust（启动时一次）。**逐台推**——Rust 那边是 per-origin 的表，没有「整表覆盖」这个口。 */
export async function pushPolicyToBackend(policy: DaemonPolicy): Promise<void> {
  for (const [origin, kill] of Object.entries(policy)) {
    await commands.set_daemon_kill_on_exit({ origin, kill });
  }
}

/** 启动时：读盘 → 推给 Rust。返回读到的表，供 UI 初始化用。 */
export async function initDaemonPolicy(): Promise<DaemonPolicy> {
  const policy = readPolicy(await loadConfig());
  await pushPolicyToBackend(policy);
  return policy;
}

/**
 * 改一台机的策略：**先推后存**。
 *
 * 顺序是刻意的 —— 推失败就不落盘，否则盘上写着 A 而运行中是 B，
 * 下次启动才「自动修好」，中间那段时间用户看到的开关是骗人的。
 */
export async function setKillOnExit(origin: string, kill: boolean): Promise<void> {
  if (!origin.trim()) throw new Error("origin 不许为空 —— 策略是每台机各一份的");
  await commands.set_daemon_kill_on_exit({ origin, kill });
  const cfg = (await loadConfig()) as Record<string, unknown>;
  const policy = readPolicy(cfg);
  policy[origin] = kill;
  cfg[KEY] = policy;
  await saveConfig(cfg);
}

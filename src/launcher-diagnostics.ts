/**
 * F08（unify-launch）：越层启动器诊断。
 *
 * MASTERPLAN 设计原则#7：越层启动器只诊断 + 引导迁移，绝不自动降级、**绝不在用户没要求时
 * 改他的配置**（「用户按了一个写明写到哪、写什么的按钮」不在禁令内，`K-R49` 那一次收窄的原话仍成立）。
 *
 * 〔AL1 · 2026-09-24〕**别名那两块搬走了**：「按账号生成命令」与「生成自定义别名」合成一类
 * （`设计/71`：别名 ＝ 名字 ＋ 一组 ccm 参数），住机器页「本机 → 工具 → 别名」，
 * 代码在 `src/settings/machine-aliases.ts`；shell 文本改由后端渲染（`account_aliases::render`），
 * 本文件从前那个 TS 生成器与「该调哪一份 ccm」的三档判词一起退役
 * —— 别名里只写裸 `ccm`（`01 §6.7b`），「你 PATH 上那个 `ccm` 是旧的」那句话照样在别名那一块上屏。
 * 「用户级 PATH」那一格（`K-R135`）跟着一起走了：它与 rc 别名块是**同一个问题的两条路**
 * （「这台机器的终端怎么找到 ccm」：POSIX 走 rc 别名块，Windows 走用户级 PATH）。
 */
// `K-R93`：**盘上有几个 agent、默认是哪个，都是后端的事实** —— 本文件从前把
// `claude` / `codex` 写死了两处（下拉清单 + 越层诊断的豁免名单）。
import { listAgents, lookupAgentProfile } from "./agent-profile";

/**
 * 后端认得的 agent 的**默认拉起二进制名**（`claude` / `codex` / …）。
 *
 * 🔴 `K-R93`：刻意**只收 `defaultLauncher`，不收 `launcherAlias`**。
 * 别名（claude 的 `cc`）是用户自己那层 shell wrapper —— 它与 `cct` / `oot` 是同一类东西，
 * 照样绕开 `ccm`，**不该被豁免**。这两个值住在同一行画像里，但它们回答的不是同一个问题。
 */
function baseLaunchers(): string[] {
  const out: string[] = [];
  for (const agent of listAgents()) {
    const got = lookupAgentProfile(agent);
    // 问不到就跳过、不猜 —— 少豁免一个只是多提示一句，编一个出来才是错的。
    if (got.known) out.push(got.facts.defaultLauncher);
  }
  return out;
}

/** 该远端命令看起来是不是绕开了 `ccm`（越层启动器）——启发式：非空、不含 "ccm"、且不是
 *  某个 agent 的裸基座命令（显式写 `claude` / `codex` 是有意选择基座行为，不算"看起来像
 *  旧式包装"）。命中不代表一定错——用户可能就是要一个完全自定义的命令——只是账号/模型偏好
 *  不会随它生效，值得提醒。
 *
 *  🔴 `K-R93`（09-12）：这里从前只豁免 `claude` 一个字面量 —— 那是「前端只认 claude」
 *  那一格漏的**第二处**（第一处是 `AGENT_PROFILE`）。填 `codex` 的人从前会收到一句
 *  「你绕开了 ccm」，而他做的与填 `claude` 是同一件事。今天名单由后端那张表给。 */
export function diagnoseRemoteLauncher(cmd: string): string | null {
  const trimmed = cmd.trim();
  if (!trimmed) return null; // 空 = 走默认（后端 `ACTIVE_AGENT` 那一份），不算绕过
  if (baseLaunchers().includes(trimmed)) return null; // 显式基座，不是旧式包装
  if (/ccm/.test(trimmed)) return null; // 命令本身含 ccm 子串（可能是包了一层的自定义命令）
  return "这条命令似乎绕开了 ccm——账号/模型偏好不会随它生效。想要这些好处的话，改填 ccm（或含 ccm 的自定义命令），或在机器页「本机 → 工具 → 别名」里拼一条。";
}

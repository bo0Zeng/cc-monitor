/**
 * 越层启动器诊断：只诊断 ＋ 引导迁移，不自动降级、不在用户没要求时改他的配置（用户按了一个写明写到哪、写什么的按钮不算）。
 * 别名与用户级 PATH 在机器页「本机 → 终端 → 别名」（`settings/machine-aliases.ts`，shell 文本由后端渲染）。
 */
// 盘上有几个 agent、默认是哪个，都是后端的事实（不写死 `claude` / `codex`）。
import { listAgents, lookupAgentProfile } from "./agent-profile";
import { copyText } from "./copy-table";

/**
 * 后端认得的 agent 的**默认拉起二进制名**（`claude` / `codex` / …）。
 *
 * 只收 `defaultLauncher`、不收 `launcherAlias`：别名（claude 的 `cc`）是用户自己那层 shell wrapper，照样绕开 `ccm`，不该被豁免。
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
 *  不会随它生效，值得提醒。豁免名单由后端那张表给（每一家的裸基座命令）。 */
export function diagnoseRemoteLauncher(cmd: string): string | null {
  const trimmed = cmd.trim();
  if (!trimmed) return null; // 空 = 走默认（后端注册表里默认那一家的启动器），不算绕过
  if (baseLaunchers().includes(trimmed)) return null; // 显式基座，不是旧式包装
  if (/ccm/.test(trimmed)) return null; // 命令本身含 ccm 子串（可能是包了一层的自定义命令）
  return copyText("launcherDiagnostics.diagnoseRemoteLauncher.bypassesCcm");
}

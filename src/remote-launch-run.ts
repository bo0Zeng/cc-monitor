/**
 * F41：远端 resume 拉起执行器（UI 侧）。连接 remote-launch 纯函数与后端
 * `launch_remote_terminal`（wt.exe/PowerShell 拉起 `ssh -t …`），tabs.ts 与
 * views/history.ts 共用此单一入口，防两处行为漂移。
 *
 * 失败回退 = F09 旧行为：复制命令 + toast 说明（非 Windows dev / 配置缺失 /
 * wt+PowerShell 都 spawn 失败时，用户仍拿得到可粘贴命令，功能永不变砖）。
 */
import { invoke } from "@tauri-apps/api/core";
import {
  buildResumeDirectCmd,
  buildResumeTmuxCmd,
  buildAttachCmd,
  buildLauncherCmd,
  deriveTmuxName,
} from "./remote-launch";
import { showActionFailureToast } from "./error-toast";
import { AGENT_PROFILE } from "./agent-profile";

/** 一键 resume 远端会话：拉起成功 toast 告知；失败回退复制命令。 */
export async function runRemoteResume(
  origin: string,
  sid: string,
  cwd: string,
  launcher: string,
  configDir?: string, // A4：非空 → 该会话用指定账号 resume（CLAUDE_CONFIG_DIR 注入）
): Promise<void> {
  let cmd: string;
  try {
    cmd = buildResumeDirectCmd(sid, cwd, launcher, configDir);
  } catch (err) {
    showActionFailureToast("无法构造 resume 命令", String(err));
    return;
  }
  try {
    await invoke("launch_remote_terminal", { origin, remoteCmd: cmd });
    showActionFailureToast(
      "已拉起远端 resume",
      `新终端窗口正在连接 [${origin}] 并 resume 该会话。`,
      { level: "info", durationMs: 6000 },
    );
    return;
  } catch (err) {
    // 回退：复制命令让用户自己粘贴（保留 F09 语义）。
    let copied = true;
    try {
      await navigator.clipboard.writeText(cmd);
    } catch {
      copied = false; // 命令在 toast 里仍可见，可手动复制
    }
    showActionFailureToast(
      copied ? "拉起失败，已复制 resume 命令" : "拉起失败，请手动复制以下命令",
      `${String(err)}\n到远端 [${origin}] 的 ssh 终端粘贴执行：\n${cmd}`,
      { level: "info", durationMs: 10000 },
    );
  }
}

/** F52：tmux 版 resume——在远端 tmux 会话 `cc-<sid8>` 里幂等 resume Claude;失败回退复制命令。
 *
 *  @returns 是否**真的把终端拉起来了**。false = 命令构造失败 / `launch_remote_terminal` 失败
 *  （此时已走剪贴板回退，需用户手动粘贴）。
 *  account-ux Phase G 审计:此前返回 void 且两条失败路径都自己吞掉,于是 `restartWithAccount`
 *  把"走到了第⑤步"当成"已 resume"——会话被 kill、没起来,却照样记 pin、照样弹「已用新账号重启」、
 *  照样 return true,批量对齐还把它计成成功。而失败是**确定性**的（如 F34 launcher 含双引号被
 *  launch.rs 拒、tmux 名不合白名单、缺 OpenSSH），不是概率事件。 */
export async function runRemoteResumeTmux(
  origin: string,
  sid: string,
  cwd: string,
  launcher: string,
  name?: string,
  configDir?: string, // A4：非空 → 该会话用指定账号 resume（CLAUDE_CONFIG_DIR 注入）
): Promise<boolean> {
  let cmd: string;
  try {
    cmd = buildResumeTmuxCmd(sid, cwd, launcher, name, configDir);
  } catch (err) {
    showActionFailureToast("无法构造 tmux resume 命令", String(err));
    return false;
  }
  try {
    await invoke("launch_remote_terminal", { origin, remoteCmd: cmd });
    showActionFailureToast(
      "已拉起 tmux resume",
      `新终端窗口正在连接 [${origin}] 并在 tmux 会话里 resume 该会话。`,
      { level: "info", durationMs: 6000 },
    );
    return true;
  } catch (err) {
    let copied = true;
    try {
      await navigator.clipboard.writeText(cmd);
    } catch {
      copied = false;
    }
    showActionFailureToast(
      copied ? "拉起失败，已复制 tmux resume 命令" : "拉起失败，请手动复制以下命令",
      `${String(err)}\n到远端 [${origin}] 的 ssh 终端粘贴执行：\n${cmd}`,
      { level: "info", durationMs: 10000 },
    );
    return false;
  }
}

/**
 * F96：历史页「在该目录起新会话」——远端分支。tmux 会话名由 cwd 派生、默认拉起命令由
 * `AGENT_PROFILE` 兜底，**让调用方（history.ts）既不必知道底下用不用 tmux、也不必知道
 * 默认拉起是哪个 agent**（用户 2026-07-15 硬约束）——history.ts 只传 F34 配置命令（可空）。
 * 薄封装 F53 的 `runRemoteLauncher`，不写第二份拉起逻辑。
 * （`buildLauncherCmd` 只对 `undefined` 套默认、空串不触发，故默认在此显式兜。）
 */
export async function runNewSessionRemote(
  origin: string,
  cwd: string,
  command: string,
  configDir?: string, // A4：非空 → 新会话用指定账号启动
): Promise<void> {
  await runRemoteLauncher(
    origin,
    cwd,
    deriveTmuxName(cwd),
    command || AGENT_PROFILE.defaultLauncher,
    configDir,
  );
}

/** F53：「在这台机开新 Claude」——在远端 tmux 会话里启动全新 Claude;失败回退复制命令。 */
export async function runRemoteLauncher(
  origin: string,
  cwd: string,
  tmuxName: string,
  command: string,
  configDir?: string, // A4：非空 → 新会话用指定账号启动（CLAUDE_CONFIG_DIR 注入）
): Promise<void> {
  let cmd: string;
  try {
    cmd = buildLauncherCmd(cwd, tmuxName, command, configDir);
  } catch (err) {
    showActionFailureToast("无法构造 launcher 命令", String(err));
    return;
  }
  try {
    await invoke("launch_remote_terminal", { origin, remoteCmd: cmd });
    showActionFailureToast(
      "已拉起「开新 Claude」",
      `新终端窗口正在连接 [${origin}] 并在 tmux 会话「${tmuxName}」里启动 Claude。`,
      { level: "info", durationMs: 6000 },
    );
  } catch (err) {
    let copied = true;
    try {
      await navigator.clipboard.writeText(cmd);
    } catch {
      copied = false;
    }
    showActionFailureToast(
      copied ? "拉起失败，已复制命令" : "拉起失败，请手动复制以下命令",
      `${String(err)}\n到远端 [${origin}] 的 ssh 终端粘贴执行：\n${cmd}`,
      { level: "info", durationMs: 10000 },
    );
  }
}

/** F51：一键 attach 到远端 tmux 会话:拉起 `ssh -t … tmux attach -t <名>`;失败回退复制命令。 */
export async function runRemoteAttach(origin: string, name: string): Promise<void> {
  let cmd: string;
  try {
    cmd = buildAttachCmd(name);
  } catch (err) {
    showActionFailureToast("无法构造 attach 命令", String(err));
    return;
  }
  try {
    await invoke("launch_remote_terminal", { origin, remoteCmd: cmd });
    showActionFailureToast(
      "已拉起 tmux attach",
      `新终端窗口正在连接 [${origin}] 并 attach 到 tmux 会话「${name}」。`,
      { level: "info", durationMs: 6000 },
    );
  } catch (err) {
    let copied = true;
    try {
      await navigator.clipboard.writeText(cmd);
    } catch {
      copied = false;
    }
    showActionFailureToast(
      copied ? "拉起失败，已复制 attach 命令" : "拉起失败，请手动复制以下命令",
      `${String(err)}\n到远端 [${origin}] 的 ssh 终端粘贴执行：\n${cmd}`,
      { level: "info", durationMs: 10000 },
    );
  }
}

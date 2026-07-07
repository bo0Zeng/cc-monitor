/**
 * 远端 resume 命令构造（纯函数，零 import 便于 `node` 单测）。issue F09。
 *
 * R7 约束：monitor 在本地 Windows、用户的交互终端在远端——monitor 无法在远端开一个
 * 可输入的 TTY，故远端 resume 做不到像本地那样开窗，只能给用户一条可粘贴的命令，由用户
 * 在自己的远端 ssh 终端执行。
 */

/**
 * 构造远端 resume 命令：cwd 非空 → `cd "<cwd>" && <launcher> --resume <sid>`；空 → `<launcher> --resume <sid>`。
 * cwd 用双引号包裹（内部 `"` 转义）以容忍路径含空格。
 * F34：launcher 可自定义（设置面板「远端 resume 命令」，如 `cct`）；空/空白 = 默认 `claude`。
 * 命令由用户自己粘贴到自己的终端执行，无注入面，故不做字符校验。
 */
export function buildRemoteResumeCmd(sid: string, cwd: string, launcher = "claude"): string {
  const l = launcher.trim() || "claude";
  const resume = `${l} --resume ${sid}`;
  const c = cwd.trim();
  if (!c) return resume;
  const quoted = `"${c.replace(/"/g, '\\"')}"`;
  return `cd ${quoted} && ${resume}`;
}

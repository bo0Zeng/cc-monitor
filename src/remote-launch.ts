/**
 * 远端拉起命令构造（纯函数，零 import 便于 `node` 单测）。Batch14-F41。
 *
 * 接替 remote-resume-cmd.ts（F09/F34）：那时远端 resume 只能给用户一条可粘贴的命令，
 * 「无注入面故不校验」；F41 起命令会被真的执行（wt.exe → `ssh -t … "<命令>"`），
 * 输入全部按不可信处理。本模块是 Batch 14 全部远端命令的单一来源（F52 tmux 版 /
 * F51 attach / F53 launcher 只在此追加 build 函数，转义与校验 helper 共享）。
 *
 * 防护清单（来源：android-terminal `RemoteCommands.kt` 实证 + cc-monitor issue #24）：
 * - 嵌套 env：resume 载荷前防御性 `unset` Claude 嵌套标记——若继承（tmux server env
 *   可能带毒），Claude 自认嵌套子会话 → 静默不写 JSONL，阅读器全瞎。
 *   **`CLAUDE_CONFIG_DIR` 必须保留**（定位数据目录），不在列表里。
 * - sid 白名单：拼进命令的 sessionId 只允许 `[A-Za-z0-9_-]{1,128}` 且拒前导 `-`。
 * - launcher denylist：用户自配命令（cc/cct/带参形态）**刻意不 quote**（要被交互
 *   shell 解释——别名/函数/带参），只挡命令串联/展开/重定向真注入向量
 *   （`;` `|` `&` `$` 反引号 `>` `<` 换行回车），命中 fail-closed 回退 `claude`。
 * - cwd：POSIX 单引号包裹（内部 `'` → `'\''`）。
 *
 * 注意：launcher 里的**双引号**在一键拉起路径会被 Rust 层拒绝（PowerShell 5.1
 * native 传参畸变面）并自动回退复制命令；要一键请用单引号写法（如
 * `cc --allowedTools 'Bash(*)'`）。denylist 放行 `"` 是为了粘贴回退仍合法。
 */

/** Claude 嵌套会话环境标记（空格分隔，喂 `unset`）。CLAUDE_CONFIG_DIR 刻意不含。 */
export const CLAUDE_NESTED_ENV_VARS =
  "CLAUDECODE CLAUDE_CODE_ENTRYPOINT CLAUDE_CODE_SESSION_ID CLAUDE_CODE_CHILD_SESSION";

/** POSIX 单引号 quote：整体 `'…'` 包裹，内部 `'` 断开为 `'\''`。 */
export function posixQuote(s: string): string {
  return `'${s.replace(/'/g, `'\\''`)}'`;
}

/** sessionId 白名单（UUID 及其变体形态）。拒前导 `-`：防伪造 sid 注入选项
 * （如 `--dangerously-skip-permissions` 会被 claude 当参数吃掉）。 */
export function isValidSessionId(sid: string): boolean {
  return /^[A-Za-z0-9_][A-Za-z0-9_-]{0,127}$/.test(sid);
}

/**
 * launcher 净化：空白 → `claude`；含注入向量字符 → fail-closed 回退 `claude`
 * （放行引号/括号/星号/方括号等合法参数字符）。
 */
export function sanitizeRemoteLauncher(cmd: string | undefined): string {
  const c = (cmd ?? "").trim();
  if (!c) return "claude";
  if (/[;|&$`<>\r\n]/.test(c)) return "claude";
  return c;
}

/**
 * 直连 resume 命令（F41）：`unset <嵌套env>; [cd '<cwd>' && ]<launcher> --resume <sid>`。
 * 经 `ssh -t user@host -- "<此串>"` 在远端登录 shell 里执行；同一文本也用于
 * 拉起失败时的剪贴板回退（粘贴到任何远端终端语义一致）。
 * sid 非法 → throw（调用方 toast 报错，绝不拼进命令）。
 */
export function buildResumeDirectCmd(sid: string, cwd: string, launcher = "claude"): string {
  if (!isValidSessionId(sid)) {
    throw new Error(`非法 sessionId（拒绝拼入命令）: ${JSON.stringify(sid)}`);
  }
  const resume = `${sanitizeRemoteLauncher(launcher)} --resume ${sid}`;
  const prefix = `unset ${CLAUDE_NESTED_ENV_VARS}; `;
  const c = cwd.trim();
  if (!c) return prefix + resume;
  return `${prefix}cd ${posixQuote(c)} && ${resume}`;
}

/**
 * F48：「在此打开终端」远端命令——进入指定 cwd 的登录交互 shell。
 * 经 wt.exe 起 `ssh -t … "bash -lic '<此串>'"`(launch.rs 传输包装)落到该目录。
 * cwd 空 → 不 cd(登录默认目录)。POSIX 单引号防路径含空格/特殊字符。
 */
export function buildOpenTerminalCmd(cwd: string): string {
  const c = cwd.trim();
  // exec 一个交互 login shell 保用户默认 shell。**不用双引号**——launch.rs 拒绝含双引号的
  // remote_cmd(PS native 传参畸变防线);$SHELL 是路径无空格,裸展开即可。
  const shell = "exec ${SHELL:-bash} -l";
  return c ? `cd ${posixQuote(c)} && ${shell}` : shell;
}

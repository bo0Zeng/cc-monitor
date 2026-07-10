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
 * F52：tmux 版 resume。在远端 tmux 会话 `cc-<sid8>` 里幂等 resume Claude,resume 完人在那个
 * tmux 里(断线可 F51 attach 回来)。命令(调研 03 §2b):
 *   `tmux new-session -d -s <名>[ -c '<cwd>'] 2>/dev/null && tmux send-keys -t <名> '<载荷>' Enter; tmux attach -t <名>`
 *
 * **幂等 + 前台白名单结构性满足**:`new-session -d 2>/dev/null && send-keys`——会话已存在 →
 * new-session 失败、`2>/dev/null` 吞、`&&` 短路 → **跳过 send-keys**(不重复 resume)→ 只 attach;
 * 不存在 → 新建 → send-keys 键入 resume → attach。send-keys 只进**新建会话**的交互 shell 提示符,
 * 绝不打进已在跑 claude 的会话(否则 `claude --resume` 会被当输入)。
 *
 * **send-keys 而非直 exec**(§2c):直 exec 常找不到 claude(只在交互 shell PATH/别名)→ 会话立死。
 * **载荷**含 `unset <嵌套env>;`(tmux server env 可能带毒 issue #24)。**全程只用单引号**(launch.rs
 * 拒双引号);载荷整体 `posixQuote` 成 send-keys 的单一参数,整条再由 launch.rs `bash -lic` 包装。
 * sid 非法 → throw。
 */
export function buildResumeTmuxCmd(sid: string, cwd: string, launcher = "claude"): string {
  if (!isValidSessionId(sid)) {
    throw new Error(`非法 sessionId（拒绝拼入命令）: ${JSON.stringify(sid)}`);
  }
  // sid 已过白名单 [A-Za-z0-9_-],前 8 位作 tmux 会话名(tmux 名不许 `.`/`:`,此字符集安全)。
  const name = `cc-${sid.slice(0, 8)}`;
  const payload = `unset ${CLAUDE_NESTED_ENV_VARS}; ${sanitizeRemoteLauncher(launcher)} --resume ${sid}`;
  const c = cwd.trim();
  const cflag = c ? ` -c ${posixQuote(c)}` : "";
  return (
    `tmux new-session -d -s ${name}${cflag} 2>/dev/null && ` +
    `tmux send-keys -t ${name} ${posixQuote(payload)} Enter; ` +
    `tmux attach -t ${name}`
  );
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

/**
 * F51:tmux 会话名合法性——非空、无控制字符(含 TAB 0x09 / 换行,防破坏 ls 解析或命令结构)、
 * ≤128。允许空格等可打印字符(`posixQuote` 会安全包裹)。名来自远端 `tmux ls` 输出(半可信),
 * 此为拼进命令前的防线;真正的注入边界是 `posixQuote`。
 */
export function isValidTmuxName(name: string): boolean {
  // eslint-disable-next-line no-control-regex
  return name.length > 0 && name.length <= 128 && !/[\x00-\x1f\x7f]/.test(name);
}

/**
 * F51 attach:`tmux attach -t '<name>'`。经 wt.exe `ssh -t … "bash -lic '<此串>'"`(launch.rs
 * 传输包装)落地,`ssh -t` 提供 attach 必需的 PTY。attach 只进已有会话、不启动 claude → 无需
 * unset 嵌套 env / launcher / sid。name 非法 → throw(调用方 toast,绝不拼入命令)。
 */
export function buildAttachCmd(name: string): string {
  if (!isValidTmuxName(name)) {
    throw new Error(`非法 tmux 会话名(拒绝拼入命令): ${JSON.stringify(name)}`);
  }
  return `tmux attach -t ${posixQuote(name)}`;
}

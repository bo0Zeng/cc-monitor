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

// F-MA：agent 画像是纯常量模块（无 DOM/render/bundler 依赖），不破坏本模块"零 bundler-import
// 便于 tsx 单测"的性质（同 diff.ts）。resume 相关 CC 常量（嵌套 env / launcher / --resume）在此。
import { AGENT_PROFILE } from "./agent-profile.ts";
// F90（#48 / SS-12 / INVARIANTS §31）：会话后端命令语法归 `session-backend.ts` 座——本模块不再
// 硬编码 `tmux …` 命令字面量，只留校验/转义/载荷/编排，命令语法问 `SESSION_BACKEND` 要。
import { SESSION_BACKEND } from "./session-backend.ts";

/** Claude 嵌套会话环境标记（空格分隔，喂 `unset`）。CLAUDE_CONFIG_DIR 刻意不含。 */
export const CLAUDE_NESTED_ENV_VARS = AGENT_PROFILE.nestedEnvVars.join(" ");

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
  if (!c) return AGENT_PROFILE.defaultLauncher;
  if (/[;|&$`<>\r\n]/.test(c)) return AGENT_PROFILE.defaultLauncher;
  return c;
}

/**
 * A4：CLAUDE_CONFIG_DIR 白名单。必须是绝对路径、无 `..` 段、无任何 shell 元字符/
 * 控制符/可欺骗 Unicode（与 daemon 侧 `is_safe_config_dir` 对齐）。fail-closed：
 * 稍有可疑即判非法，绝不拼进远端命令。
 */
export function isValidConfigDir(dir: string): boolean {
  if (!dir.startsWith("/")) return false;
  if (dir === "/" || dir.includes("/../") || dir.endsWith("/..")) return false;
  // shell 元字符 / 引号 / 控制符（C0 + DEL + C1，对齐 daemon Rust char::is_control）——一律拒
  if (/['"\\`$;|&<>*?()!\u0000-\u001f\u007f-\u009f]/.test(dir)) return false;
  // 可欺骗 Unicode（零宽 / 双向控制 / NBSP / BOM 等；NEL \u0085 已含在上面 C1 区）——一律拒
  if (/[\u00a0\u200b-\u200f\u2028\u2029\u202a-\u202e\u2066-\u2069\ufeff]/.test(dir)) return false;
  return true;
}

/**
 * A4：账号前缀。空 configDir → `""`（与旧载荷逐字节相同，保证"无账号=旧行为"）。
 * 非空则校验后 `export CLAUDE_CONFIG_DIR='<dir>'; `（posixQuote 包裹，前缀拼在 unset 之前）。
 * 非法即 throw（调用方 toast 报错，绝不拼进命令）。
 */
export function buildEnvPrefix(configDir?: string): string {
  if (!configDir) return "";
  if (!isValidConfigDir(configDir)) {
    throw new Error(`非法 CLAUDE_CONFIG_DIR（拒绝拼入命令）: ${JSON.stringify(configDir)}`);
  }
  return `export CLAUDE_CONFIG_DIR=${posixQuote(configDir)}; `;
}

/**
 * A4/F03：resume 载荷单一来源（tmux create 版 `buildResumeTmuxCmd` 与 idle 就地复用版
 * `buildResumeIntoExistingTmuxCmd` 共用，防两处漂移）：`[<账号前缀>]unset <嵌套env>; <launcher> --resume <sid>`。
 * 账号前缀空 configDir → ""（与旧载荷逐字节相同）。sid 校验由调用方在拼名/拼命令处兜底。
 */
function buildResumePayload(sid: string, launcher: string, configDir?: string): string {
  return `${buildEnvPrefix(configDir)}unset ${CLAUDE_NESTED_ENV_VARS}; ${sanitizeRemoteLauncher(launcher)} ${AGENT_PROFILE.resumeFlag} ${sid}`;
}

/**
 * 直连 resume 命令（F41）：`unset <嵌套env>; [cd '<cwd>' && ]<launcher> --resume <sid>`。
 * 经 `ssh -t user@host -- "<此串>"` 在远端登录 shell 里执行；同一文本也用于
 * 拉起失败时的剪贴板回退（粘贴到任何远端终端语义一致）。
 * sid 非法 → throw（调用方 toast 报错，绝不拼进命令）。
 */
export function buildResumeDirectCmd(
  sid: string,
  cwd: string,
  launcher = AGENT_PROFILE.defaultLauncher,
  configDir?: string,
): string {
  if (!isValidSessionId(sid)) {
    throw new Error(`非法 sessionId（拒绝拼入命令）: ${JSON.stringify(sid)}`);
  }
  const resume = `${sanitizeRemoteLauncher(launcher)} ${AGENT_PROFILE.resumeFlag} ${sid}`;
  // A4：账号前缀在 unset 之前（空 configDir → "" → 与旧载荷逐字节相同）。
  const prefix = `${buildEnvPrefix(configDir)}unset ${CLAUDE_NESTED_ENV_VARS}; `;
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
export function buildResumeTmuxCmd(
  sid: string,
  cwd: string,
  launcher = AGENT_PROFILE.defaultLauncher,
  name?: string,
  configDir?: string,
): string {
  if (!isValidSessionId(sid)) {
    throw new Error(`非法 sessionId（拒绝拼入命令）: ${JSON.stringify(sid)}`);
  }
  // 默认 `cc-<sid8>`;**F74** 灰会话 resume 传入不撞名(`pickFreshTmuxName`,避免复用被 /branch
  // 漂移占着的 `cc-<sid8>`)。名裸拼进 tmux 目标(不 posixQuote),必须无 shell/tmux 保留字符——
  // 只允许 `[A-Za-z0-9_-]`(`cc-<sid8>[-N]` 恒满足;外部传入非法即拒,防注入)。
  const tmuxName = name ?? `cc-${sid.slice(0, 8)}`;
  // 首字符不许 `-`(否则 `tmux -t -x` 把名当选项吃掉,arg 混淆;对齐 isValidSessionId 拒前导 -),
  // 其余 [A-Za-z0-9_-]。`cc-<sid8>[-N]` 恒满足;外部传入非法即拒(防注入 + 防 arg 混淆)。
  if (!/^[A-Za-z0-9_][A-Za-z0-9_-]*$/.test(tmuxName)) {
    throw new Error(`非法 tmux 会话名（拒绝拼入命令）: ${JSON.stringify(tmuxName)}`);
  }
  // A4：账号前缀在 unset 之前（空 configDir → "" → 与旧载荷逐字节相同，#72 @ccm_sid 正交不受影响）。
  const payload = buildResumePayload(sid, launcher, configDir);
  const c = cwd.trim();
  // 命令语法归后端座（SS-12 §31）。target 裸拼（`cc-<sid8>[-N]` 已过 `[A-Za-z0-9_-]` 校验）。
  // #72：把**完整 sid**当 `@ccm_sid` 传给座——resume 编排自建会话带身份,cc-monitor 之后
  // `findClaudeTmux` 精确命中(不落 cwd 回退警告)。sid 已过 `isValidSessionId`（[A-Za-z0-9_-]），裸拼安全。
  return SESSION_BACKEND.createRunAttach({
    target: tmuxName,
    quotedCwd: c ? posixQuote(c) : null,
    quotedPayload: posixQuote(payload),
    ccmSid: sid,
  });
}

/**
 * F03（idle-tmux 就地复用）：往一个**已存在的空 tmux**（claude 已退、只剩交互 shell 的 `cc-<sid8>`，
 * `@ccm_sid` 命中但 command≠claude）就地 resume——send-keys 载荷 + attach，**不 new-session**。
 * 复用原会话名 = 不产 `cc-<sid8>-N` 孤儿（治 #76 根因）；且修 create-gate 在会话已存在时短路跳过
 * send-keys、把用户 attach 进没起 claude 的空 shell（#75 一条）。载荷与 create 版共用 `buildResumePayload`。
 * **基座（无 configDir）时前置 `unset CLAUDE_CONFIG_DIR;`**：清掉空 shell 可能残留的旧账号 env
 * （避免在错账号数据目录 resume——#75 的复用变体）；账号复用则由载荷里的 export 覆盖。
 * sid / name 非法 → throw（绝不拼进命令）。
 */
export function buildResumeIntoExistingTmuxCmd(
  sid: string,
  name: string,
  launcher = AGENT_PROFILE.defaultLauncher,
  configDir?: string,
): string {
  if (!isValidSessionId(sid)) {
    throw new Error(`非法 sessionId（拒绝拼入命令）: ${JSON.stringify(sid)}`);
  }
  // 复用现有会话名（来自 list_remote_tmux），仍防御性校验：首字符非 `-`，其余 `[A-Za-z0-9_-]`。
  if (!/^[A-Za-z0-9_][A-Za-z0-9_-]*$/.test(name)) {
    throw new Error(`非法 tmux 会话名（拒绝拼入命令）: ${JSON.stringify(name)}`);
  }
  const envReset = configDir ? "" : "unset CLAUDE_CONFIG_DIR; ";
  const payload = envReset + buildResumePayload(sid, launcher, configDir);
  return SESSION_BACKEND.runInExistingAttach({ target: name, quotedPayload: posixQuote(payload) });
}

/**
 * F74：给灰会话 resume 挑一个**不撞现有 tmux 名**的会话名。基名 `cc-<sid8>`;被占(多半是
 * 被 `/branch` 漂移后仍占着原名的会话)→ 加数字后缀 `cc-<sid8>-2/-3/…` 取第一个空位。保证
 * resume 一定新建自己的 tmux 跑 `--resume <sid>` → 落进原会话,绝不 attach 进漂移的别人。
 * (纯函数,`existing` = 当前 tmux 会话名集合;sid 合法性由 `buildResumeTmuxCmd` 兜底校验。)
 */
export function pickFreshTmuxName(sid: string, existing: Set<string>): string {
  const base = `cc-${sid.slice(0, 8)}`;
  if (!existing.has(base)) return base;
  let i = 2;
  while (existing.has(`${base}-${i}`)) i += 1;
  return `${base}-${i}`;
}

/**
 * F53:从工作目录派生一个默认 tmux 会话名——basename(去尾 `/`)→ 非 `[A-Za-z0-9_-]` 换 `-`、
 * 折叠连字符、截 32 → `cc-<safe>`;空 → `cc-session`。「开新 Claude」弹框留空会话名时用它。
 */
export function deriveTmuxName(cwd: string): string {
  const base = cwd.trim().replace(/\/+$/, "").split("/").pop() ?? "";
  const safe = base
    .replace(/[^A-Za-z0-9_-]/g, "-")
    .replace(/-+/g, "-")
    .slice(0, 32)
    .replace(/^-+|-+$/g, ""); // 截断后再剥首尾 `-`,避免第 32 位恰为 `-` 留尾
  return safe ? `cc-${safe}` : "cc-session";
}

/**
 * F53:「在这台机开新 Claude」——启动**全新**会话(非 resume/attach)。复用 F52 的 tmux
 * create-gate 幂等构造,但载荷是启动命令(无 `--resume`):`unset <嵌套env>; <command>`。
 * 同名会话已存在 → new-session 失败被 `2>/dev/null` 吞 → `&&` 短路跳过 send-keys → 只 attach
 * (幂等:重复「开新」同名不重开、直接接回)。tmuxName 用户可任意(过 `isValidTmuxName` 拒
 * 空/控制字符/超长),**posixQuote 嵌入**(允许空格等,区别于 F52 定长 `cc-<sid8>` 裸用)。
 * command 过 `sanitizeRemoteLauncher`(denylist fail-closed `claude`)。名非法 → throw。
 */
export function buildLauncherCmd(
  cwd: string,
  tmuxName: string,
  command = AGENT_PROFILE.defaultLauncher,
  configDir?: string,
): string {
  const name = tmuxName.trim();
  if (!isValidTmuxName(name)) {
    throw new Error(`非法 tmux 会话名（拒绝拼入命令）: ${JSON.stringify(name)}`);
  }
  const qname = posixQuote(name);
  // A4：账号前缀在 unset 之前（空 configDir → "" → 与旧载荷逐字节相同）。
  const payload = `${buildEnvPrefix(configDir)}unset ${CLAUDE_NESTED_ENV_VARS}; ${sanitizeRemoteLauncher(command)}`;
  const c = cwd.trim();
  // 命令语法归后端座（SS-12 §31）。target 用 posixQuote 名（F53 允许空格等，区别于 F52 定长裸名）。
  return SESSION_BACKEND.createRunAttach({
    target: qname,
    quotedCwd: c ? posixQuote(c) : null,
    quotedPayload: posixQuote(payload),
  });
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
 * **无 tmux 保留字符 `.`/`:`**(它们是 `session:window.pane` 目标分隔符,new-session 会拒)、≤128。
 * 允许空格等其余可打印字符(`posixQuote` 会安全包裹)。真正的注入边界是 `posixQuote`;此校验
 * 兼防运行时 tmux 报错(F53 把会话名开成用户自由输入后,`.`/`:` 会静默失败,故在此拦)。
 */
export function isValidTmuxName(name: string): boolean {
  // eslint-disable-next-line no-control-regex
  return name.length > 0 && name.length <= 128 && !/[.:\x00-\x1f\x7f]/.test(name);
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
  // 命令语法归后端座（SS-12 §31）；target 用 posixQuote 名。
  return SESSION_BACKEND.attach(posixQuote(name));
}

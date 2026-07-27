/**
 * shell 转义 / 校验原语（纯函数，零依赖叶子模块）。
 *
 * F03（LaunchPlan IR）从 `remote-launch.ts` 搬出——渲染器（`launch-render-fallback.ts`）
 * 需要复用这些原语，而 `remote-launch.ts` 又需要复用渲染器，若原语留在 `remote-launch.ts`
 * 会造成 `remote-launch.ts → launch-render-fallback.ts → remote-launch.ts` 的运行时循环
 * import。本模块零 import、零副作用，是拓扑序的根，两边都能安全依赖它。
 *
 * `remote-launch.ts` 对外仍 `export {...} from "./shell-quote.ts"` 原样透出这五个符号
 * ——`remote-launch.test.ts` 等既有 import 面零改动。
 */
import { AGENT_PROFILE } from "./agent-profile.ts";

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
 * F07（unify-launch）：模型名白名单——覆盖"claude-opus-4-5-20260101"这类完整 ID 与"opus"这类
 * 简写别名，拒一切 shell 元字符。只做注入安全校验，不做"这是不是真实存在的模型"的语义校验
 * （远端 `claude` 自己会在模型名不存在时报错，那是它的职责）。
 */
export function isValidModelName(name: string): boolean {
  return /^[A-Za-z0-9._-]{1,128}$/.test(name);
}

/**
 * F51:tmux 会话名合法性——非空、无控制字符(含 TAB 0x09 / 换行,防破坏 ls 解析或命令结构)、
 * **无 tmux 保留字符 `.`/`:`**(它们是 `session:window.pane` 目标分隔符,new-session 会拒)、
 * **无 glob 元字符 `*`/`?`**(见下)、≤128。
 * 允许空格等其余可打印字符(`posixQuote` 会安全包裹)。真正的注入边界是 `posixQuote`;此校验
 * 兼防运行时 tmux 报错(F53 把会话名开成用户自由输入后,`.`/`:` 会静默失败,故在此拦)。
 *
 * **本函数刻意不禁 glob 字符**(`*`/`?`)——见 `isValidNewTmuxName`。它同时把守 `buildAttachCmd`,
 * 而那条路径的输入是 `list_remote_tmux` 列出的**用户自己已存在的会话名**(tabs.ts 的 attach 项)。
 * tmux 允许 `st*ar` 这类名字;在此禁掉只会把「attach 到这类已存在会话」从可用变成 throw,
 * 而**挡不住任何东西**——`exactTarget` 的 `=name:` 已经把 glob 这一级彻底关闭(实测
 * `-t '=st*ar:'` rc=0 且精确命中)。D 审计判定为行为回归,故拆成两个谓词。
 */
export function isValidTmuxName(name: string): boolean {
  // eslint-disable-next-line no-control-regex
  return name.length > 0 && name.length <= 128 && !/[.:\x00-\x1f\x7f]/.test(name);
}

/**
 * F01 第二道防线:**创建**新会话时额外禁 glob 元字符 `*`/`?`。
 *
 * tmux 的 `-t` 解析含 **glob** 一级——实测(tmux 3.6)`kill-session -t 'a*a'` 会命中并杀掉 `alpha`。
 * 第一道防线是 `session-backend.ts` 的 `exactTarget()`(`=name:` 强制精确);此处是第二道:
 * **本工具永远不把 glob 字符建进会话名**,于是即便将来某条路径漏了精确前缀也炸不出 glob 误伤。
 *
 * **只用在创建路径**(`buildLauncherCmd`/`planLauncher`)。attach 已有会话走 `isValidTmuxName`——
 * 那些名字不是我们建的,禁它既无收益又是回归(见上)。二者独立、职责不同。
 * (Rust 侧 `is_ccm_tmux_name` 的字符集今天顺带挡住这一面,但那是**身份**白名单、F04 会重构它,
 * 不能依赖它兼职做字符集防线。)
 */
export function isValidNewTmuxName(name: string): boolean {
  return isValidTmuxName(name) && !/[*?]/.test(name);
}

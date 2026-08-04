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
// F03：shell 转义/校验原语搬进零依赖叶子模块（防 remote-launch.ts ↔ launch-render-fallback.ts
// 运行时循环 import）。原样重新导出——本文件既有 import 面（remote-launch.test.ts 等）零改动。
import {
  posixQuote,
  isValidSessionId,
  sanitizeRemoteLauncher,
  isValidConfigDir,
  buildEnvPrefix,
  UNSET_CONFIG_DIR_PREFIX,
  isValidTmuxName,
  isValidNewTmuxName,
  tmuxNameSegment,
} from "./shell-quote.ts";
export {
  posixQuote,
  isValidSessionId,
  sanitizeRemoteLauncher,
  isValidConfigDir,
  buildEnvPrefix,
  UNSET_CONFIG_DIR_PREFIX,
  isValidTmuxName,
  isValidNewTmuxName,
  tmuxNameSegment,
};
// F03：7 个 builder 的意图构造 + 校验逐字搬进 launch-requests.ts（LaunchContext/LaunchPlan
// 翻译层）；本文件的每个导出现在只是「调那边 + 交渲染器」的薄适配器，位置参数签名逐字不变
// （e2e/resume-cmd-driver.ts 直接 import 这几个符号，e2e/restart-cmd-driver.ts 经
// account-restart.ts 传递性锁死 runRemoteResumeTmux 的签名）。
import { renderFallback } from "./launch-render-fallback.ts";
import {
  planResumeDirect,
  planResumeTmux,
  planResumeIntoExistingTmux,
  planLauncher,
  planAttach,
} from "./launch-requests.ts";

/** Claude 嵌套会话环境标记（空格分隔，喂 `unset`）。CLAUDE_CONFIG_DIR 刻意不含。 */
export const CLAUDE_NESTED_ENV_VARS = AGENT_PROFILE.nestedEnvVars.join(" ");

// U8c-2a：`buildUsageProbePayload` **已退役** —— 用量探针的载荷改由 Rust 内核
// `backend::control::payload::usage_probe_payload` 编译（账本 S28 的第 ② 份产出点就此消失）。
// 那条「两态、绝不裸载荷、空串是坏数据」的 fail-closed 纪律原样搬了过去并有测试；
// 前端只报 `configDir`（`null` = 账号 0）。

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
  return renderFallback(planResumeDirect(sid, cwd, launcher, { configDir }).plan);
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
 * sid 非法 → throw。
 */
export function buildResumeTmuxCmd(
  sid: string,
  cwd: string,
  launcher = AGENT_PROFILE.defaultLauncher,
  // F13：`name` 改必填 —— 会话名必须由调用方过 `mintTmuxName` 铸出来，
  // 本函数是纯渲染器，拿不到「现有会话名集合」，也就无从避让。
  name: string,
  configDir?: string,
): string {
  return renderFallback(planResumeTmux(sid, cwd, launcher, name, { configDir }).plan);
}

/**
 * F03（idle-tmux 就地复用）：往一个**已存在的空 tmux**（claude 已退、只剩交互 shell 的 `<sid8>-cc`，
 * `@ccm_sid` 命中但 command≠claude）就地 resume——send-keys 载荷 + attach，**不 new-session**。
 * 复用原会话名 = 不产 `<sid8>-cc-N` 孤儿（治 #76 根因）；且修 create-gate 在会话已存在时短路跳过
 * send-keys、把用户 attach 进没起 claude 的空 shell（#75 一条）。
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
  return renderFallback(planResumeIntoExistingTmux(sid, name, launcher, { configDir }).plan);
}

/**
 * F74：给灰会话 resume 挑一个**不撞现有 tmux 名**的会话名。基名 `<sid8>-cc`;被占(多半是
 * 被 `/branch` 漂移后仍占着原名的会话)→ 加数字后缀 `<sid8>-cc-2/-3/…` 取第一个空位。保证
 * resume 一定新建自己的 tmux 跑 `--resume <sid>` → 落进原会话,绝不 attach 进漂移的别人。
 * (纯函数,`existing` = 当前 tmux 会话名集合;sid 合法性由 `buildResumeTmuxCmd` 兜底校验。)
 */
/**
 * F13（用户 2026-08-03：「为什么会撞名? 要撞名检查」「所有的东西都要集成整合成一条路径」）：
 * **tmux 会话名的唯一铸造口** —— 给一个基名，回一个**不撞现有名**的最终名。
 *
 * # 为什么「产名」与「避让」必须是同一个函数
 *
 * 摸底量到撞名的根因**不是**忘了检查，是**两件事被拆开了**：
 * 五个产出点里只有两个带避让，而**带避让的那个避让的正好是不带避让的那个会产的名字**
 * （`pickFreshTmuxName` 精心让出 `<sid8>-cc-2`，而 `launch-requests` 的默认值直接产
 * `<sid8>-cc` 撞上去）。原注释只钉了「基名字符串相同，别只改一边」，
 * **没钉「避让也要相同」**。
 *
 * ⇒ 收成一个函数，且 **`existing` 是必填参数、没有默认值**。
 * 默认成空集就等于「有检查的样子、没有检查的事实」——
 * `forkTmuxName` 此前正是 `taken: readonly string[] = []`，那个默认值让它的避让形同虚设。
 * **这条由 `tsc` 在编译期钉住**（少传一个参数就编不过），不靠自觉。
 *
 * 撞名后缀**追加在最后**（`<base>-2/-3`）—— 让「第几个」始终是名字的末段，
 * 读起来是「哪个会话的第几份」。
 */
export function mintTmuxName(base: string, existing: ReadonlySet<string>): string {
  if (!existing.has(base)) return base;
  let i = 2;
  while (existing.has(`${base}-${i}`)) i += 1;
  return `${base}-${i}`;
}

export function pickFreshTmuxName(sid: string, existing: Set<string>): string {
  // S4b-3b：命名从 `cc-<X>` 反转成 `<X>-cc`（用户 2026-07-31）。
  // F13：避让那一半搬进 `mintTmuxName` —— **全仓唯一的铸造口**，别在这里重写一遍。
  return mintTmuxName(`${sid.slice(0, 8)}-cc`, existing);
}

/**
 * F53:从工作目录派生一个默认 tmux 会话名——basename(去尾 `/`)→ 非 `[A-Za-z0-9_-]` 换 `-`、
 * 折叠连字符、截 32 → `<safe>-cc`;空 → `session-cc`。「开新 Claude」弹框留空会话名时用它。
 *
 * **S4b-3b（用户 2026-07-31）：`cc-` 前缀改成 `-cc` 后缀。** 与 `shared/ccm::derive_tmux_name`
 * 逐字同规则（跨语言双写点，由 `e2e/ccm-cli.test.sh` 的真值对拍钉住，见 E49）。
 *
 * ⚠ **F13 定位：它只产「基名建议」，不产最终名。** 最终名一律过 [`mintTmuxName`]
 * （那里才有撞名避让）。摸底实测：`machine-card` 的「开新 Claude」此前直接拿它当最终名
 * ⇒ 同一个 cwd 点两次「开始」会产出同名，撞上 create-or-attach 的幂等闸
 * ⇒ **静默接进第一个会话，而用户以为开了新的**（issue #76 那一族）。
 */
export function deriveTmuxName(cwd: string): string {
  // 净化那一段已抽进 `shell-quote.ts::tmuxNameSegment`——`forkTmuxName` 要用同一份
  // （它此前另写了一份不净化的，Phase G 当场抓出：cwd 当基名会产出非法的 `/a/b-fork-cc`）。
  const safe = tmuxNameSegment(cwd);
  return safe ? `${safe}-cc` : "session-cc";
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
  return renderFallback(planLauncher(cwd, tmuxName, command, { configDir }).plan);
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
 * F51 attach:`tmux attach -t '<name>'`。经 wt.exe `ssh -t … "bash -lic '<此串>'"`(launch.rs
 * 传输包装)落地,`ssh -t` 提供 attach 必需的 PTY。attach 只进已有会话、不启动 claude → 无需
 * unset 嵌套 env / launcher / sid。name 非法 → throw(调用方 toast,绝不拼入命令)。
 */
export function buildAttachCmd(name: string): string {
  return renderFallback(planAttach(name).plan);
}

/**
 * 起会话输入的校验原语（纯函数，零依赖叶子模块）：模型名 · tmux 会话名。
 *
 * 〔LR2〕这里原来还有三件**拼 shell 串**的东西：`posixQuote`（单引号包裹）、`buildEnvPrefix`
 * （`export CLAUDE_CONFIG_DIR='…'; `）与 `UNSET_CONFIG_DIR_PREFIX`。它们只给 TS 兜底渲染器
 * （`launch-render-fallback.ts` ＋ `session-backend.ts`）用；那一族零生产调用、按 `设计/00 §2.5 ④` 删了，
 * 三件随之删 —— 拼串今天只在 Rust（`shell_quote_core::posix_quote` · `payload.rs`），
 * 前端零 shell 串（`设计/90 §3` 条 1，判据 `tests/launch-no-shell-in-ts.vitest.ts`）。
 * 留下的只做**校验**（拒绝拼入命令），真正拼进命令的那一步不在这里。
 *
 * 〔DUP1 · `设计/90 §3` 判据 2〕这里原来还有两个校验器，Rust 渲染侧各有一份同一条规则、渲染时自己判：
 * `isValidConfigDir`〔散文墓碑〕（＝ `payload.rs::config_dir_command_safe`，字符集逐项同）与
 * `sanitizeRemoteLauncher`〔散文墓碑〕（同 `payload.rs::render_payload` 那道闸的字符集，但它**静默换成 `claude`**，
 * 撞 `01 §5` D4）。两份删了：线上校验交渲染那一侧判，判不过带 `REFUSE:` 标拒、前端说出来，不回落。
 * 登记表 `tests/judgment-single-home.vitest.ts`（J2 · J3）。
 * 〔DUP1 · 第二轮〕`isValidSessionId`〔散文墓碑〕同理删了（J5）：sid 规则只有一份，住 `shell_quote_core::session_id_ok`，
 * 渲染侧（载荷 · 外层 · `ccm …` 调用行 · 本机拉起）与后端 ccm 各自在拼进命令之前判。
 */

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
 * 允许空格等其余可打印字符(引号由 Rust 渲染侧 `shell_quote_core::posix_quote` 安全包裹,那才是注入边界);此校验
 * 兼防运行时 tmux 报错(F53 把会话名开成用户自由输入后,`.`/`:` 会静默失败,故在此拦)。
 *
 * **本函数刻意不禁 glob 字符**(`*`/`?`)——见 `isValidNewTmuxName`。它同时把守 attach 那条(`planAttach`),
 * 而那条路径的输入是 `list_remote_tmux` 列出的**用户自己已存在的会话名**(tabs.ts 的 attach 项)。
 * tmux 允许 `st*ar` 这类名字;在此禁掉只会把「attach 到这类已存在会话」从可用变成 throw,
 * 而**挡不住任何东西**——渲染侧 `-t` 一律用的 `=name:` 已经把 glob 这一级彻底关闭(实测
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
 * 第一道防线是渲染侧 `-t` 一律 `=name:` 强制精确(今天在 Rust `payload.rs` 的外层那三格);此处是第二道:
 * **本工具永远不把 glob 字符建进会话名**,于是即便将来某条路径漏了精确前缀也炸不出 glob 误伤。
 *
 * **只用在创建路径**(`planLauncher`)。attach 已有会话走 `isValidTmuxName`——
 * 那些名字不是我们建的,禁它既无收益又是回归(见上)。二者独立、职责不同。
 * (Rust 侧 `is_ccm_tmux_name` 的字符集今天顺带挡住这一面,但那是**身份**白名单、F04 会重构它,
 * 不能依赖它兼职做字符集防线。)
 */
/**
 * tmux 会话名里**一段**的净化（不含 `-cc` 后缀）。
 *
 * **为什么要抽出来**（Phase G 审计当场抓的一个阻塞）：`deriveTmuxName` 一直在做这件事，
 * 而 `fork-launch.ts::forkTmuxName` 后来**另写了一份不做净化的**版本 —— 于是「源会话已退出
 * ⇒ 拿 cwd 当基名」这条路会产出 `/home/pi/proj-fork-cc`，被 `planResumeTmux` 的
 * `/^[A-Za-z0-9_][A-Za-z0-9_-]*$/` 当场拒掉。两处共用同一个净化器，那条路就不可能再产非法名。
 *
 * 规则与 `shared/ccm::derive_tmux_name` 逐字同源（跨语言双写点，由 `tests/e2e/ccm-cli.test.sh`
 * 的真值对拍钉住）：取末段路径 → 非 `[A-Za-z0-9_-]` 换 `-` → 折叠连字符 → 截 32 → 剥首尾 `-`。
 * 结果可能是**空串**（如输入全是分隔符），调用方负责给一个兜底名。
 */
export function tmuxNameSegment(raw: string): string {
  const base = raw.trim().replace(/\/+$/, "").split("/").pop() ?? "";
  return base
    .replace(/[^A-Za-z0-9_-]/g, "-")
    .replace(/-+/g, "-")
    .slice(0, 32)
    .replace(/^-+|-+$/g, ""); // 截断后再剥首尾 `-`，避免第 32 位恰为 `-` 留尾
}

/**
 * ★ F04b 追加 `=`：**别创建一个主路杀不掉的名字。**
 *
 * kill 的主路从 F04b 起走后端（`src/backend/control/kill.rs`），
 * 而它的形状门逐字拒绝 `:` 与 `=`（「它们是 tmux 目标语法」）。
 * `isValidTmuxName` 已经禁了 `:`，但 **`=` 是允许的** —— 于是一个像 `proj=x-cc`
 * 的名字**建得出来、却在主路上杀不掉**（backend 回 `invalid_args`）。
 *
 * 处置选的是「**改结构让问题不存在**」那一条（同仓 U3 的先例）：不在 kill 那边开特例
 * 回落到 shell 路（那等于把一次形状拒绝洗成另一条路的成功），而是**不让它被建出来**。
 * ⚠ `isValidTmuxName`（attach 已有会话那条）**刻意不跟着改** —— 那些名字不是我们建的，
 * 禁它只会把「attach 到一个已存在的 `a=b`」从可用变成 throw，挡不住任何东西。
 * 由 Rust 侧 `backend::control::backend_kill::the_creation_path_cannot_mint_a_name_the_main_path_cannot_kill`
 * 跨轨钉住。
 */
export function isValidNewTmuxName(name: string): boolean {
  return isValidTmuxName(name) && !/[*?=]/.test(name);
}

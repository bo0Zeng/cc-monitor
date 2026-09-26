/**
 * 起会话输入的纯函数（零依赖叶子模块）：tmux 会话名一段的净化（〔DUP2〕校验那两个谓词已删，见下）。
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
 * `isValidModelName`〔散文墓碑〕也删了（J17）：规则住 `shell_quote_core::model_name_ok`；设置里写入点那一句读生成物。
 */

// 〔DUP2 · `设计/90 §3` 判据 2 · 主会话 09-26 裁 J6〕这里原来有两个 tmux 会话名的谓词：`isValidTmuxName`〔散文墓碑〕（attach，
// 非空 · 无 `.:` 与 C0/DEL · ≤128）与 `isValidNewTmuxName`〔散文墓碑〕（新建，再禁 `*?=` —— F01 第二道防线 · F04b「别建一个主路杀不掉的名字」：
// kill 的主路（后端 `control/kill.rs`）逐字拒 `:` 与 `=`，建得出来的名字必须杀得掉，跨轨判据 `backend_kill_tests.rs` 钉着）。
// 今天名字的规则只有一份，住共享 crate `gate-core`（`new_tmux_name_issue` / `existing_tmux_name_issue`）：monitor 载荷外层 ·
// `ccm …` 调用行 · 后端 `ccm/plan.rs::validate_tmux_name` 都调它；判不过带 `REFUSE:` 标拒，前端照拒说出来。

/**
 * tmux 会话名里**一段**的净化（不含 `-cc` 后缀）。
 *
 * **为什么要抽出来**（Phase G 审计当场抓的一个阻塞）：`deriveTmuxName` 一直在做这件事，
 * 而 `fork-launch.ts::forkTmuxName` 后来**另写了一份不做净化的**版本 —— 于是「源会话已退出
 * ⇒ 拿 cwd 当基名」这条路会产出 `/home/pi/proj-fork-cc`，被当时 `planResumeTmux` 的内联式子当场拒掉
 * （〔DUP2〕今天由渲染侧的 gate-core 新建那一条拒）。两处共用同一个净化器，那条路就不可能再产非法名。
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

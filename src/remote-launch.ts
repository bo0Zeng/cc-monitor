/**
 * **tmux 会话名的铸造口**（对外名字见 `设计/80 §9.0`）。
 *
 * 〔LR2〕这份文件原来还有「远端拉起命令构造」那一半：五个 builder（resume 直起 · resume 进 tmux ·
 * 送进已有 tmux · 起新会话 · attach）加「在此打开终端」那一串。步 22b·B 之后生产那一行全由 Rust
 * `render_launch_payload` 渲染，它们零生产调用，照 `设计/00 §2.5 ④`「TS 的只供对拍、排期删」删了
 * （`80 §9.3` 原写「是退路、服务用户自己粘」—— 现打不成立：剪贴板回退复制的就是 Rust 渲出的那一串；
 * 主会话 09-25 裁删，`80 §9.1 / §9.3` 交文档路改）。输入校验（sid / 会话名 / launcher / configDir）
 * 一直住 `launch-requests.ts` ＋ `shell-quote.ts`，没有跟着走。
 *
 * 剩下的这一半 **一个字没动**（归 FE1「铸名只留一份」）：`mintTmuxName` 是全仓唯一带撞名避让的铸造口
 * （issue #76「同名会话静默接回」的唯一防线），`deriveTmuxName` 与后端 `ccm::plan::derive_tmux_name`
 * 逐字同规则（`tests/e2e/ccm-cli.test.sh` 真值对拍）。
 */
import { tmuxNameSegment } from "./shell-quote.ts";

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

/**
 * `K-R96`（用户 2026-09-12，`R55` 裁定一逐字：「**要是可读的名字 / 不要id**」）：
 * **起会话时那个 tmux 会话名 —— `<项目名>-cc`，撞了往后排。**
 *
 * # 🔴 它替掉了 `pickFreshTmuxName`，而且**换的不是名字，是入参**
 *
 * 从前那个函数的签名是 `(sid, existing)`，基名 `` `${sid.slice(0,8)}-cc` ``。
 * 那个名字在 tmux 列表里长成 `cb3230f3-cc` —— 用户看着一屏这种东西**认不出哪个是哪个**。
 *
 * ⇒ 基名改从 **cwd** 来（[`deriveTmuxName`]，与后端 `ccm::plan::derive_tmux_name`
 * 逐字同规则的那一份），于是同一件事在前后端读起来是同一个名字。
 * **入参从 `sid` 换成 `cwd`** 是刻意的：两个都是 `string`，只改函数体不改签名的话
 * 调用方会**静默**把 sid 当 cwd 传进来（派生出 `cb3230f3-dead-beef-cc` 这种四不像）。
 * 换了名字 ⇒ `tsc` 逼每一个调用点都被人看过一遍。
 *
 * # sid 去哪了 —— **它一直就不在名字里**
 *
 * sid 真正的载体是 tmux 的 **`@ccm_sid`** 选项（`ccm` 建会话时 `set-option` 写上去）。
 * 「按 `<sid8>-cc` 前缀认会话」这件事**本仓从来没有人做**：杀会话的菜单与身份判定
 * 都只问 `@ccm_sid`（`src/bridge/src/backend/control/tmux.rs` 那段逐字：「本条的两个消费者都不问那个」）。
 * ⇒ 名字里去掉 sid **不会**弄坏任何认领逻辑；而 `@ccm_sid` **一格都不许动**
 * （`KR96D3` 第四刀：把它一起去掉必须红）。
 *
 * # 避让仍然只有一个家
 *
 * 撞名后缀走 [`mintTmuxName`]（全仓唯一带避让的铸造口，`existing` 必填、没有默认值）。
 * ⇒ 产出 `<项目名>-cc` / 撞了 `<项目名>-cc-2` / 再撞 `-3`。
 * 这与后端 `ccm` 那条路（`plan::build` 拿 `TakenNames` 退让）**是同一条规则**。
 *
 * @param cwd  这条会话的工作目录；空 / 派生不出东西 ⇒ 回落 `session-cc`（同 `deriveTmuxName`）。
 * @param existing 当前已占用的 tmux 会话名集合。**「不知道」的时候别调本函数** ——
 *                 传空集 = 「一个都没占」，那是 issue #76 的形状（见 `tmux-name-mint.ts`：列不出 ⇒ 不铸名）。
 */
export function mintSessionTmuxName(cwd: string, existing: ReadonlySet<string>): string {
  return mintTmuxName(deriveTmuxName(cwd), existing);
}

/**
 * F53:从工作目录派生一个默认 tmux 会话名——basename(去尾 `/`)→ 非 `[A-Za-z0-9_-]` 换 `-`、
 * 折叠连字符、截 32 → `<safe>-cc`;空 → `session-cc`。「开新 Claude」弹框留空会话名时用它。
 *
 * **S4b-3b（用户 2026-07-31）：`cc-` 前缀改成 `-cc` 后缀。** 与 `shared/ccm::derive_tmux_name`
 * 逐字同规则（跨语言双写点，由 `tests/e2e/ccm-cli.test.sh` 的真值对拍钉住，见 E49）。
 *
 * ⚠ **F13 定位：它只产「基名建议」，不产最终名。** 最终名一律过 [`mintTmuxName`]
 * （那里才有撞名避让）。摸底实测：`machine-card` 的「开新 Claude」此前直接拿它当最终名
 * ⇒ 同一个 cwd 点两次「开始」会产出同名，撞上当时那个 `create-or-attach` 的幂等闸
 * （**该模式 `C14` 已删**：起会话不再吞 `new-session` 的错、不再无条件 attach。
 *  ⇒ 今天同名的后果从「静默接回」变成「**建失败、看得见**」，而铸名口正是为了让它不发生）
 * ⇒ **静默接进第一个会话，而用户以为开了新的**（issue #76 那一族）。
 */
export function deriveTmuxName(cwd: string): string {
  // 净化那一段已抽进 `shell-quote.ts::tmuxNameSegment`——`forkTmuxName` 要用同一份
  // （它此前另写了一份不净化的，Phase G 当场抓出：cwd 当基名会产出非法的 `/a/b-fork-cc`）。
  const safe = tmuxNameSegment(cwd);
  return safe ? `${safe}-cc` : "session-cc";
}

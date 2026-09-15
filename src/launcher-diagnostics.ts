/**
 * F08（unify-launch）：越层启动器诊断 + 别名生成器——放在同一个文件里，因为它们是同一个用户
 * 旅程的两半（诊断"你现在填的命令可能有问题" → 生成器"这是拼一条正确命令的工具"），Phase D
 * UX 审计发现这两半分居设置面板两处互不相邻的地方（诊断在"行为"分组、生成器曾在按主机重复
 * 渲染的"远端 (SSH)"每台机器卡片里）会让用户看到诊断提示后无路可循——已合并成同一处设置分组
 * 里紧邻的两块，不再按主机重复（生成器内容本来就与选中哪台机器无关）。
 *
 * MASTERPLAN 设计原则#7：越层启动器只诊断 + 引导迁移，绝不自动降级、**绝不在用户没要求时
 * 改他的配置**。
 *
 * 🔴 `K-R49`（09-10）**把这一句收窄了一格，别再按旧的读法撤掉下面那块活**。
 * 原句逐字是「绝不**偷改**用户配置」，而「偷改」与「用户按了一个写着『写入』的按钮」
 * 是两件事 —— 原则禁的是**自动、静默**：
 *
 * - **仍然禁**：任何在启动 / 后台 / 探测路径上，不经用户当次手势就写盘的动作；
 *   把用户填的启动器命令**自动**改掉、**自动**降级到另一个启动器；
 *   猜一份 shell 配置（`.bashrc`? `.zshrc`?）然后往里写。
 * - **不禁**：用户在界面上点了一个写明「写到哪、写什么」的按钮之后，把那件事做掉。
 *
 * 用户 09-10 逐字：「**我现在添加了一个账号但是没法直接添加命令, 还得手动去改**」——
 * 他要的正是「帮我改」。⇒ [`buildAccountAliasBlock`] 那一块**是这条原则的例外，
 * 而且是写在原则旁边的那一条**，不是对它的违反。它同时把代价压到最小：
 * 真正被重写的是 **cc-monitor 自己那份文件**（`~/.cc-monitor/account-aliases.sh`），
 * 用户的 shell 配置最多多**一行** `source`，而且那份 rc 由他自己在下拉里选。
 */
import { buildPasteBlock } from "./paste-block"; // T03：待贴文本统一组件
import { commands } from "./ipc/commands"; // K-R49：真落盘那一跳（后端围栏在 `account_aliases.rs`）
import { showActionFailureToast } from "./error-toast"; // `K-R135`：那一格的失败要出声
import type { AccountAliasReport } from "./generated/AccountAliasReport";
// `K-R62`：本机 POSIX 那一格（装别名块 / 查裸行）走的是 `cc_integration_*` 那三条命令，
// 它们的返回形状就是这一个生成物（源 `profile_installer.rs::ProfileScan`）。
import type { ProfileScan } from "./generated/ProfileScan";
// `K-R69` / `KR69D2`+`KR69D3`：本机那条 `ccm` 入口这一格（我们那一份 · PATH 上那一份 · 判词）。
import type { LocalCcmEntry } from "./generated/LocalCcmEntry";
// `K-R93`：**盘上有几个 agent、默认是哪个，都是后端的事实** —— 本文件从前把
// `claude` / `codex` 写死了两处（下拉清单 + 越层诊断的豁免名单）。
import { ACTIVE_AGENT, listAgents, lookupAgentProfile } from "./agent-profile";

/**
 * 🔴 `K-R69` / `KR69D3`：**生成出来的那一行，该调哪一份 `ccm`。**
 *
 * # 立件时这里是什么样
 *
 * [`buildAliasLine`] 一直吐**裸 `ccm`**，靠 PATH 解析 —— 而 `K-R69` 现打：
 * app **从来没有在本机装过 `ccm`** ⇒ 用户把这行贴进 rc 之后，解析到的仍然是他自己那份
 * 旧的（`~/.local/bin/ccm`，2026-07-27 的 bash）。**「靠 PATH 撞运气」不是修辞，是当时的机制。**
 *
 * # 现在的形状，以及为什么**不是**写死绝对路径
 *
 * 用户 09-11 明裁「这些命令都是可以自定义的」（`R19`）⇒ 写死会把自定义堵死，
 * 换台机器 / 换个用户也当场失效。⇒ 三档：
 *
 * | PATH 上那个是谁 | 吐什么 | 为什么 |
 * |---|---|---|
 * | **就是我们这一份**（`ours`） | 裸 `ccm` | 已经指得到了，没必要把路径塞进用户的 rc |
 * | **不是我们这一份 / PATH 上没有** | `"${CCM:-<我们那一份>}"` | 显式指向它，**同时留一个 `CCM` 环境变量的口子**给自定义 |
 * | **说不出**（我们那份没装 / 探不到） | 裸 `ccm` | 没资格替用户指路；这一格由 [`LocalCcmEntry.message`] 说成「查不了」 |
 *
 * ⚠ **它不是「猜」**：三档全由后端那次 `--ccm-probe` 握手的判词决定
 * （`ccm_probe::classify_path_ccm`，比的是身份不是路径）。
 */
export function ccmInvocation(status: LocalCcmEntry | null): string {
  // 没问到、我们那份没装下来、或者它自己都答不出 `--ccm-probe` ⇒ 不替用户指路。
  if (!status || !status.entry || !status.ours.installed) return "ccm";
  if (status.verdict === "ours") return "ccm";
  return `"\${CCM:-${status.entry}}"`;
}

/** 问一次本机 `ccm` 这一格。**问不到不许静默**：回一句话，由调用方原样上屏。 */
async function loadCcmEntry(): Promise<{
  status: LocalCcmEntry | null;
  error: string | null;
}> {
  try {
    return { status: await commands.local_ccm_entry_status(), error: null };
  } catch (e) {
    return { status: null, error: `问不到本机 ccm 这一格：${String(e)}` };
  }
}

/**
 * 后端认得的 agent 的**默认拉起二进制名**（`claude` / `codex` / …）。
 *
 * 🔴 `K-R93`：刻意**只收 `defaultLauncher`，不收 `launcherAlias`**。
 * 别名（claude 的 `cc`）是用户自己那层 shell wrapper —— 它与 `cct` / `oot` 是同一类东西，
 * 照样绕开 `ccm`，**不该被豁免**。这两个值住在同一行画像里，但它们回答的不是同一个问题。
 */
function baseLaunchers(): string[] {
  const out: string[] = [];
  for (const agent of listAgents()) {
    const got = lookupAgentProfile(agent);
    // 问不到就跳过、不猜 —— 少豁免一个只是多提示一句，编一个出来才是错的。
    if (got.known) out.push(got.facts.defaultLauncher);
  }
  return out;
}

/** 该远端命令看起来是不是绕开了 `ccm`（越层启动器）——启发式：非空、不含 "ccm"、且不是
 *  某个 agent 的裸基座命令（显式写 `claude` / `codex` 是有意选择基座行为，不算"看起来像
 *  旧式包装"）。命中不代表一定错——用户可能就是要一个完全自定义的命令——只是账号/模型偏好
 *  不会随它生效，值得提醒。
 *
 *  🔴 `K-R93`（09-12）：这里从前只豁免 `claude` 一个字面量 —— 那是「前端只认 claude」
 *  那一格漏的**第二处**（第一处是 `AGENT_PROFILE`）。填 `codex` 的人从前会收到一句
 *  「你绕开了 ccm」，而他做的与填 `claude` 是同一件事。今天名单由后端那张表给。 */
export function diagnoseRemoteLauncher(cmd: string): string | null {
  const trimmed = cmd.trim();
  if (!trimmed) return null; // 空 = 走默认（后端 `ACTIVE_AGENT` 那一份），不算绕过
  if (baseLaunchers().includes(trimmed)) return null; // 显式基座，不是旧式包装
  if (/ccm/.test(trimmed)) return null; // 命令本身含 ccm 子串（可能是包了一层的自定义命令）
  return "这条命令似乎绕开了 ccm——账号/模型偏好不会随它生效。想要这些好处的话，改填 ccm（或含 ccm 的自定义命令），或用下面的生成器拼一条。";
}

/** F08：把用户在别名生成器里选的组合，拼成一行 shell 函数别名（同 `ccm-aliases.sh` 的既有
 *  写法，用函数不用 `alias`——函数能正确转发 `"$@"`）。纯函数，抽出来单测，不依赖 DOM。
 *  `account`/`base` 由调用方保证互斥（UI 层做的是主动互斥——填一个会清掉另一个，见
 *  `buildAliasGeneratorSection`——不是本函数需要处理的"两者都传"情形，但仍保留 account
 *  优先的兜底，防御性处理调用方万一没做互斥的情况）。 */
/**
 * 🔴 `K-R69` / `KR69D3`：第三个参数是**这条命令该调哪一份 `ccm`**。
 *
 * 默认值刻意仍是裸 `ccm` —— 那是「说不出 / 已经指得到」两档的答案，
 * 而**不是**「懒得管」：三档由 [`ccmInvocation`] 从后端那次身份握手算出来，
 * 调用方把算出来的那个串传进来。
 * ⚠ 它原样拼进 shell，所以只许收 [`ccmInvocation`] 的产物（要么是裸名，
 * 要么是它自己拼好、已经带双引号的 `"${CCM:-…}"`），别在别处现攒一个。
 */
export function buildAliasLine(
  name: string,
  flags: {
    tmux?: boolean;
    account?: string;
    base?: boolean;
    agent?: string;
    model?: string;
    launcher?: string;
  },
  invocation: string = "ccm",
): string {
  const trimmedName = name.trim();
  if (!trimmedName) return "（先填个别名名字）";
  // Phase D 审计（阻塞项修复）：名字此前零校验——含空格/分号/括号等字符会直接拼出语法错误的
  // shell 代码（实测复现：`bash -n <<< 'my alias() { ccm "$@"; }'` 真的报语法错误），而 UI
  // 自己教用户"复制粘贴进 ~/.bashrc"，一旦真粘贴会当场弄坏用户的 shell 配置。函数名合法字符集
  // 同 bash 标识符规则：首字符字母/下划线，其余字母/数字/下划线。
  if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(trimmedName)) {
    return "（别名名字只能用字母/数字/下划线，且不能以数字开头——如 zcct）";
  }
  const q = (s: string): string => `'${s.replace(/'/g, `'\\''`)}'`;
  const parts: string[] = [];
  if (flags.tmux) parts.push("--tmux");
  const acct = flags.account?.trim();
  if (acct) parts.push("--account", q(acct));
  else if (flags.base) parts.push("--base");
  if (flags.agent) parts.push("--agent", flags.agent);
  const model = flags.model?.trim();
  if (model) parts.push("--model", q(model));
  const launcher = flags.launcher?.trim();
  if (launcher) parts.push("--launcher", q(launcher));
  const flagStr = parts.join(" ");
  return `${trimmedName}() { ${invocation}${flagStr ? ` ${flagStr}` : ""} "$@"; }`;
}

/**
 * `K-R49`：一个账号叫什么名字，它那条命令就该叫什么。
 *
 * 规则与 `cc-acct-iso shellinit` 生成的那一族**逐字同形**（`<名>cc`），
 * 这样从两条路进来的人看到的是同一套命令名，不是两套。
 *
 * ⚠ 两处收窄，都是 `buildAliasLine` 那条校验逼出来的（非法名字粘进 shell 配置会当场弄坏它）：
 * ① 账号名里的非法字符**丢掉**而不是替换成下划线 —— `a.b` 与 `a_b` 换成下划线之后会撞成同一个名字；
 * ② 结果若以数字开头（账号 0 那种）就前缀一个 `_`，因为 shell 函数名不许数字打头。
 */
export function suggestAliasName(account: string): string {
  const cleaned = account.replace(/[^A-Za-z0-9_]/g, "");
  if (!cleaned) return "";
  return /^[0-9]/.test(cleaned) ? `_${cleaned}cc` : `${cleaned}cc`;
}

/**
 * `K-R49` 的正主：**加了账号，那条命令也该跟着有 —— 而且真的落盘。**
 *
 * # 它与上面那个手工生成器的分工
 *
 * [`buildAliasGeneratorSection`] 是「我要拼一条**自定义**组合」；这一块是
 * 「**把我这几个账号的命令一次给齐**」。两者共用同一个 [`buildAliasLine`]（唯一那份生成器），
 * 本块一个字节的别名内容都不自己拼。
 *
 * # 🔴 落点刻意**不是** `~/.bashrc`
 *
 * 真正被重写的是 `~/.cc-monitor/account-aliases.sh`（cc-monitor 自己的文件），
 * **按账号表整份重写** ⇒ 幂等、删了账号它那条当场消失、删掉整份文件也只是少几个命令。
 * 往 rc 里追加的那条路有三条病（重复追加 · 删不掉 · 弄坏了 shell 起不来），
 * 逐条记在 `account_aliases.rs` 的模块头注里。
 *
 * # 用户的 shell 配置最多多**一行**，而且多数人连这一行都不用管
 *
 * `shared/ccm-aliases.sh` 自带一行 `if [ -r … ]; then . …; fi` 指向那份生成文件 ⇒
 * 装过 ccm 别名块的人**什么都不用做**。没装的人可以在下拉里**自己选**一份 rc，
 * 由后端把那一行 `source` 装进围栏里（备份 + 原子替换 + 写后回读 + 幂等）。
 * ⚠ 下拉的默认项是「**不动我的 shell 配置**」—— 界面不替人选那份文件。
 *
 * # 🔴 `K-R62`（09-11）：**同一个下拉下面多了「装 ccm 别名块」与「你 rc 里这几行是旧的」**
 *
 * 立件时现打的账：本机 POSIX 侧**装与查都没有口** —— 「终端集成」那一块整篇是 PowerShell，
 * 而 `panel.ts` 用 `hostOsAllows` 把它只留给 Windows ⇒ Linux 上用 cc-monitor 的人
 * 在界面上**一个装口都够不着**，别名块只能自己贴。
 *
 * 补在这里而不是另起一块，理由是它们本来就是同一段话的两半：上面那一行 `source`
 * 之所以「多数人连这一行都不用加」，正是因为 `shared/ccm-aliases.sh` 里自带它 ——
 * 那份文件就是这两个按钮装的东西，而且**与远端「装 ccm 助手」推过去的是同一个常量**
 * （`sftp::CCM_WRAPPER_SNIPPET`，本机与远端同一份实现、同一对围栏）。
 *
 * ⚠ 两条边界，一条都不省：
 * · **默认什么都不做** —— 下拉停在「不动我的 shell 配置」时整块 `hidden`，一条 IPC 都不发；
 * · **产品一个字节都不删用户的行**（`K31` + 用户逐字「原本的配置要手动删除」）。
 *   那段「这几行是旧的」是后端**逐行指名**之后生成的提示，动手的是用户 ——
 *   因为那些行没有围栏，边界只有他自己知道。
 *
 * @param loadAccounts 取账号名的那一跳。**由调用方给**，因为这一块两处挂载
 *   （设置面板的「行为」组 · 账号那一节），而它们手里的账号读口不是同一个。
 */
export function buildAccountAliasBlock(
  loadAccounts: () => Promise<string[]>,
): HTMLElement {
  const wrap = document.createElement("details");
  wrap.className = "ccm-acct-alias";
  const summary = document.createElement("summary");
  summary.textContent = "按账号生成命令（zcc / bcc 这一族）";
  wrap.appendChild(summary);

  const hint = document.createElement("p");
  hint.className = "ccm-acct-alias-hint";
  hint.textContent =
    "给这台机器（cc-monitor 跑着的这台）的 shell 用：每个账号一条命令，" +
    "写进 cc-monitor 自己管的那份别名文件，不是你的 ~/.bashrc。" +
    "它按账号表整份重写：加了账号就多一条，删了账号那条就没了。";
  wrap.appendChild(hint);

  // 🔴 `K-R69` / `KR69D2`：「**你 PATH 上那个 `ccm` 是旧的**」那句话的落点。
  // 后端**逐字**给（`ccm_probe::render_path_ccm_hint`），这里原样上屏，前端不改一个字。
  // 产品**不删**用户任何东西（`K31` ＋ 用户逐字「原本的配置要手动删除」）——
  // 这一块只是把「终端里敲 `ccm` 走到的其实是哪一份」说出来。
  const pathCcm = document.createElement("pre");
  pathCcm.className = "ccm-path-ccm";
  pathCcm.hidden = true;
  wrap.appendChild(pathCcm);

  const list = document.createElement("div");
  list.className = "ccm-acct-alias-list";
  wrap.appendChild(list);

  // rc 选择器。**默认那一项是「不动」** —— 猜一份 shell 配置写进去是最坏的那条路
  // （`.bashrc` / `.zshrc` / fish 的 `config.fish` 写法各不相同）。
  const rcSel = document.createElement("select");
  rcSel.className = "ccm-acct-alias-rc";
  const rcRow = document.createElement("label");
  rcRow.className = "ccm-acct-alias-rcrow";
  rcRow.append("这台机器的 shell 配置（那一行 source 加进哪份）：", rcSel);
  wrap.appendChild(rcRow);

  const out = document.createElement("pre");
  out.className = "ccm-acct-alias-out";
  wrap.appendChild(out);

  const btn = document.createElement("button");
  btn.type = "button";
  btn.className = "ccm-acct-alias-write";
  btn.textContent = "写入";
  wrap.appendChild(btn);

  // ── 🔴 `K-R62`：**选定那份 rc 之后，这台机器上的 ccm 别名块也在这里装 / 查** ──────
  //
  // 为什么挂在这一块下面而不是「终端集成」那一块：后者整篇是 PowerShell，
  // 而且 `panel.ts` 用 `hostOsAllows` 把它**只留给 Windows**（Linux 上换成一行说明）。
  // 于是本机 POSIX 用户在界面上**一个装口都够不着** —— 那正是 `K-R62 §0b` 那个 🔴。
  // 这一块本来就在问「你要不要把 cc-monitor 的东西加进这份 rc」，而 `shared/ccm-aliases.sh`
  // 自带那行 source **正是**上面那份生成文件被接上的方式 ⇒ 同一个旅程的两半，挨着放。
  //
  // ⚠ **默认什么都不做**：下拉停在「不动我的 shell 配置」时整块 `hidden`，
  //   一条 IPC 都不发（连**读**都不读）。选了具体那份 rc 才扫，扫是只读的。
  const rcBlock = document.createElement("div");
  rcBlock.className = "ccm-rc-block";
  rcBlock.hidden = true;
  const rcStatus = document.createElement("div");
  rcStatus.className = "ccm-rc-block-status";
  rcBlock.appendChild(rcStatus);
  const rcBtns = document.createElement("div");
  rcBtns.className = "ccm-rc-block-buttons";
  const installBtn = document.createElement("button");
  installBtn.type = "button";
  installBtn.className = "ccm-rc-block-install";
  installBtn.textContent = "装 ccm 别名块";
  installBtn.title =
    "把 cc / cct 那一块（shared/ccm-aliases.sh，与「装 ccm 助手」推给远端的是同一份）" +
    "装进你选的那份 rc：BEGIN/END 围栏内，写前先备份、写后回读比对、不符回滚，" +
    "块外一个字节都不动。";
  const uninstallBtn = document.createElement("button");
  uninstallBtn.type = "button";
  uninstallBtn.className = "ccm-rc-block-uninstall";
  uninstallBtn.textContent = "卸载别名块";
  uninstallBtn.title = "只删我们自己那个围栏块，你写的任何一行都不动。";
  rcBtns.append(installBtn, uninstallBtn);
  rcBlock.appendChild(rcBtns);
  // 「你 rc 里这几行是旧的」—— 后端逐行指名之后生成的那段话，**原样上屏**。
  // 🔴 产品自己一个字节都不删（`K31` + 用户逐字「原本的配置要手动删除」）。
  const rcLegacy = document.createElement("pre");
  rcLegacy.className = "ccm-rc-block-legacy";
  rcLegacy.hidden = true;
  rcBlock.appendChild(rcLegacy);
  wrap.appendChild(rcBlock);

  let lines: string[] = [];
  /** `K-R69`：本机 `ccm` 这一格。`null` = 还没问到 / 问不出（那时别名回落到裸名）。 */
  let ccmStatus: LocalCcmEntry | null = null;

  const renderReport = (r: AccountAliasReport): void => {
    const rows: string[] = [`别名文件：${r.aliasPath}`];
    rows.push(
      r.names.length
        ? `这份文件里会有 ${r.names.length} 条：${r.names.join(" · ")}`
        : "这份文件里一条命令都没有（这台机器上还没有账号）",
    );
    // 🔴 名字撞了**只出声、不拦**：`cc` 在多数机器上是 C 编译器，用户有权自己决定盖不盖。
    for (const c of r.collisions) rows.push(`⚠ ${c}`);
    for (const n of r.notes) rows.push(n);
    if (r.wroteAliasFile) rows.push("已写入。");
    if (r.wroteRc) rows.push("shell 配置里那一行也加好了。");
    out.textContent = rows.join("\n");

    // 选项每次按最新的报告重建：装完之后那一项要变成「已经 source 过了」。
    const keep = rcSel.value;
    rcSel.textContent = "";
    const none = document.createElement("option");
    none.value = "";
    none.textContent = "不动我的 shell 配置";
    rcSel.appendChild(none);
    for (const c of r.rcCandidates) {
      const o = document.createElement("option");
      o.value = c.path;
      o.textContent = c.sourced ? `${c.path}（已经 source 过了）` : c.path;
      rcSel.appendChild(o);
    }
    if ([...rcSel.options].some((o) => o.value === keep)) rcSel.value = keep;
  };

  const call = async (dryRun: boolean): Promise<void> => {
    try {
      const r = await commands.write_account_aliases({
        lines,
        rcPath: rcSel.value || null,
        dryRun,
      });
      renderReport(r);
    } catch (e) {
      out.textContent = `${dryRun ? "预览" : "写入"}失败：${String(e)}`;
    }
  };

  /** `K-R62`：把选中那份 rc 的现状扫一遍并上屏。**只读**，一个字节都不写。 */
  const renderScan = (scan: ProfileScan): void => {
    rcStatus.textContent = scan.has_ccm_block
      ? `✓ ${scan.path}：ccm 别名块已经装了`
      : `✗ ${scan.path}：还没有 ccm 别名块（cc / cct 这一族）`;
    installBtn.textContent = scan.has_ccm_block ? "重装别名块" : "装 ccm 别名块";
    uninstallBtn.hidden = !scan.has_ccm_block;
    // 🔴 「你 rc 里这几行是旧的」：后端**逐行指名**，产品自己不动手。
    rcLegacy.hidden = !scan.manual_cleanup_hint;
    rcLegacy.textContent = scan.manual_cleanup_hint;
  };

  const refreshRc = async (): Promise<void> => {
    const path = rcSel.value;
    rcBlock.hidden = !path;
    if (!path) return;
    try {
      renderScan(
        await commands.cc_integration_scan_path({ path, commandName: "cc" }),
      );
    } catch (e) {
      // 扫不动**不许静默**：静默的后果是屏幕上停着上一份 rc 的读数，而它现在是假的。
      rcStatus.textContent = `扫不动 ${path}：${String(e)}`;
      rcLegacy.hidden = true;
    }
  };

  /** 装 / 卸都走 `profile_installer`（围栏 + 备份 + 回读比对 + 回滚），前端不拼一个字节。 */
  const runRc = async (verb: "装" | "卸", act: (path: string) => Promise<void>): Promise<void> => {
    const path = rcSel.value;
    if (!path) return;
    installBtn.disabled = true;
    uninstallBtn.disabled = true;
    rcStatus.textContent = `${verb}别名块中…`;
    try {
      await act(path);
    } catch (e) {
      rcStatus.textContent = `${verb}失败：${String(e)}`;
      installBtn.disabled = false;
      uninstallBtn.disabled = false;
      return;
    }
    installBtn.disabled = false;
    uninstallBtn.disabled = false;
    await refreshRc();
  };

  const refresh = async (): Promise<void> => {
    // 🔴 `K-R69`：**先问本机那条 `ccm` 入口**，再生成 —— 生成出来的那一行该指哪儿
    // 取决于这个答案（`KR69D3`）。问不到**不许静默**：那句话原样上屏。
    const got = await loadCcmEntry();
    ccmStatus = got.status;
    const say = got.error ?? got.status?.message ?? "";
    pathCcm.hidden = say === "";
    pathCcm.textContent = say;

    let names: string[];
    try {
      names = await loadAccounts();
    } catch (e) {
      out.textContent = `读不到这台机器上的账号：${String(e)}`;
      return;
    }
    // 生成**全部**交给 `buildAliasLine`——本块不自己拼一个字节。
    // ⚠ 先配对再过滤：先 `filter` 再按下标回头取账号名，下标会错位 ——
    //   那一形会给账号 A 生成一条指向账号 B 的命令，而两行看起来都对。
    const invocation = ccmInvocation(ccmStatus);
    lines = names
      .map((account) => ({ account, alias: suggestAliasName(account) }))
      .filter((p) => p.alias)
      .map((p) => buildAliasLine(p.alias, { account: p.account }, invocation));
    list.textContent = "";
    for (const l of lines) {
      const row = document.createElement("code");
      row.className = "ccm-acct-alias-row";
      row.textContent = l;
      list.appendChild(row);
    }
    await call(true); // 预览：后端一个字节都不写
  };

  btn.addEventListener("click", () => {
    btn.disabled = true;
    void call(false).finally(() => {
      btn.disabled = false;
    });
  });
  // `K-R62`：换了那份 rc 就重扫一遍。**只在人真的换了下拉时发 IPC** ——
  // `renderReport` 重建选项那一下是程序改值，不触发 `change`，也就不会自己去读用户的文件。
  rcSel.addEventListener("change", () => void refreshRc());
  installBtn.addEventListener("click", () => {
    void runRc("装", (path) =>
      // `commandName` / `includeCcFunction` 只对 PowerShell 那一臂有意义；
      // POSIX 那一块的名字住在 `shared/ccm-aliases.sh` 里，由它说了算。
      commands.cc_integration_install({
        path,
        commandName: "cc",
        includeCcFunction: false,
      }),
    );
  });
  uninstallBtn.addEventListener("click", () => {
    void runRc("卸", (path) => commands.cc_integration_uninstall({ path }));
  });
  wrap.addEventListener("toggle", () => {
    if (wrap.open) void refresh();
  });
  void refresh();
  return wrap;
}

/** F08：别名生成器的 DOM——MASTERPLAN §0 推论③「自定义在组合层」的落点。只生成文本，
 *  不代写任何配置文件；`--account`/`--base` 用主动互斥（填一个清另一个），不是静默优先级
 *  （Phase D UX 审计发现纯 tooltip 说明不够，用户看不到任何控件本身的联动反馈）。 */
export function buildAliasGeneratorSection(): HTMLElement {
  const wrap = document.createElement("details");
  wrap.className = "ccm-alias-gen";
  const summary = document.createElement("summary");
  summary.textContent = "生成自定义别名";
  wrap.appendChild(summary);

  const hint = document.createElement("p");
  hint.className = "ccm-alias-gen-hint";
  hint.textContent =
    "拼一条 ccm 组合，生成可以直接粘进 ~/.bashrc（或对应 shell 配置文件）的别名函数。";
  wrap.appendChild(hint);

  // 🔴 `K-R69` / `KR69D3`：手工生成器与上面那一块**同职** —— 它吐的那句 `ccm` 一样
  // 靠 PATH 撞运气。⇒ 同一个来源、同一个 [`ccmInvocation`]，别在这里另写一套。
  // 〔纪律：治的是「所有同职的地方」，不是「我这一处」。〕
  const genPathCcm = document.createElement("pre");
  genPathCcm.className = "ccm-path-ccm";
  genPathCcm.hidden = true;
  wrap.appendChild(genPathCcm);
  let genStatus: LocalCcmEntry | null = null;

  const grid = document.createElement("div");
  grid.className = "ccm-alias-gen-grid";

  const nameIn = document.createElement("input");
  nameIn.type = "text";
  nameIn.placeholder = "别名名字，如 zcct";
  nameIn.title = "生成的 shell 函数名——你在终端敲这个词来触发这条组合";

  const tmuxCk = document.createElement("input");
  tmuxCk.type = "checkbox";
  const tmuxLabel = document.createElement("label");
  tmuxLabel.append(tmuxCk, " --tmux");

  const acctIn = document.createElement("input");
  acctIn.type = "text";
  acctIn.placeholder = "--account <名>（留空=不带）";

  const baseCk = document.createElement("input");
  baseCk.type = "checkbox";
  const baseLabel = document.createElement("label");
  baseLabel.append(baseCk, " --base");
  baseLabel.title = "与 --account 互斥——填一个会自动清空另一个";

  // Phase D 审计（重要项修复）：--account/--base 此前只在 tooltip 里说互斥、生成结果文本里
  // 静默择一，控件本身毫无联动反馈。改成主动互斥：填了 account 就清空/关闭 base，勾了 base
  // 就清空 account——任意时刻只有一个真的处于"激活"状态，不再需要用户自己去读输出文本才能
  // 发现另一个被忽略了。
  acctIn.addEventListener("input", () => {
    if (acctIn.value.trim() && baseCk.checked) baseCk.checked = false;
  });
  baseCk.addEventListener("change", () => {
    if (baseCk.checked && acctIn.value.trim()) acctIn.value = "";
  });

  const agentSel = document.createElement("select");
  // 🔴 `K-R93`：这张清单**从前是两行写死的字面量**（`claude` ＋ `codex`），与
  // `src/agent-profile.ts` 那份画像各写各的。今天两处同源：后端那张表
  //（`adapter.rs::agent_profile_facts` → `src/generated/agent-profile-table.ts`）
  // 说有几个就是几个，默认那一档也用后端的 `ACTIVE_AGENT`，不写死 claude。
  const agentOptions: Array<[string, string]> = [
    ["", `--agent（默认 ${ACTIVE_AGENT}，省略）`],
    ...listAgents()
      .filter((a) => a !== ACTIVE_AGENT)
      .map((a): [string, string] => [a, `--agent ${a}`]),
  ];
  for (const [value, text] of agentOptions) {
    const opt = document.createElement("option");
    opt.value = value;
    opt.textContent = text;
    agentSel.appendChild(opt);
  }

  const modelIn = document.createElement("input");
  modelIn.type = "text";
  modelIn.placeholder = "--model <名>（留空=不带，如 opus）";

  const launcherIn = document.createElement("input");
  launcherIn.type = "text";
  launcherIn.placeholder = "--launcher <cmd>（留空=不带）";

  // T03：输出面 + 复制按钮 + 三句话改走统一组件；**表单与生成逻辑留在这里**
  // （7 个控件是这一处独有的，上提就是把三件不相干的事装进一个盒子）。
  const paste = buildPasteBlock({
    text: () =>
      buildAliasLine(
        nameIn.value,
        {
          tmux: tmuxCk.checked,
          account: acctIn.value,
          base: baseCk.checked,
          agent: agentSel.value || undefined,
          model: modelIn.value,
          launcher: launcherIn.value,
        },
        ccmInvocation(genStatus),
      ),
    target: "~/.bashrc（或你实际用的 shell 配置文件）",
    mergeNote: "追加一行函数定义即可，不影响文件里已有的内容。",
    activation:
      "source 它，或开一个新终端——当前这个终端窗口不会立刻认得这个别名。",
    // 保留 F08 Phase D 审计修的那道门：名字为空/非法时输出是中文提示而不是可执行代码，
    // 当"生成成功"一样复制出去，粘进 .bashrc 同样会造成语法错误。
    invalidReason: (t: string) =>
      !nameIn.value.trim() || t.startsWith("（")
        ? "先填一个合法的别名名字（字母/数字/下划线，不能以数字开头）。"
        : null,
    className: "ccm-alias-gen-out",
  });
  for (const el of [
    nameIn,
    tmuxCk,
    acctIn,
    baseCk,
    agentSel,
    modelIn,
    launcherIn,
  ]) {
    el.addEventListener("input", paste.refresh);
    el.addEventListener("change", paste.refresh);
  }

  grid.append(
    nameIn,
    tmuxLabel,
    acctIn,
    baseLabel,
    agentSel,
    modelIn,
    launcherIn,
  );
  wrap.appendChild(grid);
  wrap.appendChild(paste.element);
  // `K-R69`：问一次本机那条 `ccm` 入口，问到了就重算一次输出（那句话也一起上屏）。
  // ⚠ **不阻塞挂载**：问不到时上面那个默认值（裸 `ccm`）就是答案，界面照常可用。
  void loadCcmEntry().then((got) => {
    genStatus = got.status;
    const say = got.error ?? got.status?.message ?? "";
    genPathCcm.hidden = say === "";
    genPathCcm.textContent = say;
    paste.refresh();
  });
  return wrap;
}

/**
 * 🔴 `K-R135`（`R85`）：**用户级 PATH 那一格** —— 现在状态 · 一个按钮加 · 一个按钮撤。
 *
 * 用户逐字：「只把那个目录塞进进程内 `$env:PATH` 这是怎么做的，**删除能清干净吗？
 * 应该让用户手动点击加，也能管理删除。就像是 log 数据管理一样。**」
 * ⇒ 形状照他点名的那个范式 `src/settings/diagnostics-section.ts`
 * （路径 ＋ 现在的读数 ＋ 几个按钮，`refresh()` 在构造与点按钮时跑）。
 *
 * # 🔴 三条纪律，都是这一格特有的
 *
 * 1. **现算，不缓存** —— 每次 `refresh()` 后端真跑一趟 `powershell.exe`（几百 ms）。
 *    ⇒ 只在**构造**与**点按钮**时调，**不轮询**。用户改完 PATH 回来点一下刷新就对了。
 * 2. **探不动 ≠ 不在 PATH 上。** 后端回 `error` 时这一格显示那句原话，
 *    **不许静默成「未安装」** —— 静默的后果是用户去点「加」，那一下同样会失败，
 *    而两次失败之间他学不到任何东西（同本文件 `refreshRc` 那条「扫不动不许静默」）。
 * 3. **两个按钮互斥地禁用**：已经在了就不让点「加」（点了也只是幂等空转，但按钮亮着
 *    等于在说「还没加」）；不在就不让点「撤」。`error` / 不支持时两个都禁。
 *
 * ⚠ **非 Windows 上不隐藏，而是显示一句「这一档只有 Windows 有」** ——
 * 隐藏会让 Linux 上的人以为「我这儿没这个问题」，而事实是**他那边走的是另一条路**
 * （rc 里那个围栏块，就在上面那一格）。把两条路的关系说出来，比藏起来强。
 */
export function buildUserPathBlock(): HTMLElement {
  const wrap = document.createElement("div");
  wrap.className = "settings-group ccm-user-path-block";

  const heading = document.createElement("div");
  heading.className = "settings-group-title";
  heading.textContent = "这台机器的 ccm 命令（用户级 PATH）";
  wrap.appendChild(heading);

  const hint = document.createElement("div");
  hint.className = "settings-hint";
  hint.textContent =
    "把 cc-monitor 放 ccm 的那个目录加到你的用户级 PATH 上，三种终端（PowerShell / cmd / Git Bash）就都敲得到 ccm。" +
    "只改你自己的用户级 PATH：不需要管理员、不碰系统 PATH、不碰别的用户。改完要重开终端才生效。";
  wrap.appendChild(hint);

  const statusRow = document.createElement("div");
  statusRow.className = "settings-row";
  const statusLabel = document.createElement("span");
  statusLabel.className = "settings-label";
  statusLabel.textContent = "现在状态";
  statusRow.appendChild(statusLabel);
  const statusValue = document.createElement("span");
  statusValue.className = "settings-cc-stat-value ccm-user-path-status";
  statusValue.textContent = "—";
  statusRow.appendChild(statusValue);
  wrap.appendChild(statusRow);

  const dirRow = document.createElement("div");
  dirRow.className = "settings-row settings-row-stack";
  const dirLabel = document.createElement("span");
  dirLabel.className = "settings-label";
  dirLabel.textContent = "那个目录";
  dirRow.appendChild(dirLabel);
  const dirValue = document.createElement("span");
  dirValue.className = "settings-cc-autolaunch-path-value ccm-user-path-dir";
  dirValue.style.fontFamily = "var(--font-mono, monospace)";
  dirValue.style.fontSize = "11px";
  dirValue.style.wordBreak = "break-all";
  dirValue.textContent = "—";
  dirRow.appendChild(dirValue);
  wrap.appendChild(dirRow);

  const btnRow = document.createElement("div");
  btnRow.className = "settings-cc-profile-buttons";
  const addBtn = document.createElement("button");
  addBtn.type = "button";
  addBtn.className = "settings-btn settings-btn-primary ccm-user-path-add";
  addBtn.textContent = "加到用户级 PATH";
  const delBtn = document.createElement("button");
  delBtn.type = "button";
  delBtn.className = "settings-btn settings-btn-secondary ccm-user-path-remove";
  delBtn.textContent = "从用户级 PATH 撤掉";
  delBtn.title = "只摘掉我们自己那一格，你 PATH 上别的东西一个字节都不动。";
  const refreshBtn = document.createElement("button");
  refreshBtn.type = "button";
  refreshBtn.className = "settings-btn settings-btn-secondary ccm-user-path-refresh";
  refreshBtn.textContent = "刷新";
  refreshBtn.title = "重新读一次你的用户级 PATH（现算，不缓存）";
  btnRow.append(addBtn, delBtn, refreshBtn);
  wrap.appendChild(btnRow);

  // 那两条命令的逐字文本 —— **给不想点按钮的人复制**。
  // 🔴 它与按钮跑的是**同一份字节**（后端同一个 render 函数），所以这里敢这么说。
  const cmdNote = document.createElement("div");
  cmdNote.className = "settings-hint ccm-user-path-cmd-note";
  cmdNote.textContent = "不想点按钮？下面这段就是按钮会跑的那一段，复制到 PowerShell 里自己跑一次也一样：";
  cmdNote.hidden = true;
  wrap.appendChild(cmdNote);
  const cmdPre = document.createElement("pre");
  cmdPre.className = "ccm-user-path-cmd";
  cmdPre.hidden = true;
  wrap.appendChild(cmdPre);

  const setBusy = (busy: boolean): void => {
    addBtn.disabled = busy;
    delBtn.disabled = busy;
    refreshBtn.disabled = busy;
  };

  const refresh = async (): Promise<void> => {
    setBusy(true);
    try {
      const st = await commands.ccm_user_path_status();
      dirValue.textContent = st.dir ?? "—";
      if (!st.supported) {
        // 不隐藏，说清它与上面那一格的关系（见本函数头注第 3 条下面那一段）。
        statusValue.textContent =
          "这一档只有 Windows 有 —— 你这台机器上让 ccm 被找到的是上面那个 rc 围栏块。";
        addBtn.hidden = true;
        delBtn.hidden = true;
        cmdNote.hidden = true;
        cmdPre.hidden = true;
        return;
      }
      if (st.error) {
        // 🔴 探不动 ≠ 不在 PATH 上。原话上屏，两个按钮都不给点。
        statusValue.textContent = `读不出来：${st.error}`;
        addBtn.disabled = true;
        delBtn.disabled = true;
        return;
      }
      statusValue.textContent = st.onUserPath
        ? "✓ 已经在你的用户级 PATH 上"
        : "✗ 还不在你的用户级 PATH 上（三种终端里都敲不到 ccm）";
      addBtn.disabled = st.onUserPath;
      delBtn.disabled = !st.onUserPath;
      const text = st.onUserPath ? st.removeCommand : st.addCommand;
      cmdPre.textContent = text ?? "";
      cmdNote.hidden = !text;
      cmdPre.hidden = !text;
    } catch (e) {
      statusValue.textContent = `读不出来：${String(e)}`;
      addBtn.disabled = true;
      delBtn.disabled = true;
    } finally {
      refreshBtn.disabled = false;
    }
  };

  const act = async (what: "add" | "remove"): Promise<void> => {
    setBusy(true);
    try {
      if (what === "add") await commands.ccm_user_path_add();
      else await commands.ccm_user_path_remove();
    } catch (e) {
      showActionFailureToast(
        what === "add" ? "加到用户级 PATH 失败" : "从用户级 PATH 撤掉失败",
        String(e),
      );
    }
    // 成功失败都重扫：**盘上现在是什么样，就显示什么样**（不拿我们以为的结果去写界面）。
    await refresh();
  };

  addBtn.addEventListener("click", () => void act("add"));
  delBtn.addEventListener("click", () => void act("remove"));
  refreshBtn.addEventListener("click", () => void refresh());
  void refresh();
  return wrap;
}

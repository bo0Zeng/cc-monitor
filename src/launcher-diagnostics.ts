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
import type { AccountAliasReport } from "./generated/AccountAliasReport";
// `K-R62`：本机 POSIX 那一格（装别名块 / 查裸行）走的是 `cc_integration_*` 那三条命令，
// 它们的返回形状就是这一个生成物（源 `profile_installer.rs::ProfileScan`）。
import type { ProfileScan } from "./generated/ProfileScan";

/** 该远端命令看起来是不是绕开了 `ccm`（越层启动器）——启发式：非空、不含 "ccm"、且不是
 *  裸 `claude`（显式写 claude 是有意选择基座行为，不算"看起来像旧式包装"）。命中不代表
 *  一定错——用户可能就是要一个完全自定义的命令——只是账号/模型偏好不会随它生效，值得提醒。 */
export function diagnoseRemoteLauncher(cmd: string): string | null {
  const trimmed = cmd.trim();
  if (!trimmed) return null; // 空 = 走默认 claude，不算绕过
  if (trimmed === "claude") return null; // 显式基座，不是旧式包装
  if (/ccm/.test(trimmed)) return null; // 命令本身含 ccm 子串（可能是包了一层的自定义命令）
  return "这条命令似乎绕开了 ccm——账号/模型偏好不会随它生效。想要这些好处的话，改填 ccm（或含 ccm 的自定义命令），或用下面的生成器拼一条。";
}

/** F08：把用户在别名生成器里选的组合，拼成一行 shell 函数别名（同 `ccm-aliases.sh` 的既有
 *  写法，用函数不用 `alias`——函数能正确转发 `"$@"`）。纯函数，抽出来单测，不依赖 DOM。
 *  `account`/`base` 由调用方保证互斥（UI 层做的是主动互斥——填一个会清掉另一个，见
 *  `buildAliasGeneratorSection`——不是本函数需要处理的"两者都传"情形，但仍保留 account
 *  优先的兜底，防御性处理调用方万一没做互斥的情况）。 */
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
  return `${trimmedName}() { ccm${flagStr ? ` ${flagStr}` : ""} "$@"; }`;
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
    lines = names
      .map((account) => ({ account, alias: suggestAliasName(account) }))
      .filter((p) => p.alias)
      .map((p) => buildAliasLine(p.alias, { account: p.account }));
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
  for (const [value, text] of [
    ["", "--agent（默认 claude，省略）"],
    ["codex", "--agent codex"],
  ]) {
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
      buildAliasLine(nameIn.value, {
        tmux: tmuxCk.checked,
        account: acctIn.value,
        base: baseCk.checked,
        agent: agentSel.value || undefined,
        model: modelIn.value,
        launcher: launcherIn.value,
      }),
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
  return wrap;
}

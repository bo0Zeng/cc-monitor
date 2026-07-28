/**
 * F08（unify-launch）：越层启动器诊断 + 别名生成器——放在同一个文件里，因为它们是同一个用户
 * 旅程的两半（诊断"你现在填的命令可能有问题" → 生成器"这是拼一条正确命令的工具"），Phase D
 * UX 审计发现这两半分居设置面板两处互不相邻的地方（诊断在"行为"分组、生成器曾在按主机重复
 * 渲染的"远端 (SSH)"每台机器卡片里）会让用户看到诊断提示后无路可循——已合并成同一处设置分组
 * 里紧邻的两块，不再按主机重复（生成器内容本来就与选中哪台机器无关）。
 *
 * MASTERPLAN 设计原则#7：越层启动器只诊断 + 引导迁移，绝不自动降级、绝不偷改用户配置。
 */
import { showActionFailureToast } from "./error-toast";

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
  flags: { tmux?: boolean; account?: string; base?: boolean; agent?: string; model?: string; launcher?: string },
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
  hint.textContent = "拼一条 ccm 组合，生成可以直接粘进 ~/.bashrc（或对应 shell 配置文件）的别名函数。";
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
  for (const [value, text] of [["", "--agent（默认 claude，省略）"], ["codex", "--agent codex"]]) {
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

  const out = document.createElement("input");
  out.type = "text";
  out.readOnly = true;
  out.className = "ccm-alias-gen-out";

  const regen = (): void => {
    out.value = buildAliasLine(nameIn.value, {
      tmux: tmuxCk.checked,
      account: acctIn.value,
      base: baseCk.checked,
      agent: agentSel.value || undefined,
      model: modelIn.value,
      launcher: launcherIn.value,
    });
  };
  for (const el of [nameIn, tmuxCk, acctIn, baseCk, agentSel, modelIn, launcherIn]) {
    el.addEventListener("input", regen);
    el.addEventListener("change", regen);
  }
  regen();

  const copyBtn = document.createElement("button");
  copyBtn.type = "button";
  copyBtn.className = "settings-btn settings-btn-secondary";
  copyBtn.textContent = "复制";
  copyBtn.addEventListener("click", () => {
    // Phase D 审计（重要项修复）：名字为空/非法时 out.value 是中文提示文案，不是可执行代码——
    // 别把它当"生成成功"一样弹确认复制的 toast（那样粘进 .bashrc 同样会造成语法错误）。
    if (!nameIn.value.trim() || out.value.startsWith("（")) {
      showActionFailureToast("还没生成好", "先填一个合法的别名名字（字母/数字/下划线，不能以数字开头）。", { level: "info", durationMs: 3000 });
      return;
    }
    void navigator.clipboard?.writeText(out.value).then(
      () => showActionFailureToast(
        "已复制",
        "粘贴进 ~/.bashrc（或对应 shell 配置文件），然后 `source` 它或开一个新终端才会生效——当前这个终端窗口不会立刻认得这个别名。",
        { level: "info", durationMs: 5000 },
      ),
      () => showActionFailureToast("复制失败", "剪贴板不可用，手动选中复制。", { level: "error" }),
    );
  });

  grid.append(nameIn, tmuxLabel, acctIn, baseLabel, agentSel, modelIn, launcherIn, out, copyBtn);
  wrap.appendChild(grid);
  return wrap;
}

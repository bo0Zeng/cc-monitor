/**
 * 设置面板「远端 (SSH)」区（SSH-remote issue #15 / 多机 #30）。
 *
 * 让用户配置 + 启用「远端模式」：monitor 通过 SSH 连到 **0..N 台** 远端主机，由各台的
 * daemon 作为额外数据源（与本地 jsonl-watcher 聚合）。配置写入 config.json 的 `remote`
 * 子对象（`{ enabled, hosts: [...] }`），由 Rust 侧 `lib.rs::load_remote_configs` 启动时读。
 *
 * **camelCase key 必须与 Rust reader 严格一致**（否则后端读不到）：
 *   enabled (bool) / hosts[] 内每台：label (string, 可选默认 host) / host / port (默认 22) /
 *   user / keyPath (可选) / daemonPath / hostKeyFingerprint (可选)
 *
 * **向后兼容**：旧的单对象 `remote: { enabled, host, ... }`（无 `hosts` 键）读取时归一成
 * 1 台（label 默认 = host）；保存时升级写成 `hosts` 数组。
 *
 * 设计（对齐 behavior.ts / diagnostics-section.ts 范式）：
 * - 读写走 config.ts 的 loadConfig / saveConfig（schema-agnostic 透传）。
 * - **MERGE 而非覆盖**：保存时先 loadConfig 拿到完整 config，只替换 `remote` 子对象。
 * - 改动后需**重启 monitor 才生效**（数据源在 setup() 启动时定型），保存后 banner 提示。
 * - 每次输入 change 立即保存（无"未保存"中间态）→ refresh() 可安全从 config 重建卡片。
 *
 * Tier 1（issue #15）：从 ~/.ssh/config 导入别名（`ssh -G`）→ 作为**新机器**加入列表；
 * 每台各有「测试连接」（`test_remote_connection`）展示 SSH/指纹/daemon，指纹可一键固化。
 */

import { commands } from "../ipc/commands";
import { openPortForwardPanel } from "../views/port-forward";
// F12：配置数据层已抽到 src/remote-config.ts（治分层倒挂）——UI 从数据模块 import，不再自持 CRUD。
import {
  describeFacet,
  FACET_LABELS,
  MACHINE_FACETS,
  readStatus,
  LOCAL_MACHINE_KEY,
  forgetMachine,
  renameMachine,
  type MachineFacet,
  type MachineStatus,
  type FacetState,
} from "./machine-status";
import {
  HOST_DEFAULTS,
  readRemoteConfig,
  patchRemoteConfig,
  hostKey,
  type RemoteHostConfig,
  type RemoteConfig,
} from "../remote-config";
import {
  MachineCard,
  defaultDaemonPathFor,
  shouldShowResetFingerprint,
} from "./machine-card";
import { markRestartNeeded } from "./restart-notice";
import { computeGaps, summarizeGaps, describeGap } from "./readiness";
import { hostOs } from "./host-os"; // S9：本机 OS 决定哪些组件适用
// 旧调用点从本模块 import 这两个（测试也是）——搬家后原样再导出，不制造无谓的改动面。
export { shouldShowResetFingerprint };
import { makeInfoIcon } from "./info-icon";

// C04d 批 5c：五个类型换成生成物（源 `ssh_source.rs`）。手写版与生成物**逐字等价** ⇒ 零漂移。
//
// **`ConnectStage` 一并生成的理由不是「顺手」**：下面 `describeStage` 里有
// `const _never: never = st` 穷尽性兜底，而**手写类型时 Rust 新增一个 variant 并不会让它红**
// ——那条 `never` 检查一直在守一个 TS 侧自己造的联合，不是 Rust 的真实形状。
// 换成生成物后它才真正对 Rust 的改动有牙（本批次已用变异验证）。
import type { ConnectStage } from "../generated/ConnectStage";
import type { ImportGroup } from "../generated/ImportGroup";
import type { ImportMember } from "../generated/ImportMember";

export type { ConnectStage };


/** F46：阶段事件 → 泳道行的图标 + 文案。纯函数便于单测。 */
export function describeStage(st: ConnectStage): {
  icon: string;
  text: string;
} {
  switch (st.kind) {
    case "dialing":
      return { icon: "→", text: `拨号 ${st.endpoint}` };
    case "hostKey":
      return { icon: "🔑", text: `${st.endpoint} 主机指纹 ${st.fingerprint}` };
    case "failed":
      return { icon: "✗", text: `${st.endpoint} 失败：${st.reason}` };
    case "won":
      return { icon: "✓", text: `${st.endpoint} 胜出（其余地址已取消）` };
    case "auth":
      return st.ok
        ? { icon: "✓", text: "鉴权通过" }
        : { icon: "✗", text: `鉴权失败：${st.detail ?? ""}` };
    case "established":
      return { icon: "●", text: "连接就绪" };
    default: {
      // F46 建议 E：穷尽性兜底——未来新增 ConnectStage 变体时编译期(never)即报错。
      const _never: never = st;
      return {
        icon: "·",
        text: String((_never as { kind?: string }).kind ?? ""),
      };
    }
  }
}


// F12：`RemoteHostConfig` / `RemoteConfig` / `parseAddressLines` / `sftpEligibleHosts` 已移入
// `src/remote-config.ts`（数据层），本文件从那里 import（见顶部）。



const REMOTE_INFO_TEXT =
  "远端模式：monitor 通过 SSH 连到一台或多台远端主机，由各台 daemon 作为额外数据源\n" +
  "与本地聚合（渲染、Tab、分支等行为完全相同；远端 Tab 标题带 [机器名] 前缀）。\n" +
  "关闭（默认）或机器列表为空时一切走本地，不受影响。\n\n" +
  "⚠ 启用 / 修改任意远端设置后，需重启 monitor 才生效。\n" +
  "某台配置不完整（缺 host / user / daemonPath）时后端会跳过该台。";

/**
 * Feature ②：远端 ↗ 拉前的 bashrc 块——**注册原语与启动器分离**（镜像本地
 * `__ccm_bind` + 可选 `cc` wrapper 的设计；用户设计评审指正：注册不该耦合启动）：
 *
 * - `__ccm_rbind`（注册原语）：只做注册——tmux 内对当前 session 开标题直通 +
 *   **F02 起这些实现搬进了 `~/.local/bin/ccm`（可执行文件）**，本文件只 import 别名块。
 *   为什么不再装成 shell 函数：函数**优先于 PATH**，与用户已有同名函数硬冲突且必然遮蔽
 *   （实测：共存时新 CLI 一次都跑不到，且是静默的）；且远端是 zsh/fish 时 `.bashrc`
 *   根本不被 source，函数形态拿不到。别名块只做**组合**（`cct() { ccm --tmux "$@"; }`），
 *   不含任何实现——自定义在组合层，不在实现层。
 *
 * `ccm-rbind-%s` 标记必须与后端 `bind.rs` 的 `format!("ccm-rbind-{sid}")` 完全一致。
 *
 * tmux 自适配（Batch7 真机排查实证）：tmux 默认 `set-titles off`——OSC 标题转义
 * 只落到 pane title、到不了外层 ssh 终端窗口标题，marker 被截住导致绑定必然
 * 失败，而 tmux 恰是远端最常见形态。原语内自动对**当前 session** 开直通
 * （session 级选项，不写 tmux.conf、不影响其它 session）。
 */
// 单一来源：shared/ccm-aliases.sh（后端 sftp.rs include_str! 同一文件，杜绝漂移）
import CCM_WRAPPER_SNIPPET from "../../shared/ccm-aliases.sh?raw";
import { buildPasteBlock } from "../paste-block"; // T03：待贴文本统一组件

/**
 * S4b：「每台机器一页」的宿主。由 `panel.ts` 用 `SettingsRouter` 实现。
 *
 * 抽成接口而不是直接把 router 传进来：本分节只需要「给我开一页 / 收掉一页 / 跳过去」
 * 这三件事，不该知道路由器长什么样（也让它在没有路由器的场合——如既有单测——照常工作）。
 */
export interface MachinePagesHost {
  /**
   * `parts` 有值时宿主可以把它拆成「连接 / 组件」两栏（S4b-3b-2）；
   * 本机页没有卡片、不带 parts。
   */
  addMachinePage(
    id: string,
    title: string,
    element: HTMLElement,
    parts?: { connection: HTMLElement; components: HTMLElement },
  ): void;
  removeMachinePage(id: string): void;
  navigateToMachinePage(id: string): void;
}

export interface RemoteSectionOptions {
  /** 被 CollapsibleGroup 包起来时传 headless: true，不渲染自己的小标题。 */
  headless?: boolean;
  /**
   * S4b：有它就把每台机器的编辑表单搬到**它自己那一页**，列表里只留一行
   * （名字 + 状态 + 点进去）。**不传就是老形态**（卡片就地折叠展开）——
   * 既有单测与任何不带路由器的宿主照常工作。
   */
  pages?: MachinePagesHost;
}

/** S4b：机器详情页的路由 id 前缀。 */
export const MACHINE_PAGE_PREFIX = "machine:";
/** S4b-2：本机那一页的路由 id。与 `LOCAL_MACHINE_KEY` 同源，两处不各写一份。 */
export const LOCAL_MACHINE_PAGE_ID = `${MACHINE_PAGE_PREFIX}${LOCAL_MACHINE_KEY}`;

// === 共享 DOM 小工具 ===

/**
 * S3/S4b：把一台机器的状态格子渲染进容器。**纯读账本，绝不发起探测**
 *（主计划 §1-2）。列表行与本机行共用同一份渲染，免得两处慢慢长歪。
 */
export function renderStatusCells(
  strip: HTMLElement,
  status: MachineStatus,
  overrides: Partial<Record<MachineFacet, FacetState>> = {},
): void {
  strip.replaceChildren();
  for (const facet of MACHINE_FACETS) {
    const d = describeFacet(overrides[facet] ?? status[facet]);
    const cell = document.createElement("span");
    cell.className = `remote-status-cell remote-status-${d.tone}`;
    cell.dataset.facet = facet;
    cell.textContent = `${d.icon} ${FACET_LABELS[facet]}`;
    // 年龄放 title 只是**补充**：`describeFacet` 的正文已把「多旧」说清楚，
    // 而 §1-3 明令状态性信息不得只活在 hover 里。
    cell.title = `${FACET_LABELS[facet]}：${d.text}`;
    strip.appendChild(cell);
  }
}






// === 单台机器卡片 ===


/**
 * 一台远端机器的 UI 卡片：自己的字段输入 + 测试连接（含 TOFU 指纹固化）+ 删除。
 * collect() 读出 RemoteHostConfig。所有字段 change 都通过 hooks.onChange 触发 section 保存。
 */

// === 远端区（机器列表容器）===

export class RemoteSection {
  private root: HTMLElement;
  private headless: boolean;
  /** S4b：机器详情页宿主（没有就退回「卡片就地展开」的老形态）。 */
  private pages?: MachinePagesHost;
  /** S4b：已注册的机器页 id —— 重建列表时按它收掉旧页。 */
  private machinePageIds: string[] = [];
  /** S5/E56：「还差什么」清单容器。 */
  private gapsBox!: HTMLElement;

  /** 打开面板时从 config 拉到的快照，用于判断是否变化（变了就提示重启）。 */
  private original: RemoteConfig = { enabled: false, hosts: [] };

  /**
   * S1：本编辑器**加载时**看到的机器 key 列表。保存时 `remove = loadedKeys − 现存卡片的 key`。
   * 基准取「加载时看到的」而非「盘上全量」，是为了让 S2 拆页后一页只对自己那几台负责。
   */
  private loadedKeys: string[] = [];

  private enabledCheckbox!: HTMLInputElement;
  private machinesContainer!: HTMLElement;
  private emptyHint!: HTMLElement;
  private banner!: HTMLElement;
  private importSelect!: HTMLSelectElement;
  private importHint!: HTMLElement;

  private cards: MachineCard[] = [];

  constructor(opts: RemoteSectionOptions = {}) {
    this.headless = opts.headless ?? false;
    this.pages = opts.pages;
    this.root = this.build();
    void this.refresh();
  }

  get element(): HTMLElement {
    return this.root;
  }

  /** 设置面板每次 open 时调，确保展示的是 config.json 里的最新值。 */
  async refresh(): Promise<void> {
    this.original = await readRemoteConfig();
    this.enabledCheckbox.checked = this.original.enabled;
    this.rebuildCards(this.original.hosts);
    this.hideBanner();
    void this.populateAliases();
  }

  /**
   * S3：本机行 —— 列表**第一行、不可删**。
   *
   * 这是 `INVARIANTS §40`「本地 = 不走 ssh 的远端」的诚实表达：本机不是一个特殊物种，
   * 它就是机器列表里的一行，只是那几个格子的取值不同。
   *
   * ★ **它刻意不是一张 `MachineCard`，也绝不进 `this.cards`。**
   * `this.cards` 是 S1 保存路径的输入（每张卡 = config.json 里的一条 `RemoteHostConfig`）。
   * 把本机混进去，保存时就会往用户的远端机器列表里写一台叫「本机」的假机器。
   * 由 `remote-section.vitest.ts` 里那条「加了本机行之后写出去的机器数不变」钉住。
   */
  private buildLocalRow(): HTMLElement {
    const row = document.createElement("div");
    row.className = "remote-machine remote-machine-row remote-machine-local";
    row.dataset.pageId = LOCAL_MACHINE_PAGE_ID;
    const legend = document.createElement("div");
    legend.className = "remote-machine-legend";
    row.appendChild(legend);

    // S4b-2：本机也有自己的一页 —— 「本地 = 不走 ssh 的远端」不只是说法，
    // 它在导航里就该和别的机器长得一样、点得进去。
    const name = document.createElement("button");
    name.type = "button";
    name.className = "remote-machine-name remote-machine-open";
    name.textContent = "本机";
    name.addEventListener("click", () =>
      this.pages?.navigateToMachinePage(LOCAL_MACHINE_PAGE_ID),
    );
    legend.appendChild(name);

    const strip = document.createElement("span");
    strip.className = "remote-machine-status";
    legend.appendChild(strip);
    // daemon 那格对本机是**不适用**，不是「缺组件」：`watcher.rs` 直读 jsonl，
    // 本机压根不需要 daemon（主计划 §2.4 那张表逐字写着「不需要」）。
    renderStatusCells(strip, readStatus(LOCAL_MACHINE_KEY), {
      daemon: { kind: "na", detail: "不需要", at: 0 },
    });
    // **没有删除按钮** —— 本机删不掉，这不是「暂未实现」，是它本来就不该能删。
    return row;
  }

  /** 用 config 里的机器列表重建卡片。 */
  private rebuildCards(hosts: RemoteHostConfig[]): void {
    // S4b：重建前先把上一批机器页收掉，否则改完配置会留下一串指向已不存在机器的导航项。
    for (const id of this.machinePageIds) this.pages?.removeMachinePage(id);
    this.machinePageIds = [];
    this.cards = [];
    this.machinesContainer.innerHTML = "";
    this.machinesContainer.appendChild(this.buildLocalRow());
    if (this.pages) {
      // 本机页的内容由宿主（panel）填 —— 它拿得到那几块 per-machine 分节，本分节拿不到。
      const localPage = document.createElement("div");
      localPage.className = "machine-page-local";
      this.pages.addMachinePage(LOCAL_MACHINE_PAGE_ID, "本机", localPage);
      this.machinePageIds.push(LOCAL_MACHINE_PAGE_ID);
    }
    for (const h of hosts) {
      // 从 config 重建的卡片默认折叠（只显示机器名）——多机时列表整洁；点名称展开编辑。
      // S1：从盘上来的卡片带着它此刻的 origin 当 persistedKey。
      this.appendCard(h, true, hostKey(h));
    }
    // S1：本编辑器**这次加载时**看到的 key 集合。删除判据以它为基准，
    // 而**不是**「盘上全量」—— 这正是 S2 拆页后的安全边界：一页只对自己加载过的负责。
    this.loadedKeys = hosts.map(hostKey);
    this.renderGaps(hosts);
    this.updateEmptyHint();
  }

  /**
   * S5/E56：渲染「还差什么」。**纯读账本**（`computeGaps` 是纯函数，不碰 IO）。
   *
   * 「缺」与「没测过」分开显示 —— 一个刚装好、什么都没点过的新用户不该看到一屏红叉。
   * 后果写出来（不只是一个 ✗），让他自己判断值不值得补。
   */
  private renderGaps(hosts: RemoteHostConfig[]): void {
    const daemonless = new Set(
      hosts.filter((h) => h.daemonless).map((h) => hostKey(h)),
    );
    const gaps = computeGaps({
      origins: [LOCAL_MACHINE_KEY, ...hosts.map(hostKey)],
      statusOf: readStatus,
      isDaemonless: (o) => daemonless.has(o),
      // S9：Windows 本机的启动器是「终端集成」那块，不是 POSIX 的 ccm。
      hostOs: hostOs(),
    });
    const summary = summarizeGaps(gaps);
    this.gapsBox.replaceChildren();
    if (!summary) {
      // 全绿就整块不出现 —— 老用户不该天天看见一个空清单。
      this.gapsBox.style.display = "none";
      return;
    }
    this.gapsBox.style.display = "";
    const head = document.createElement("div");
    head.className = "settings-label remote-gaps-head";
    head.textContent = `还差什么：${summary}`;
    this.gapsBox.appendChild(head);
    const list = document.createElement("ul");
    list.className = "remote-gaps-list";
    for (const g of gaps) {
      const li = document.createElement("li");
      li.className = `remote-gap remote-gap-${g.kind} remote-gap-${g.severity}`;
      li.dataset.origin = g.origin;
      li.dataset.facet = g.facet;
      li.dataset.kind = g.kind;
      li.textContent = describeGap(g);
      list.appendChild(li);
    }
    this.gapsBox.appendChild(list);
  }

  private appendCard(
    initial: RemoteHostConfig,
    collapsed = false,
    persistedKey: string | null = null,
  ): MachineCard {
    const card = new MachineCard(
      initial,
      {
        onChange: () => void this.save(),
        onRemove: (c) => this.removeCard(c),
        onStatusChanged: (c) => this.refreshMachineRow(c),
      },
      collapsed,
      persistedKey,
    );
    this.cards.push(card);
    if (this.pages) {
      // S4b：表单搬到这台机器自己那一页；列表里只留一行（名字 + 状态 + 点进去）。
      const id = `${MACHINE_PAGE_PREFIX}${persistedKey ?? hostKey(initial)}`;
      card.setPageMode();
      this.pages.addMachinePage(id, card.displayName(), card.element, card.parts());
      this.machinePageIds.push(id);
      this.machinesContainer.appendChild(this.buildMachineRow(card, id));
    } else {
      this.machinesContainer.appendChild(card.element);
    }
    this.updateEmptyHint();
    return card;
  }

  /**
   * S4b：列表里的一行 —— 名字 + 状态条 + 点进去。**编辑表单不在这里**（在那台机器自己那页）。
   *
   * 状态条只渲染在行上，不再渲染在卡片 legend 上：§2.3 那张图里状态就是**列表**的一列，
   * 而详情页上用户看的是那些动作按钮本身的结果，不需要再来一份缓存结论。
   */
  private buildMachineRow(card: MachineCard, pageId: string): HTMLElement {
    const row = document.createElement("div");
    row.className = "remote-machine remote-machine-row";
    row.dataset.pageId = pageId;

    const legend = document.createElement("div");
    legend.className = "remote-machine-legend";
    row.appendChild(legend);

    const name = document.createElement("button");
    name.type = "button";
    name.className = "remote-machine-name remote-machine-open";
    name.textContent = card.displayName();
    name.addEventListener("click", () => this.pages?.navigateToMachinePage(pageId));
    legend.appendChild(name);

    const strip = document.createElement("span");
    strip.className = "remote-machine-status";
    legend.appendChild(strip);
    renderStatusCells(strip, readStatus(card.persistedKey ?? hostKey(card.collect())));

    const removeBtn = document.createElement("button");
    removeBtn.type = "button";
    removeBtn.className =
      "settings-btn settings-btn-secondary remote-machine-remove";
    removeBtn.textContent = "删除";
    removeBtn.title = "从列表移除这台机器";
    removeBtn.addEventListener("click", (ev) => {
      ev.stopPropagation();
      this.removeCard(card);
    });
    legend.appendChild(removeBtn);
    return row;
  }

  /**
   * 按 pageId 找列表里那一行。
   *
   * **刻意不用 `querySelector` + `CSS.escape`**：pageId 里含用户填的机器名（任意字符），
   * 直接拼进选择器会炸；而 `CSS.escape` 在 jsdom 里不一定有（实测：用了它之后
   * `removeCard` 在 `save()` 之前就抛，删除功能整个静默失效，5 条既有测试同时变红）。
   * 扫一遍 `dataset` 既安全又不依赖宿主实现。
   */
  private findMachineRow(pageId: string): HTMLElement | null {
    for (const el of this.machinesContainer.children) {
      if ((el as HTMLElement).dataset?.pageId === pageId) return el as HTMLElement;
    }
    return null;
  }

  /** S4b：刷新某台机器在列表里那一行（名字 + 状态条）。没有分页宿主时什么都不用做。 */
  private refreshMachineRow(card: MachineCard): void {
    if (!this.pages) return;
    const pageId = `${MACHINE_PAGE_PREFIX}${card.persistedKey ?? hostKey(card.collect())}`;
    const row = this.findMachineRow(pageId);
    if (!row) return;
    const nameBtn = row.querySelector<HTMLElement>(".remote-machine-open");
    if (nameBtn) nameBtn.textContent = card.displayName();
    const strip = row.querySelector<HTMLElement>(".remote-machine-status");
    if (strip) {
      renderStatusCells(
        strip,
        readStatus(card.persistedKey ?? hostKey(card.collect())),
      );
    }
  }

  private removeCard(card: MachineCard): void {
    const idx = this.cards.indexOf(card);
    if (idx < 0) return;
    // S3：连同状态账本一起清 —— 否则下一台取同名的机器会**继承上一台的结论**，
    // 显示一个它从没做过的 ✓。
    if (card.persistedKey) forgetMachine(card.persistedKey);
    this.cards.splice(idx, 1);
    card.element.remove();
    // S4b：连它那一页和列表行一起收掉，否则导航里会留下一个指向已删机器的死项。
    const pageId = `${MACHINE_PAGE_PREFIX}${card.persistedKey ?? hostKey(card.collect())}`;
    this.pages?.removeMachinePage(pageId);
    this.machinePageIds = this.machinePageIds.filter((x) => x !== pageId);
    this.findMachineRow(pageId)?.remove();
    this.updateEmptyHint();
    void this.save();
  }

  private updateEmptyHint(): void {
    this.emptyHint.style.display = this.cards.length === 0 ? "block" : "none";
  }

  /** 从 ~/.ssh/config 拉别名清单填进导入下拉。空 → 禁用下拉 + 提示。 */
  private async populateAliases(): Promise<void> {
    let aliases: string[] = [];
    try {
      // 同 `mcp-section` 那处：**别只防 reject**，`invoke` 也可能 resolve 成 `undefined`
      // → 下面 `aliases.length` 抛（T07 审计④）。
      const got = await commands.list_ssh_host_aliases();
      if (Array.isArray(got)) aliases = got;
    } catch (e) {
      console.warn("list_ssh_host_aliases failed:", e);
    }

    this.importSelect.innerHTML = "";
    const placeholder = document.createElement("option");
    placeholder.value = "";
    placeholder.textContent = "选择一个主机别名…（导入为新机器）";
    this.importSelect.appendChild(placeholder);

    if (aliases.length === 0) {
      this.importSelect.disabled = true;
      this.importHint.textContent =
        "未在 ~/.ssh/config 找到可导入的主机别名（也可点「添加机器」手动填写）。";
      this.importHint.style.display = "block";
      return;
    }

    for (const a of aliases) {
      const opt = document.createElement("option");
      opt.value = a;
      opt.textContent = a;
      this.importSelect.appendChild(opt);
    }
    this.importSelect.disabled = false;
    this.importHint.style.display = "none";
    this.importSelect.value = "";
  }

  // === DOM 构建 ===

  private build(): HTMLElement {
    const group = document.createElement("div");
    group.className = this.headless ? "settings-headless" : "settings-group";

    if (!this.headless) {
      const heading = document.createElement("div");
      heading.className = "settings-group-title";
      heading.textContent = "远端 (SSH)";
      heading.appendChild(makeInfoIcon(REMOTE_INFO_TEXT));
      group.appendChild(heading);
    }

    this.banner = document.createElement("div");
    this.banner.className = "settings-banner";
    group.appendChild(this.banner);

    // ★ S4b-3b：**一条工具条**（主计划 §2.3 那张图逐字给的顺序）：
    //   + 添加 · 从 ssh config 导入 · 批量导入 · 端口转发 · [x] 启用远端模式
    //
    // 此前这几个控件散在列表**上下两侧**（导入在最上、端口转发和启用 toggle 在中间、
    // 添加按钮在列表下方），空列表提示还得写「点**下方**添加机器，或从**上方**下拉导入」
    // ——一句提示要同时指两个方向，本身就是布局在报警。
    //
    // 归拢的判据与 §2.1 同源：**它们改的都不是某一台机器的状态，而是这份列表本身**
    //（加一台 / 导入一批 / 全局开关 / 跨机器的隧道台）。per-machine 的东西在机器详情页上。
    const toolbar = document.createElement("div");
    toolbar.className = "settings-row remote-toolbar";

    const addBtn = document.createElement("button");
    addBtn.type = "button";
    addBtn.className = "settings-btn settings-btn-secondary";
    addBtn.textContent = "+ 添加机器";
    addBtn.addEventListener("click", () => {
      this.appendCard({ ...HOST_DEFAULTS });
      // 空白机器先不写 config（缺必填字段无意义）；用户填了字段 change 时才 save。
    });
    toolbar.appendChild(addBtn);

    // 从 ~/.ssh/config 导入（下拉 + 批量导入按钮）——它自己会往 toolbar 里塞两个控件。
    this.buildImportRow(toolbar);

    // F58：端口转发管理台入口。**跨机器**的隧道台，属于列表级而非某台机器。
    const pfBtn = document.createElement("button");
    pfBtn.type = "button";
    pfBtn.className = "settings-btn settings-btn-secondary";
    pfBtn.textContent = "端口转发…";
    pfBtn.title =
      "本地端口转发(-L)管理台:把远端机(或其内网)端口映到本机,经已配置的 SSH 连接隧道";
    pfBtn.addEventListener("click", () => openPortForwardPanel());
    toolbar.appendChild(pfBtn);

    // 启用 toggle（全局）。**留在工具条上而不是收进折叠**：它是状态性开关，
    // 关着的时候整个列表都不生效 —— 藏起来会让人对着一列配好的机器纳闷为什么没连上
    //（`INVARIANTS §12`：「用户看到了但没注意到关键信息」已真实发生过一次）。
    const enabledRow = document.createElement("label");
    enabledRow.className = "settings-row-checkbox remote-toolbar-toggle";
    this.enabledCheckbox = document.createElement("input");
    this.enabledCheckbox.type = "checkbox";
    this.enabledCheckbox.className = "settings-checkbox";
    this.enabledCheckbox.addEventListener("change", () => void this.save());
    enabledRow.appendChild(this.enabledCheckbox);
    const enabledLabel = document.createElement("span");
    enabledLabel.className = "settings-checkbox-label";
    enabledLabel.textContent = "启用远端模式";
    enabledRow.appendChild(enabledLabel);
    enabledRow.appendChild(
      makeInfoIcon(
        "勾选后 monitor 启动时会**额外**用 SSH 连下列每台机器作为数据源（与本地聚合）。\n" +
          "⚠ 需重启 monitor 才生效。某台配置不完整时后端跳过该台。列表为空 = 等于关闭。",
      ),
    );
    toolbar.appendChild(enabledRow);
    group.appendChild(toolbar);

    // ★ S5 / E56：「还差什么」——新用户一站式的落点。
    // **只读 S3 的账本，不发任何请求**（§1-2）；空的时候整块不渲染，不打扰老用户。
    this.gapsBox = document.createElement("div");
    this.gapsBox.className = "remote-gaps";
    this.gapsBox.style.display = "none";
    group.appendChild(this.gapsBox);

    // 机器列表容器
    this.machinesContainer = document.createElement("div");
    this.machinesContainer.className = "remote-machines";
    group.appendChild(this.machinesContainer);

    // 空列表提示。文案跟着布局改：控件全在**上方**那条工具条上了，
    // 不再需要「点下方…或从上方…」这种同时指两个方向的说法。
    this.emptyHint = document.createElement("div");
    this.emptyHint.className = "settings-hint";
    this.emptyHint.textContent =
      "尚未添加远端机器。用上方工具条「+ 添加机器」，或从 ssh config 导入。";
    this.emptyHint.style.display = "none";
    group.appendChild(this.emptyHint);

    // 4. Feature ②：远端 ↗ 拉前用的只读 ccm wrapper 片段
    this.buildWrapperSnippetRow(group);

    return group;
  }

  /** 顶部「从 ~/.ssh/config 导入」行：label + select + hint。 */
  /**
   * S4b-3b：「从 ssh config 导入」的两个控件，**直接塞进工具条**，不再自带一整行。
   *
   * 原先它是一整块：标题行「从 ~/.ssh/config 导入」+ ⓘ + 下拉 + 批量按钮 + hint 行。
   * 归进工具条后标题行是多余的（下拉自己的 placeholder 已经写着「选择一个主机别名…」），
   * ⓘ 挪到下拉上，hint 仍保留 —— 它承载的是**失败态**（没读到别名 / 读取失败），
   * 属于 §1-3 说的「不读就会做错事的」，不能收进 hover。
   */
  private buildImportRow(toolbar: HTMLElement): void {
    this.importSelect = document.createElement("select");
    this.importSelect.className = "settings-input settings-input-select";
    this.importSelect.title =
      "选一个 ~/.ssh/config 里的主机别名，自动用 `ssh -G` 解析出 host/port/user/私钥路径并新增一台机器填好（免手敲）。仍可手动微调任意字段。";
    const placeholder = document.createElement("option");
    placeholder.value = "";
    placeholder.textContent = "从 ssh config 导入…";
    this.importSelect.appendChild(placeholder);
    this.importSelect.disabled = true;
    this.importSelect.addEventListener(
      "change",
      () => void this.onImportAlias(),
    );
    toolbar.appendChild(this.importSelect);

    // F57：批量导入——一次导入全部主机,智能聚合同机多地址,预览可拆分。
    const batchBtn = document.createElement("button");
    batchBtn.type = "button";
    batchBtn.className = "settings-btn settings-btn-secondary";
    batchBtn.textContent = "批量导入…";
    batchBtn.title =
      "一次导入 ~/.ssh/config 全部主机；同密钥+同用户+同基名前缀的别名智能聚合成一台多地址主机（预览可拆分/勾选）";
    batchBtn.addEventListener("click", () => void this.onBatchImport());
    toolbar.appendChild(batchBtn);

    // 失败态提示（读不到别名 / 读取失败）。**不进 hover** —— 见方法头注。
    // 挂在工具条外面（整行宽），否则长文案会把工具条撑变形。
    this.importHint = document.createElement("div");
    this.importHint.className = "settings-hint";
    this.importHint.style.display = "none";
    toolbar.insertAdjacentElement("afterend", this.importHint);
  }

  /** 选了别名 → resolve_ssh_host → 新增一台机器并填好 → 保存。 */
  private async onImportAlias(): Promise<void> {
    const alias = this.importSelect.value;
    if (!alias) return;
    try {
      const resolved = await commands.resolve_ssh_host({
        alias,
      });
      const card = this.appendCard({ ...HOST_DEFAULTS });
      card.applyResolved(resolved, alias);
      await this.save();
      this.showBanner(`已从别名「${alias}」导入为新机器。`);
    } catch (e) {
      console.warn("resolve_ssh_host failed:", e);
      this.showBanner(`导入别名「${alias}」失败：${String(e)}`);
    } finally {
      this.importSelect.value = "";
    }
  }

  /** F57：批量导入——import_ssh_hosts（智能聚合）→ 预览弹框（可拆分/勾选）→ 建卡。 */
  private async onBatchImport(): Promise<void> {
    let groups: ImportGroup[];
    try {
      groups = await commands.import_ssh_hosts();
    } catch (e) {
      this.showBanner(`批量导入失败：${String(e)}`);
      return;
    }
    if (groups.length === 0) {
      this.showBanner("~/.ssh/config 里没有可导入的主机。");
      return;
    }
    this.showImportPreview(groups);
  }

  /** F57：聚合组 → RemoteHostConfig（label 可被预览编辑覆盖）。 */
  private groupToCfg(g: ImportGroup, label: string): RemoteHostConfig {
    return {
      label: label.trim() || g.label,
      host: g.host,
      port: g.port || 22,
      user: g.user,
      keyPath: g.keyPath ?? "",
      daemonPath: g.user ? defaultDaemonPathFor(g.user) : "",
      hostKeyFingerprint: "",
      addresses: g.addresses,
      jump: g.jump ?? "",
      daemonless: false, // F59：从 ssh config 导入的主机默认走 daemon 路径
      resumeCommand: "", // S4b-3：空 = 沿用全局默认（导入时无从得知这台该用什么）
    };
  }

  /** F57：拆分——组内单个成员 → 一台独立机（label=别名,用成员级 port/proxyJump 精确还原,无备用地址）。 */
  private memberToCfg(g: ImportGroup, m: ImportMember): RemoteHostConfig {
    return {
      label: m.alias,
      host: m.host,
      port: m.port || 22,
      user: g.user,
      keyPath: g.keyPath ?? "",
      daemonPath: g.user ? defaultDaemonPathFor(g.user) : "",
      hostKeyFingerprint: "",
      addresses: [],
      jump: m.proxyJump ?? "",
      daemonless: false, // F59：从 ssh config 导入的主机默认走 daemon 路径
      resumeCommand: "", // S4b-3：空 = 沿用全局默认（导入时无从得知这台该用什么）
    };
  }

  /** F57：批量导入预览弹框——列各聚合组,勾选包含 / 拆分成独立机 / 改 label,确认建卡。 */
  private showImportPreview(groups: ImportGroup[]): void {
    type Row = {
      g: ImportGroup;
      include: boolean;
      split: boolean;
      label: string;
    };
    const state: Row[] = groups.map((g) => ({
      g,
      include: true,
      split: false,
      label: g.label,
    }));

    const back = document.createElement("div");
    back.className = "import-preview-back";
    const box = document.createElement("div");
    box.className = "import-preview-box";
    const title = document.createElement("div");
    title.className = "import-preview-title";
    title.textContent = `从 ~/.ssh/config 导入（检测到 ${groups.length} 台）`;
    box.appendChild(title);

    const list = document.createElement("div");
    list.className = "import-preview-list";
    for (const s of state) {
      const src = s.g.members.map((m) => m.alias).join(", ");
      const addrHint = s.g.addresses.length
        ? ` +${s.g.addresses.length} 备用地址`
        : "";
      const jumpHint = s.g.jump ? ` · 跳板 ${s.g.jump}` : "";
      const aggLine = `${s.g.host}${addrHint} · ${s.g.user || "(无 user)"}${jumpHint} · 来源: ${src}`;

      const item = document.createElement("div");
      item.className = "import-preview-item";
      const inc = document.createElement("input");
      inc.type = "checkbox";
      inc.checked = true;
      inc.addEventListener("change", () => (s.include = inc.checked));
      item.appendChild(inc);

      const body = document.createElement("div");
      body.className = "import-preview-body";
      const labelInput = document.createElement("input");
      labelInput.type = "text";
      labelInput.className = "import-preview-label";
      labelInput.value = s.label;
      labelInput.addEventListener("input", () => (s.label = labelInput.value));
      body.appendChild(labelInput);
      const info = document.createElement("div");
      info.className = "import-preview-info";
      info.textContent = aggLine;
      body.appendChild(info);
      item.appendChild(body);

      if (s.g.members.length > 1) {
        const splitWrap = document.createElement("label");
        splitWrap.className = "import-preview-split";
        const split = document.createElement("input");
        split.type = "checkbox";
        split.addEventListener("change", () => {
          s.split = split.checked;
          info.textContent = split.checked
            ? `拆成 ${s.g.members.length} 台独立机: ${src}`
            : aggLine;
        });
        splitWrap.append(split, document.createTextNode("拆分"));
        item.appendChild(splitWrap);
      }
      list.appendChild(item);
    }
    box.appendChild(list);

    const foot = document.createElement("div");
    foot.className = "import-preview-foot";
    const cancel = document.createElement("button");
    cancel.type = "button";
    cancel.className = "settings-btn";
    cancel.textContent = "取消";
    cancel.addEventListener("click", () => back.remove());
    const confirm = document.createElement("button");
    confirm.type = "button";
    confirm.className = "settings-btn settings-btn-primary";
    confirm.textContent = "导入";
    confirm.addEventListener("click", () => {
      back.remove();
      void this.applyImportPreview(state);
    });
    foot.append(cancel, confirm);
    box.appendChild(foot);

    back.addEventListener("click", (e) => {
      if (e.target === back) back.remove();
    });
    back.addEventListener("keydown", (e) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        back.remove();
      }
    });
    back.appendChild(box);
    document.body.appendChild(back);
    box.querySelector("input")?.focus();
  }

  /** F57：把预览里勾选的组建成机器卡（拆分组建多台;同名 label 已存在则跳过）。 */
  private async applyImportPreview(
    state: Array<{
      g: ImportGroup;
      include: boolean;
      split: boolean;
      label: string;
    }>,
  ): Promise<void> {
    // 已存在的卡（导入前）→ 撞到就跳过（不重复导入）。批内新机同名 → 加后缀消歧（不丢机,F57-1）。
    const preExisting = new Set(
      this.cards.map((c) => {
        const cfg = c.collect();
        return cfg.label.trim() || cfg.host;
      }),
    );
    const usedInBatch = new Set<string>();
    let added = 0;
    let skipped = 0;
    const push = (cfg: RemoteHostConfig): void => {
      const base = cfg.label.trim() || cfg.host;
      if (preExisting.has(base)) {
        skipped++;
        return;
      }
      let key = base;
      let n = 2;
      while (usedInBatch.has(key)) key = `${base}-${n++}`;
      if (key !== base) cfg.label = key; // 批内同基名不同机 → 后缀消歧,绝不丢机
      usedInBatch.add(key);
      this.appendCard(cfg);
      added++;
    };
    for (const s of state) {
      if (!s.include) continue;
      if (s.split) {
        for (const m of s.g.members) push(this.memberToCfg(s.g, m));
      } else {
        push(this.groupToCfg(s.g, s.label));
      }
    }
    if (added > 0) await this.save();
    this.showBanner(
      `批量导入完成：新增 ${added} 台${skipped ? `，跳过 ${skipped} 台（同名已存在）` : ""}。`,
    );
  }

  /**
   * Feature ②：远端 ↗ 拉前的只读 `ccm` wrapper 片段。纯 DOM/信息展示，不读写 config。
   */
  private buildWrapperSnippetRow(parent: HTMLElement): void {
    // F81（#40）：默认折叠——原生 <details> 不带 open 属性即收起；点标题（<summary>）展开看片段。
    // 片段占位大、多数人配一次不再看，故默认收起。**不加 .settings-row（display:flex）**——flex 的
    // <details> 在部分浏览器折叠会失效（闭合仍渲染全部子元素）；用块流 + 子元素自身外边距排布。
    const row = document.createElement("details");
    row.className = "remote-wrapper-details";

    const label = document.createElement("summary");
    label.className = "settings-label remote-wrapper-summary";
    label.textContent = "远端 ↗ 拉前（可选）";
    label.appendChild(
      makeInfoIcon(
        "用 `ccm` 起会话（而非直接 `claude`），远端会周期性把 ssh 窗口标题设成\n" +
          "`ccm-rbind-<sid>`，本地 monitor 扫到即绑定该窗口；同时给 tmux 打上 @ccm_sid，\n" +
          "于是终端起的会话 app 也认得出、能 attach、能换号重启。\n\n" +
          "✅ 每台机器卡片上的「装 ccm 启动器」按钮一键装好（CLI 到 ~/.local/bin/ccm，\n" +
          "别名块到 ~/.bashrc，先备份、幂等可重装）；下面片段是别名块，可手动复制\n" +
          "（zsh / 自定义 profile 用；CLI 本体仍需用按钮部署）。\n\n" +
          "⚠ 限制：多个 ssh 会话若开在同一个 Windows Terminal 窗口的不同 tab 里，↗ 只能\n" +
          "拉起该窗口、无法切到具体 tab。建议每个远端会话单独开窗。",
      ),
    );
    row.appendChild(label);

    // T03：改走统一的待贴块。**这一处此前有两个真缺陷**，只有把三个待贴落点放到一起
    // 数才看得见：① 没有粘后指引（另两处都有）；② 复制失败被 `console.warn` 吞掉
    // ——用户点了「复制」，按钮不变、没有任何提示，然后去粘贴，粘到的是上一次剪贴板里的东西。
    row.appendChild(
      buildPasteBlock({
        text: () => CCM_WRAPPER_SNIPPET,
        target: "这台远端的 ~/.bashrc（或它实际用的 shell 配置文件）",
        mergeNote:
          "追加到文件末尾即可；里面是带 BEGIN/END 围栏的块，重复贴会有两份，先删掉旧的那一份。",
        activation: "source 它，或在该远端开一个新的登录 shell。",
        multiline: true,
        rows: 10,
        // 指回已有规则的那个 class（迁移时改名成 `-paste` 让 styles.css:3480
        // 那条规则失去了宿主，而新名字一条规则都没有）
        className: "remote-wrapper-snippet",
      }).element,
    );

    parent.appendChild(row);
  }

  // === 数据 ===

  /** 读出当前整段 RemoteConfig（enabled + 所有卡片）。 */
  private collect(): RemoteConfig {
    return {
      enabled: this.enabledCheckbox.checked,
      hosts: this.cards.map((c) => c.collect()),
    };
  }

  /** 任一控件变化 → 组装 → merge 进 config.json → 提示重启。 */
  private async save(): Promise<void> {
    const next = this.collect();
    // best-effort UI 校验：启用但某台缺必填字段 → 软提示（不拦保存，后端会跳过该台）。
    const incompleteCount = next.enabled
      ? next.hosts.filter((h) => !h.host || !h.user || !h.daemonPath).length
      : 0;
    // 指纹格式软校验：非空且不以 SHA256: 开头 → 大概率粘错字段。
    const fingerprintLooksOff = next.hosts.some(
      (h) =>
        !!h.hostKeyFingerprint && !h.hostKeyFingerprint.startsWith("SHA256:"),
    );

    if (incompleteCount > 0) {
      this.showBanner(
        `已保存，但有 ${incompleteCount} 台 host/user/daemonPath 不完整 —— 后端会跳过这些台。补全后重启 monitor 才会连。`,
      );
    } else if (fingerprintLooksOff) {
      this.showBanner(
        "已保存。注意：某台主机指纹不是 `SHA256:` 开头格式 —— 请确认没粘错。",
      );
    }

    try {
      // ★ S1：**局部合并，不再整表覆盖**。
      //
      // 老写法是 `writeRemoteConfig(next)` —— 把 `cfg.remote` 整个换成本编辑器手上这份。
      // 它今天之所以不出事，纯粹是因为 `collect()` 恰好映射了**全部**卡片：
      // **正确性来自 UI 的巧合，不是来自构造**。S2 一旦把机器拆成一页一台，
      // 同一句调用就会把不在本页的机器**静默删光**。
      //
      // 现在改成显式的 upsert/remove：
      // - upsert 用每张卡的 `persistedKey` 定位盘上那一条 ⇒ 改 label（换 origin）
      //   仍然是**改**那一条，不会变成「新增 + 孤儿」。
      // - remove 只取「本编辑器加载时见过、现在卡片没了」的那些 ⇒ 没加载过的机器
      //   既不 upsert 也不 remove，**字节不动**。
      // 按**出现次数**比，不是按集合比。集合比在「两台机器 origin 相同、删掉其中一台」
      // 时会算出 remove=[]（另一张卡还占着同一个 key）⇒ 删除静默失效。
      // 老的整表覆盖写法没这个问题，所以这属于必须挡住的回归。
      const countBy = (keys: (string | null)[]): Map<string, number> => {
        const m = new Map<string, number>();
        for (const k of keys) if (k !== null) m.set(k, (m.get(k) ?? 0) + 1);
        return m;
      };
      const liveCount = countBy(this.cards.map((c) => c.persistedKey));
      await patchRemoteConfig({
        enabled: next.enabled,
        upsert: this.cards.map((c) => ({
          key: c.persistedKey,
          value: c.collect(),
        })),
        remove: [...countBy(this.loadedKeys)]
          .filter(([k, n]) => n > (liveCount.get(k) ?? 0))
          .map(([k]) => k),
      });
      // 落盘成功后卡片身份跟到新 origin 上（用户这次可能就是在改名）。
      for (const c of this.cards) {
        const next = hostKey(c.collect());
        // S3：状态账本跟着改名走，否则改个名字那几格就凭空清零。
        if (c.persistedKey && c.persistedKey !== next) {
          renameMachine(c.persistedKey, next);
        }
        c.persistedKey = next;
        c.renderStatusStrip();
      }
      this.loadedKeys = this.cards.map((c) => c.persistedKey!);

      const changed = !sameRemote(next, this.original);
      this.original = next;
      if (changed && incompleteCount === 0 && !fingerprintLooksOff) {
        // S7：「要重启」这个**状态**收敛到底部常驻条，banner 只报「这次动作成功了」。
        // 原先每次保存都在 banner 里重说一遍「需要重启」—— 那句话恒真，说多了就成噪音，
        // 真该注意时反而认不出来（§12）。
        markRestartNeeded("远端机器配置");
        this.showBanner("远端设置已更新。");
      }
    } catch (e) {
      console.warn("save remote config failed:", e);
      this.showBanner(`保存失败：${String(e)}`);
    }
  }

  private showBanner(text: string): void {
    this.banner.textContent = text;
    this.banner.classList.add("settings-banner-show");
  }

  private hideBanner(): void {
    this.banner.textContent = "";
    this.banner.classList.remove("settings-banner-show");
  }
}

// === config.json 读写（多机，向后兼容旧单对象）===

/** 把一个任意 JSON 对象规整成 RemoteHostConfig（缺失/类型不对走默认）。 */
// F12：`coerceAddresses` / `coerceHost` / `readRemoteConfig` / `findHostByOrigin` /
// `resolveRemoteConfigByOrigin` / 写入口已移入 `src/remote-config.ts`（数据层）。
// S1：写入口 = `patchRemoteConfig`（局部合并）；整表覆盖的 `writeRemoteConfig` 已收回该文件内部、不再导出。
// `sameHost` / `sameRemote`（下方）是 UI dirty-check，留本文件。

function sameHost(a: RemoteHostConfig, b: RemoteHostConfig): boolean {
  return (
    a.label === b.label &&
    a.host === b.host &&
    a.port === b.port &&
    a.user === b.user &&
    a.keyPath === b.keyPath &&
    a.daemonPath === b.daemonPath &&
    a.hostKeyFingerprint === b.hostKeyFingerprint &&
    a.jump === b.jump && // F56（D-I3）:仅改跳板也算变更，触发「需重启生效」提示
    a.daemonless === b.daemonless && // F59:仅改 daemonless 开关也算变更（触发「需重启生效」）
    // F45（Phase G 补）:仅改「备用地址」也算变更。此前独漏 addresses（jump/daemonless 都比了）
    // → 只改多地址、其它不动时「需重启生效」横幅被静默抑制，用户可能不重启、新地址不生效。
    a.addresses.length === b.addresses.length &&
    a.addresses.every((x, i) => x === b.addresses[i])
  );
}

function sameRemote(a: RemoteConfig, b: RemoteConfig): boolean {
  return (
    a.enabled === b.enabled &&
    a.hosts.length === b.hosts.length &&
    a.hosts.every((h, i) => sameHost(h, b.hosts[i]))
  );
}

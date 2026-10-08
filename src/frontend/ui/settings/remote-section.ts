/**
 * 设置面板「远端 (SSH)」区（SSH-remote issue #15 / 多机 #30）。
 *
 * 让用户配置 + 启用「远端模式」：monitor 通过 SSH 连到 **0..N 台** 远端主机，由各台的
 * backend 作为额外数据源（与本机后端那一路聚合 ——本机会话内容 CF1 起也走本机后端，不再是 monitor 自己读）。配置写入 config.json 的 `remote`
 * 子对象（`{ enabled, hosts: [...] }`），由 Rust 侧 `lib.rs::load_remote_configs` 启动时读。
 *
 * **camelCase key 必须与 Rust reader 严格一致**（否则后端读不到）：
 *   enabled (bool) / hosts[] 内每台：label (string, 可选默认 host) / host / port (默认 22) /
 *   user / keyPath (可选) / hostKeyFingerprint (可选)（`backendPath` 删了：落点恒是那台的 `~/.cc-monitor/bin/ccm`）
 *
 * 旧的单对象 `remote: { enabled, host, ... }`（无 `hosts` 键）**不再认**：一台都不显示，
 * 机器列表顶上说「远端配置认不出：…」（`remote-config.ts::REMOTE_CONFIG_UNRECOGNIZED`）。
 *
 * 设计（对齐 behavior.ts / diagnostics-section.ts 范式）：
 * - 读走 config.ts 的 loadConfig；写经 `remote-config.ts::patchRemoteConfig`（schema-agnostic 透传）。
 * - **只动 `remote` 这一个键**：写口是按键补丁（`config.ts::patchConfigFrom` 现读 → 只交 `set ["remote"]`），
 *   盘上别的键不经这里。
 * - 改动后需**重启 monitor 才生效**（数据源在 setup() 启动时定型），保存后 banner 提示。
 * - 每次输入 change 立即保存（无"未保存"中间态）→ refresh() 可安全从 config 重建卡片。
 *
 * Tier 1（issue #15）：从 ~/.ssh/config 导入别名（`ssh -G`）→ 作为**新机器**加入列表；
 * 每台各有「测试连接」（`test_remote_connection`）展示 SSH/指纹/backend，指纹可一键固化。
 */

import { commands } from "../ipc/commands";
import { openPortForwardPanel } from "../views/port-forward";
import { listForwards, stopForward } from "../port-forward-reads";
import { askInterrupts } from "./interrupts";
// F12：配置数据层已抽到 src/frontend/ui/remote-config.ts（治分层倒挂）——UI 从数据模块 import，不再自持 CRUD。
import {
  readRemoteConfig,
  tryRemoteConfig,
  patchRemoteConfig,
  hostKey,
  findHostByOrigin,
  type RemoteHostConfig,
  type RemoteConfig,
} from "../remote-config";
import {
  MachineCard,
  shouldShowResetFingerprint,
  type MachineCardParts,
} from "./machine-card";
import { button } from "../kit/button";
import { icon } from "../kit/icon";
import { openMenu, type MenuItem } from "../kit/menu";
import { statusDot, setDot } from "../kit/status-dot";
import { toast, undoToast } from "../kit/toast";
import { forgetSeen } from "../last-seen";
import { openAddMachine } from "./add-machine";
import { localMeta, machineMeta, NO_FACTS, type MachineFacts, type MachineSection } from "./machine-page";
import { fetchAccounts, fetchLocalAccounts } from "../account-reads";
import { machineFace, paintProblem, type MachineFace, type MachineFix, type MachineState } from "./machine-state";
import { isLocalOrigin } from "../ipc/origin";
import type { AccountsState } from "../accounts";
import { moveMachinePrefs } from "../account-prefs";
// K-P1/P2s：本机后端那条把手的 origin（后端注册表里的键 `inbound_client::LOCAL_ORIGIN`）。**与本机那一页的路由键不是同一个串**。
import { LOCAL_ORIGIN } from "../backend-policy";
// 旧调用点从本模块 import 这两个（测试也是）——搬家后原样再导出，不制造无谓的改动面。
export { shouldShowResetFingerprint };
import { makeInfoIcon } from "./info-icon";

// C04d 批 5c：五个类型换成生成物（源 `stream_source/`）。手写版与生成物**逐字等价** ⇒ 零漂移。
import { importSshHosts } from "../ssh-config-reads";

// E80（2026-08-01）：`describeStage` 与 `ConnectStage` 的再导出**搬去 `machine-card.ts`**。
//
// 那两样此前住在这里，而唯一的消费者是 `machine-card.ts` —— 于是 `machine-card`（从本文件
// 抽出去的那个）回头 import 本文件的**值**，构成一条真的运行期 import 环
// （`remote-section → machine-card → remote-section`）。今天不炸只是因为两边的用点都在
// 方法体里、模块求值期不触发，属 TDZ 型隐患；而本仓的 eslint 没有 `import/no-cycle`，
// 环在这里是**结构性不可见**的（Phase G 代码工程视角独立写 DFS 才扫出来）。
// ⇒ 把只有一个消费者的东西搬到那个消费者身边，环就没了。环守卫见 `tests/frontend/ui/import-cycle-guard.vitest.ts`。

// F12：`RemoteHostConfig` / `RemoteConfig` / `parseAddressLines` / `sftpEligibleHosts` 已移入
// `src/frontend/ui/remote-config.ts`（数据层），本文件从那里 import（见顶部）。



// 做成函数、用到时才取文（模块顶层不留取文口调用 —— 顶层调用会让 Rollup 把设置面板挪进主窗共享 chunk）。
const REMOTE_INFO_TEXT = (): string =>
  copyText("remote.info.remote");

/**
 * 远端的别名块：只做**组合**（`cct() { ccm --tmux "$@"; }`），不含任何实现 —— 实现都在 `ccm` 里。
 * 不装成 shell 函数的理由：函数**优先于 PATH**，与用户已有同名函数硬冲突且必然遮蔽；远端是 zsh/fish 时 `.bashrc` 根本不被 source。
 */
// 单一来源：src/shared/ccm-aliases.sh（后端 sftp.rs include_str! 同一文件，杜绝漂移）
import { copyText } from "../copy-table"; // T03：待贴文本统一组件

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
    parts?: MachineCardParts,
  ): void;
  removeMachinePage(id: string): void;
  navigateToMachinePage(id: string): void;
  /** 机器改了名称：那一页的导航项与页头跟着改（没有这一页 ⇒ 不动）。 */
  renameMachinePage?(id: string, title: string): void;
  /**
   * 🔴 步 3：**这一趟「同步机器页」收尾了**（成或败都叫一次）。
   *
   * 宿主要它是为了分开两件在屏幕上长得一样的事：
   * 「还在加载」与「一个机器页都注册不出来」。没有这个回调，宿主只能靠定时器猜 ——
   * 而猜错的方向正好是本件要治的那一个（让兜底态提前露脸）。
   *
   * ⚠ 可选：不带路由器的宿主（既有单测）不必实现它。
   */
  machinePagesSettled?(): void;
  /** 进那台的页并展开卡头里的「连接设置」/「这台上的 cc-monitor」。 */
  openMachineSection?(id: string, section: MachineSection): void;
  /** 重问那台的连接（⋯ →「刷新」）。 */
  refreshMachine?(origin: string): void;
  /** 机器表刚对齐过（有台起了 / 断了）：宿主重问后端那几台、重画各台的点。 */
  machinesReconciled?(): void;
  /** 问题行上的修法按钮点了（那一页 · 哪一颗）。 */
  runFix?(pageId: string, fix: MachineFix): void;
}

/**
 * 机器列表那一行上**别人挂进来的格子**。
 *
 * 用途只有一个：DAEMON 开关（后端的 状态 / 操作 / 退出行为 / 健康 四格）**并进列表行**，
 * 不再在列表页上单独占一块。本分节不认识那四格长什么样 —— 只管「每一行给它留个位置」。
 */
export interface MachineRowExtras {
  /** 列表末尾：后端清单里有、机器列表里没有的那几台。 */
  tail(): HTMLElement;
}

export interface RemoteSectionOptions {
  /** 被别的分组包起来时传 headless: true，不渲染自己的小标题。 */
  headless?: boolean;
  /** 见 `MachineRowExtras`。不传就是老形态（行上只有名字 ＋ 状态条）。 */
  rowExtras?: MachineRowExtras;
  /**
   * S4b：有它就把每台机器的编辑表单搬到**它自己那一页**，列表里只留一行
   * （名字 + 状态 + 点进去）。**不传就是老形态**（卡片就地折叠展开）——
   * 既有单测与任何不带路由器的宿主照常工作。
   */
  pages?: MachinePagesHost;
}

/** S4b：机器详情页的路由 id 前缀。 */
export const MACHINE_PAGE_PREFIX = "machine:";
/**
 * 本机那一页的路由键。本机没有 origin 那一格的写法（它不走 ssh），用一个**不可能与真实 origin 撞车**的名字：
 * origin 来自 `label || host`，用户填不出带中文括号的 host，label 也不会长这样。
 */
const LOCAL_MACHINE_KEY = "（本机）";
/** S4b-2：本机那一页的路由 id。 */
export const LOCAL_MACHINE_PAGE_ID = `${MACHINE_PAGE_PREFIX}${LOCAL_MACHINE_KEY}`;

/**
 * 列表那一行右侧那一句：照那台账号库的成品排（做不了多账号 · 没启用 · `N 个账号 · 默认 X`）；问不到 ⇒ 空。
 * `os` 是那台的系统名（报了才有）。
 */
export function accountsSummary(state: AccountsState, os: string | null): string {
  // 这一次没问到 ⇒ 画这次运行里那台最近一次答成的那一份（离线 / 要更新那台照出上次的值）。
  const got = state.available ? { meta: state.meta, accounts: state.accounts } : (state.last ?? { meta: null, accounts: [] });
  const meta = got.meta;
  if (meta === null) return "";
  if (meta.unsupported) return os ? copyText("machineList.summary.single", { os }) : copyText("machineList.summary.singleBare");
  if (!meta.enabled) return copyText("machineList.summary.notEnabled");
  const n = got.accounts.length;
  const def = got.accounts.find((a) => a.isDefault);
  return def ? copyText("machineList.summary.accounts", { n, name: def.name }) : copyText("machineList.summary.count", { n });
}

/** 删掉的那台经它在转的端口转发一条条停掉（本机后端那本转发账；停不掉的只进日志）。 */
async function stopForwardsOf(origin: string): Promise<void> {
  try {
    for (const f of await listForwards()) if (f.origin === origin) await stopForward(f.id);
  } catch (e) {
    console.warn(`[remove] ${origin} 的端口转发没停干净：`, e);
  }
}

// === 共享 DOM 小工具 ===







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
  /**
   * Phase G：卡片 → 它那一页的 id。**创建时写一次，之后只读**。
   * 用 `Map` 而不是把 id 挂到 `MachineCard` 上：路由 id 是**列表这一层**的概念，
   * 卡片不该知道自己被谁注册成了哪一页。
   */
  private pageIdOf = new Map<MachineCard, string>();
  /** S5/E56：「还差什么」清单容器。 */
  private countLine!: HTMLElement;
  /** 后端报来的各台事实（系统 · 版本），按后端那套名字。 */
  private readonly facts = new Map<string, MachineFacts>();
  /** 各页最近一次照成品画出的样子（数「几台离线 · 几台要更新」用）。 */
  private readonly faces = new Map<string, MachineFace>();
  /** 每台最近一帧的状态成品（比对指纹要那台出示的那一枚）。 */
  private readonly states = new Map<string, MachineState>();
  /** 各页最近一次喂进来的连接（数「几台离线」用）。 */
  private readonly connected = new Map<string, boolean | null>();

  /** 打开面板时从 config 拉到的快照，用于判断是否变化（变了就提示重启）。 */
  private original: RemoteConfig = { hosts: [] };

  /**
   * S1：本编辑器**加载时**看到的机器 key 列表。保存时 `remove = loadedKeys − 现存卡片的 key`。
   * 基准取「加载时看到的」而非「盘上全量」，是为了让 S2 拆页后一页只对自己那几台负责。
   */
  private loadedKeys: string[] = [];

  private machinesContainer!: HTMLElement;
  private emptyHint!: HTMLElement;
  private banner!: HTMLElement;
  /** `remote` 段认不出时那一句（常驻，不像 banner 会被下一次动作冲掉）。 */
  private unrecognizedNote!: HTMLElement;
  private importHint!: HTMLElement;

  private cards: MachineCard[] = [];

  /** 见 `MachineRowExtras`。 */
  private rowExtras?: MachineRowExtras;
  /** 列表尾巴（`rowExtras.tail()`）。新加的行插在它前面，它永远在最后。 */
  private rowsTail: HTMLElement | null = null;

  constructor(opts: RemoteSectionOptions = {}) {
    this.headless = opts.headless ?? false;
    this.pages = opts.pages;
    this.rowExtras = opts.rowExtras;
    this.root = this.build();
    // 步 4：`refresh()` 自己会把失败画到这一块的 banner 上（见它的 catch），
    // 这里再收一次是为了**不产生未捕获 rejection** —— 那条路的终点是状态栏，
    // 而状态栏不是这一块的错误该去的地方。
    void this.refresh().catch(() => {});
  }

  get element(): HTMLElement {
    return this.root;
  }

  /** 设置面板每次 open 时调，确保展示的是 config.json 里的最新值。 */
  async refresh(): Promise<void> {
    // 步 3：**成也好败也好，收尾时告诉宿主一声。**
    // `readRemoteConfig()` reject 时这个方法是 `void this.refresh()` 掉的一个
    // 未捕获 rejection ⇒ 一个机器页都不会注册，而宿主那边只看得到「什么都没来」。
    // `finally` 让两条路都经过这里。
    // 🔴 那个「自己从 ☐ 跳到 ☑」的复选框（`§8` 判据 #2）：读回来之前**不可交互**。
    //    否则用户在它变之前以为它是关的、点一下，结果是把它关掉（而他以为自己在打开）。
    //    读失败就一直灰着 —— 那一刻它显示的值不是盘上的值，点它就是写一个假状态回去。
    try {
      this.original = await readRemoteConfig();
      this.rebuildCards(this.original.hosts);
      this.unrecognizedNote.textContent = this.original.unrecognized ?? "";
      // ⚠ 用类不用 `hidden`：`.settings-banner-show` 的 `display: block` 盖得过 `hidden` 属性。
      this.unrecognizedNote.classList.toggle(
        "settings-banner-show",
        this.original.unrecognized !== undefined,
      );
      this.hideBanner();
    } catch (e) {
      // 🔴 步 4：**异步失败落在这一块上**，不再只打到状态栏。
      //
      // 这个方法的两个调用点都是 `void this.refresh()`（本类构造器 ＋ `panel.open()`），
      // 而 `void` 掉的 Promise 其 reject 是**未捕获 rejection** ⇒ 今天它一路走到
      // `main.ts` 那条全局兜底，变成状态栏上一行 `REJ: …`
      //（截图里那句 `REJ: Command plugin:dialog|confirm not allowed
      //   by ACL` 就是这条路出来的）。状态栏离出事的那一块十万八千里，用户看不出
      //   「机器列表为什么是空的」。
      // ⇒ 就地说一句，并把异常继续往外抛（调用方要判成不成功，本行只负责说出口）。
      this.showBanner(copyText("remote.refresh.failed", { e: String(e) }));
      throw e;
    } finally {
      this.pages?.machinePagesSettled?.();
    }
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
    const row = this.buildRow(LOCAL_MACHINE_PAGE_ID, copyText("remote.localRow.title"), localMeta("list", this.factsOfOrigin(LOCAL_ORIGIN)), LOCAL_ORIGIN);
    row.classList.add("remote-machine-local");
    return row;
  }

  /**
   * 列表里的一行（本机远端同形）：点 · 名字（不健康时旁边一个词）· 地址与系统 · 右侧账号数 · ⋯；
   * 掉线时下面一行问题行 ＋ 修法。点整行进那台的页。
   */
  private buildRow(pageId: string, name: string, meta: string, origin: string): HTMLElement {
    const row = document.createElement("div");
    row.className = "remote-machine-row";
    row.dataset.pageId = pageId;
    row.dataset.origin = origin;
    row.addEventListener("click", (ev) => {
      if ((ev.target as HTMLElement).closest("button") && !(ev.target as HTMLElement).closest(".remote-machine-open")) return;
      this.pages?.navigateToMachinePage(pageId);
    });
    const dot = statusDot("unknown", copyText("settingsNav.dot.unknown", { machine: name }), "compact");
    dot.classList.add("remote-machine-dot");
    const main = document.createElement("div");
    main.className = "remote-machine-main";
    const line = document.createElement("div");
    line.className = "remote-machine-line";
    const nameBtn = document.createElement("button");
    nameBtn.type = "button";
    nameBtn.className = "remote-machine-name remote-machine-open";
    nameBtn.textContent = name;
    const word = document.createElement("span");
    word.className = "remote-machine-word";
    line.append(nameBtn, word);
    const metaEl = document.createElement("div");
    metaEl.className = "remote-machine-meta";
    metaEl.textContent = meta;
    main.append(line, metaEl);
    const side = document.createElement("div");
    side.className = "remote-machine-side";
    const summary = document.createElement("span");
    summary.className = "remote-machine-summary";
    const more = button({ label: copyText("machinePage.head.more"), kind: "icon", icon: "more", size: "compact", hint: copyText("machinePage.head.more") });
    more.addEventListener("click", (ev) => {
      ev.stopPropagation();
      openMenu({ el: more, align: "end" }, this.menuFor(pageId));
    });
    side.append(summary, more);
    const problem = document.createElement("div");
    problem.className = "machine-problem";
    problem.hidden = true;
    row.append(dot, main, side, problem);
    return row;
  }

  /**
   * 右侧那一句：那台账号库的概况（`N 个账号 · 默认 X` ／ 未启用多账号 ／ `Windows · 单账号`）。
   * 连上了才问（问的是已连着的那条通道，不另拨）；问不到 ⇒ 空着。
   */
  private async loadSummary(row: HTMLElement, link: "up" | "down"): Promise<void> {
    const origin = row.dataset.origin ?? "";
    if (origin === "" || row.dataset.summaryAsked === link) return;
    row.dataset.summaryAsked = link;
    const state = isLocalOrigin(origin) ? await fetchLocalAccounts() : await fetchAccounts(origin);
    const box = row.querySelector<HTMLElement>(".remote-machine-summary");
    if (box) box.textContent = accountsSummary(state, this.factsOfOrigin(origin).os);
  }

  /** 那台后端报来的事实（系统 · 版本）；还没报 ⇒ 两格都空。 */
  private factsOfOrigin(origin: string): MachineFacts {
    return this.facts.get(origin) ?? NO_FACTS;
  }

  /**
   * 宿主把后端报来的那台的事实喂进来：列表那一行 · 卡头的第二行跟着换。
   * `origin` 是后端那套名字（本机 `<local>`）。
   */
  setFacts(origin: string, facts: MachineFacts): void {
    this.facts.set(origin, facts);
    const pageId = isLocalOrigin(origin) ? LOCAL_MACHINE_PAGE_ID : this.pageIdOfMachine(origin);
    if (!pageId) return;
    const meta = this.findMachineRow(pageId)?.querySelector<HTMLElement>(".remote-machine-meta");
    if (meta) meta.textContent = this.listMetaOfPage(pageId);
  }

  /** 宿主照后端的回答喂进来：那一行的点 · 词 · 问题行。 */
  setConnected(pageId: string, connected: boolean | null, disabled = this.isDisabledPage(pageId)): void {
    const row = this.findMachineRow(pageId);
    if (!row) return;
    if (disabled) {
      this.paintRowDisabled(row, pageId);
      this.paintCount();
      return;
    }
    const name = row.querySelector<HTMLElement>(".remote-machine-open")?.textContent ?? "";
    const dot = row.querySelector<HTMLElement>(".remote-machine-dot");
    if (dot) {
      if (connected === true) setDot(dot, "up", copyText("settingsNav.dot.up", { machine: name }));
      else if (connected === false) setDot(dot, "failed", copyText("settingsNav.dot.down", { machine: name }));
      else setDot(dot, "unknown", copyText("settingsNav.dot.unknown", { machine: name }));
    }
    const word = row.querySelector<HTMLElement>(".remote-machine-word");
    if (word) word.textContent = connected === false ? copyText("machinePage.state.down") : "";
    const problem = row.querySelector<HTMLElement>(".machine-problem");
    if (problem) {
      problem.replaceChildren();
      problem.hidden = connected !== false;
      if (connected === false) {
        const text = document.createElement("span");
        text.textContent = copyText("machinePage.problem.offline", { machine: name });
        problem.append(icon("error", "compact"), text);
        if (pageId !== LOCAL_MACHINE_PAGE_ID) {
          problem.appendChild(
            button({ label: copyText("machinePage.conn.toggle"), size: "compact", onClick: (ev) => {
              ev.stopPropagation();
              this.pages?.openMachineSection?.(pageId, "conn");
            } }),
          );
        }
      }
    }
    this.connected.set(pageId, connected);
    this.paintCount();
    if (connected === true) void this.loadSummary(row, "up");
    [...this.pageIdOf.entries()].find(([, id]) => id === pageId)?.[0].showLive(connected === true);
  }

  /** 宿主照后端的状态成品喂进来：那一行的点 · 词 · 问题行 ＋ 修法；页头那一句按它数。 */
  setMachine(pageId: string, m: MachineState): void {
    const row = this.findMachineRow(pageId);
    if (!row) return;
    const name = row.querySelector<HTMLElement>(".remote-machine-open")?.textContent ?? "";
    const face = machineFace(m, name);
    const dot = row.querySelector<HTMLElement>(".remote-machine-dot");
    if (dot) setDot(dot, face.dot, face.word ? `${name} · ${face.word}` : copyText("settingsNav.dot.up", { machine: name }));
    const word = row.querySelector<HTMLElement>(".remote-machine-word");
    if (word) word.textContent = face.word;
    const problem = row.querySelector<HTMLElement>(".machine-problem");
    if (problem) paintProblem(problem, face, (fix) => this.runFix(pageId, fix));
    this.faces.set(pageId, face);
    this.states.set(pageId, m);
    const up = m.state === "up" || m.state === "needs_update" || m.state === "newer";
    this.connected.set(pageId, up);
    this.paintCount();
    // 连着 ⇒ 问那台；没连着 ⇒ 问一次拿「上次的」（壳记的最近一次）。连没连着一变就重问。
    if (m.state !== "disabled" && m.state !== "unknown") void this.loadSummary(row, up ? "up" : "down");
    [...this.pageIdOf.entries()].find(([, id]) => id === pageId)?.[0].showLive(up);
  }

  /** 问题行的修法：连接这台 / 连接设置在这里就做得了，其余交宿主。 */
  private runFix(pageId: string, fix: MachineFix): void {
    if (fix === "connect") this.setConnect(pageId, true);
    else if (fix === "conn_settings") this.pages?.openMachineSection?.(pageId, "conn");
    else if (fix === "compare_fingerprint") void this.compareFingerprint(pageId);
    else if (fix === "push_key") this.cardOfPage(pageId)?.pushKey();
    else this.pages?.runFix?.(pageId, fix);
  }

  private cardOfPage(pageId: string): MachineCard | undefined {
    return [...this.pageIdOf.entries()].find(([, id]) => id === pageId)?.[0];
  }

  /** 问题行［比对指纹…］：记下的那枚与那台这一次出示的那枚并排，信任与否在机器卡。 */
  async compareFingerprint(pageId: string): Promise<void> {
    await this.cardOfPage(pageId)?.compareFingerprint(this.states.get(pageId)?.seenHostKey ?? null);
  }

  /** 问题行［更新］：把这一版换到那台上（那一颗住机器卡）。 */
  async updateMachine(pageId: string): Promise<void> {
    await this.cardOfPage(pageId)?.update();
  }

  private paintRowDisabled(row: HTMLElement, pageId: string): void {
    const name = row.querySelector<HTMLElement>(".remote-machine-open")?.textContent ?? "";
    const dot = row.querySelector<HTMLElement>(".remote-machine-dot");
    if (dot) setDot(dot, "exited", copyText("settingsNav.dot.disabled", { machine: name }));
    const word = row.querySelector<HTMLElement>(".remote-machine-word");
    if (word) word.textContent = copyText("machinePage.state.disabled");
    const problem = row.querySelector<HTMLElement>(".machine-problem");
    if (!problem) return;
    const text = document.createElement("span");
    text.textContent = copyText("machinePage.problem.disabled");
    problem.replaceChildren(
      text,
      button({
        label: copyText("machinePage.problem.connect"),
        size: "compact",
        onClick: (ev) => {
          ev.stopPropagation();
          this.setConnect(pageId, true);
        },
      }),
    );
    problem.hidden = false;
  }

  /** 拨那台的「连接这台」（问题行［连接］· 连接设置里的开关）。 */
  setConnect(pageId: string, on: boolean): void {
    const card = [...this.pageIdOf.entries()].find(([, id]) => id === pageId)?.[0];
    card?.setConnect(on);
  }

  /** 页头下那一句：`4 台` ／ `4 台 · 1 台离线` ／ `4 台 · 1 台离线 · 1 台要更新`。 */
  private paintCount(): void {
    const n = this.cards.length + 1;
    const live = (id: string): boolean => this.findMachineRow(id) !== null && !this.isDisabledPage(id);
    const off = [...this.connected.entries()].filter(([id, c]) => {
      const f = this.faces.get(id);
      return live(id) && (f ? f.offline : c === false);
    }).length;
    const upd = [...this.faces.entries()].filter(([id, f]) => live(id) && f.needsUpdate).length;
    const parts = [copyText("machineList.count.all", { n })];
    if (off > 0) parts.push(copyText("machineList.count.off", { a: off }));
    if (upd > 0) parts.push(copyText("machineList.count.update", { b: upd }));
    this.countLine.textContent = parts.join(copyText("kit.text.sep"));
  }

  /** 页头右侧：添加机器 ＋ ⋯（端口转发 · 全部刷新）。 */
  headActions(): HTMLElement[] {
    const add = button({ label: copyText("machineList.head.add"), icon: "plus", onClick: () => void this.openAdd() });
    const more = button({ label: copyText("machinePage.head.more"), kind: "icon", icon: "more", hint: copyText("machinePage.head.more") });
    more.addEventListener("click", () =>
      openMenu({ el: more, align: "end" }, [
        { label: copyText("machineList.menu.portForward"), onClick: () => openPortForwardPanel() },
        {
          label: copyText("machineList.head.refreshAll"),
          onClick: () => {
            void this.refresh().catch(() => {});
          },
        },
      ]),
    );
    return [add, more];
  }

  /** 一台的 ⋯ 菜单（列表那一行与机器页卡头同一份）。 */
  menuFor(pageId: string): MenuItem[] {
    if (pageId === LOCAL_MACHINE_PAGE_ID) {
      return [
        { label: copyText("machineList.menu.refresh"), onClick: () => this.pages?.refreshMachine?.(LOCAL_ORIGIN) },
        { divider: true, label: "" },
        { label: copyText("machineList.menu.cc"), onClick: () => this.pages?.openMachineSection?.(pageId, "cc") },
      ];
    }
    const card = [...this.pageIdOf.entries()].find(([, id]) => id === pageId)?.[0];
    if (!card) return [];
    const origin = card.persistedKey ?? hostKey(card.collect());
    return [
      { label: copyText("machineList.menu.launch"), onClick: () => card.openLauncher() },
      { label: copyText("machineList.menu.files"), onClick: () => card.openFiles() },
      { label: copyText("machineList.menu.portForward"), onClick: () => openPortForwardPanel() },
      { label: copyText("machineList.menu.refresh"), enabled: origin !== "", onClick: () => this.pages?.refreshMachine?.(origin) },
      { divider: true, label: "" },
      { label: copyText("machineList.menu.conn"), onClick: () => this.pages?.openMachineSection?.(pageId, "conn") },
      { label: copyText("machineList.menu.cc"), onClick: () => this.pages?.openMachineSection?.(pageId, "cc") },
      { divider: true, label: "" },
      { label: copyText("machineList.menu.remove"), danger: true, onClick: () => void this.removeCard(card) },
    ];
  }

  /** 那一页那台的 `user@host`（`withPort` ⇒ 带 `:端口`）；不是远端机器页 ⇒ `null`；没填主机 ⇒ 空串。 */
  private whoOfPage(pageId: string, withPort: boolean): string | null {
    const card = [...this.pageIdOf.entries()].find(([, id]) => id === pageId)?.[0];
    if (!card) return null;
    const c = card.collect();
    if (!c.host) return "";
    const who = c.user ? `${c.user}@${c.host}` : c.host;
    return withPort ? `${who}:${c.port || 22}` : who;
  }

  /** 列表那一行的第二行：`user@host · 系统`。 */
  private listMetaOfPage(pageId: string): string {
    if (pageId === LOCAL_MACHINE_PAGE_ID) return localMeta("list", this.factsOfOrigin(LOCAL_ORIGIN));
    const who = this.whoOfPage(pageId, false);
    if (!who) return "";
    return machineMeta("list", who, this.factsOfOrigin(this.originOfPage(pageId) ?? ""));
  }

  /** 机器页卡头第二行：`user@host:端口 · 系统 · 版本 x`（没填主机 ⇒ 空；不是远端机器页 ⇒ `null`）。 */
  metaOfPage(pageId: string): string | null {
    if (pageId === LOCAL_MACHINE_PAGE_ID) return localMeta("head", this.factsOfOrigin(LOCAL_ORIGIN));
    const who = this.whoOfPage(pageId, true);
    if (who === null || who === "") return who;
    return machineMeta("head", who, this.factsOfOrigin(this.originOfPage(pageId) ?? ""));
  }

  /** 用 config 里的机器列表重建卡片。 */
  private rebuildCards(hosts: RemoteHostConfig[]): void {
    // S4b：重建前先把上一批机器页收掉，否则改完配置会留下一串指向已不存在机器的导航项。
    for (const id of this.machinePageIds) this.pages?.removeMachinePage(id);
    this.machinePageIds = [];
    this.cards = [];
    // Phase G：这批卡整个作废，页 id 表跟着清 —— 不清的话 `#n` 去重会把上一批的
    // id 也算进冲突，重建几次之后每台机器的 id 会一路往后飘（`devbox#2`、`devbox#3`…）。
    this.pageIdOf.clear();
    this.machinesContainer.innerHTML = "";
    this.rowsTail = null;
    this.machinesContainer.appendChild(this.buildLocalRow());
    if (this.pages) {
      // 本机页的内容由宿主（panel）填 —— 它拿得到那几块 per-machine 分节，本分节拿不到。
      const localPage = document.createElement("div");
      localPage.className = "machine-page-local";
      this.pages.addMachinePage(LOCAL_MACHINE_PAGE_ID, copyText("remote.cards.local"), localPage);
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
    if (this.rowExtras) {
      this.rowsTail = this.guardedExtra(() => this.rowExtras!.tail());
      this.machinesContainer.appendChild(this.rowsTail);
    }
    this.updateEmptyHint();
    this.paintCount();
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
        onRemove: (c) => void this.removeCard(c),
        onStatusChanged: (c) => this.refreshMachineRow(c),
        tryCells: (c, next) => tryRemoteConfig({ upsert: [{ key: c.persistedKey, value: next, was: c.persistedKey === null ? undefined : findHostByOrigin(this.original.hosts, c.persistedKey) ?? undefined }] }),
      },
      collapsed,
      persistedKey,
    );
    this.cards.push(card);
    if (this.pages) {
      // S4b：表单搬到这台机器自己那一页；列表里只留一行（名字 + 状态 + 点进去）。
      const id = this.assignPageId(card, persistedKey ?? hostKey(initial));
      card.setPageMode();
      this.pages.addMachinePage(id, card.displayName(), card.element, card.parts());
      this.machinePageIds.push(id);
      const row = this.machinesContainer.insertBefore(
        this.buildRow(id, card.displayName(), "", card.persistedKey ?? hostKey(card.collect())),
        this.rowsTail,
      );
      const meta = row.querySelector<HTMLElement>(".remote-machine-meta");
      if (meta) meta.textContent = this.listMetaOfPage(id);
    } else {
      this.machinesContainer.appendChild(card.element);
    }
    this.updateEmptyHint();
    return card;
  }

  /** 往一行上挂宿主给的格子。宿主那一侧抛了**不许把机器列表带走**（同 `safeBlock` 的隔离）。 */
  private guardedExtra(make: () => HTMLElement): HTMLElement {
    try {
      return make();
    } catch (e) {
      console.warn("[remote-section] 行上的附加格子没建起来：", e);
      return document.createElement("span");
    }
  }

  /**
   * S4b：列表里的一行 —— 名字 + 状态条 + 点进去。**编辑表单不在这里**（在那台机器自己那页）。
   *
   * 状态条只渲染在行上，不再渲染在卡片 legend 上：§2.3 那张图里状态就是**列表**的一列，
   * 而详情页上用户看的是那些动作按钮本身的结果，不需要再来一份缓存结论。
   */
  private findMachineRow(pageId: string): HTMLElement | null {
    for (const el of this.machinesContainer.children) {
      if ((el as HTMLElement).dataset?.pageId === pageId) return el as HTMLElement;
    }
    return null;
  }

  /**
   * Phase G：**给这张卡定一个此后不再变的页 id**。
   *
   * # 它修的是两个实测复现的缺陷（都源自「页 id 每次现算」）
   *
   * 原来 `appendCard` / `refreshMachineRow` / `removeCard` 三处各自算一遍
   * `MACHINE_PAGE_PREFIX + (persistedKey ?? hostKey(collect()))` —— 而那个 key 是**会变的**：
   *
   * 1. **连点两次「+ 添加机器」直接抛**：空白卡的 `hostKey()` 是 `""` ⇒ id 恒为 `machine:`，
   *    第二次撞上 `router.addRoute` 的重复注册 `throw`。而 `this.cards.push` 已经执行、
   *    页和行都没建 ⇒ 之后任何一次 `save()` 会把这张**界面上看不见的幽灵卡**写进 `config.json`。
   *    异常在 click handler 里没人接，屏幕上零提示。
   * 2. **改名之后删不掉（UI 侧）**：`save()` 会把 `persistedKey` 改成新 origin，
   *    于是后两处算出的 id 与注册时那个永久分叉 ⇒ `removeMachinePage` 空转、
   *    `findMachineRow` 返回 null ⇒ **列表行 / 导航项 / 详情页三者全留下**，
   *    而盘上那台已经删了。用户看到「删了还在」，再点进那个幽灵页编辑就把机器写回去。
   *
   * ⇒ 身份只在**创建**时定一次，之后一律查表。这与 S1 给 `persistedKey` 立的规矩同源：
   * **「这一条是谁」不能从会变的显示值里现推**。
   *
   * 冲突时加 `#n` 后缀而不是抛：重名是用户输入的正常后果，不该炸掉整个列表
   * （E48「UI 该拦重名」仍然要做，但那是**提示**，不是靠崩溃来阻止）。
   */
  private assignPageId(card: MachineCard, base: string): string {
    // 空白卡（还没填 host/label）没有可读身份 —— 给个占位，别让它变成裸 `machine:`。
    const stem = `${MACHINE_PAGE_PREFIX}${base || "新机器"}`;
    let id = stem;
    for (let n = 2; this.machinePageIds.includes(id); n += 1) id = `${stem}#${n}`;
    this.pageIdOf.set(card, id);
    return id;
  }

  /**
   * 某一页此刻讲的是哪台：按那张卡现在的名字（改过名的是新名）。还没名字的空白卡、不是机器页 ⇒ `null`。
   * 页 id 建卡时定死、不跟着改名走，所以「切到这一页 = 看哪台」要问这里，不能从页 id 里抠。
   */
  /** 这一页的卡还没填主机地址（不是一台连得上的机器：名字填了也一样）。 */
  isUnconfiguredPage(pageId: string): boolean {
    for (const [card, id] of this.pageIdOf) {
      if (id === pageId) return card.collect().host.trim() === "";
    }
    return false;
  }

  /** 这台（按名字）的页 id；列表里没有 ⇒ `null`。 */
  pageIdOfMachine(origin: string): string | null {
    for (const [card, id] of this.pageIdOf) if (hostKey(card.collect()) === origin) return id;
    return null;
  }

  originOfPage(pageId: string): string | null {
    for (const [card, id] of this.pageIdOf) {
      if (id === pageId) return hostKey(card.collect()) || null;
    }
    return null;
  }

  /** 这张卡的页 id。创建时定死，**绝不重算**（理由见 `assignPageId`）。 */
  private pageIdFor(card: MachineCard): string | null {
    return this.pageIdOf.get(card) ?? null;
  }

  /** S4b：刷新某台机器在列表里那一行（名字 + 状态条）。没有分页宿主时什么都不用做。 */
  private refreshMachineRow(card: MachineCard): void {
    if (!this.pages) return;
    const pageId = this.pageIdFor(card);
    if (!pageId) return;
    const row = this.findMachineRow(pageId);
    if (!row) return;
    const nameBtn = row.querySelector<HTMLElement>(".remote-machine-open");
    if (nameBtn) nameBtn.textContent = card.displayName();
    const meta = row.querySelector<HTMLElement>(".remote-machine-meta");
    if (meta) meta.textContent = this.listMetaOfPage(pageId);
    const origin = card.persistedKey ?? hostKey(card.collect());
    if (row.dataset.origin !== origin) {
      row.dataset.origin = origin;
      delete row.dataset.summaryAsked;
    }
    this.pages.renameMachinePage?.(pageId, card.displayName());
  }

  /** 「添加机器」：框里点「添加」才建卡、写盘；没存上 ⇒ 卡收回去、框不关。 */
  /** 机器表里有几台（上一次读回的那一份）。 */
  machineCount(): number {
    return this.original.hosts.length;
  }

  /** 页头［添加机器］与「开始用」第三步同一个口。 */
  addMachine(): Promise<boolean> {
    return this.openAdd();
  }

  private openAdd(): Promise<boolean> {
    return openAddMachine({
      tryAdd: (cfgs) => tryRemoteConfig({ upsert: cfgs.map((value) => ({ key: null, value })) }),
      groups: () => importSshHosts(this.cards.map((c) => c.collect()).map((h) => ({ host: h.host, user: h.user, port: h.port || 22 }))),
      add: async (cfgs) => {
        const cards = cfgs.map((c) => this.appendCard(c));
        if (await this.save()) {
          this.hideBanner();
          toast(copyText("addMachine.save.done", { names: cards.map((c) => c.displayName()).join(copyText("kit.text.sep")) }), "", { level: "success" });
          return null;
        }
        for (const c of cards) this.dropCard(c);
        this.hideBanner();
        return copyText("addMachine.err.save");
      },
    });
  }

  /** 界面上拿掉一张卡（那一行 · 那一页），不写盘。 */
  private dropCard(card: MachineCard): { idx: number; pageId: string | null; row: HTMLElement | null; after: Element | null } {
    const idx = this.cards.indexOf(card);
    const pageId = this.pageIdFor(card);
    const row = pageId ? this.findMachineRow(pageId) : null;
    const after = row?.nextElementSibling ?? null;
    if (idx >= 0) this.cards.splice(idx, 1);
    card.element.remove();
    if (pageId) {
      this.pages?.removeMachinePage(pageId);
      this.machinePageIds = this.machinePageIds.filter((x) => x !== pageId);
      row?.remove();
    }
    this.updateEmptyHint();
    this.paintCount();
    return { idx, pageId, row, after };
  }

  /**
   * 从列表删除（撤得回 ⇒ 不问）：那一行、那一页当场拿掉，出「已删除 X ［撤销］」8 秒；
   * 撤销 ⇒ 原样放回（连接设置一格不丢）；没撤 ⇒ 到点才写盘、清那台的上次值（本机后端记的）、停掉经它的端口转发。
   * 那台正有端口转发 ⇒ 那一句带上「停止转发 n」（数由壳答，`machine_interrupts`）。
   */
  private async removeCard(card: MachineCard): Promise<void> {
    if (!this.cards.includes(card)) return;
    const name = card.displayName();
    const origin = card.persistedKey;
    const forwards = origin ? ((await askInterrupts(origin))?.forwards ?? 0) : 0;
    if (!this.cards.includes(card)) return;
    const { idx, pageId, row, after } = this.dropCard(card);
    let undone = false;
    undoToast(
      forwards > 0 ? copyText("machineList.remove.doneForwards", { machine: name, n: forwards }) : copyText("machineList.remove.done", { machine: name }),
      () => {
        undone = true;
        this.cards.splice(Math.min(idx, this.cards.length), 0, card);
        if (pageId && this.pages) {
          this.pages.addMachinePage(pageId, card.displayName(), card.element, card.parts());
          this.machinePageIds.push(pageId);
        }
        if (row) this.machinesContainer.insertBefore(row, after?.parentNode === this.machinesContainer ? after : this.rowsTail);
        this.updateEmptyHint();
        this.paintCount();
        void this.save();
      },
      () => {
        if (undone) return;
        if (card.persistedKey) void forgetSeen(card.persistedKey);
        this.pageIdOf.delete(card);
        void this.save();
        if (origin && forwards > 0) void stopForwardsOf(origin);
      },
    );
  }

  private updateEmptyHint(): void {
    this.emptyHint.style.display = this.cards.length === 0 ? "block" : "none";
  }

  // === DOM 构建 ===

  private build(): HTMLElement {
    const group = document.createElement("div");
    group.className = this.headless ? "settings-headless" : "settings-group";

    if (!this.headless) {
      const heading = document.createElement("div");
      heading.className = "settings-group-title";
      heading.textContent = copyText("remote.build.title");
      heading.appendChild(makeInfoIcon(REMOTE_INFO_TEXT()));
      group.appendChild(heading);
    }

    this.banner = document.createElement("div");
    this.banner.className = "settings-banner";
    group.appendChild(this.banner);

    // 紧跟在 banner 后面、仍在工具条与列表之上。⚠ 排在 banner **之后**：
    //   `pending-and-block-errors.vitest.ts` 按「这一块第一个 `.settings-banner`」找动作结果那一条。
    this.unrecognizedNote = document.createElement("div");
    this.unrecognizedNote.className = "settings-banner remote-config-unrecognized";
    group.appendChild(this.unrecognizedNote);

    // ★ S4b-3b：**一条工具条**（那张图逐字给的顺序）：
    //   + 添加 · 从 ssh config 导入 · 批量导入 · 端口转发 · [x] 启用远端模式
    //
    // 此前这几个控件散在列表**上下两侧**（导入在最上、端口转发和启用 toggle 在中间、
    // 添加按钮在列表下方），空列表提示还得写「点**下方**添加机器，或从**上方**下拉导入」
    // ——一句提示要同时指两个方向，本身就是布局在报警。
    //
    // 归拢的判据与 §2.1 同源：**它们改的都不是某一台机器的状态，而是这份列表本身**
    //（加一台 / 导入一批 / 全局开关 / 跨机器的隧道台）。per-machine 的东西在机器详情页上。
    this.countLine = document.createElement("div");
    this.countLine.className = "settings-page-sub remote-machine-count";
    group.appendChild(this.countLine);

    this.importHint = document.createElement("div");
    this.importHint.className = "settings-hint";
    this.importHint.style.display = "none";
    group.appendChild(this.importHint);

    // ★ S5 / E56：「还差什么」——新用户一站式的落点。
    // **只读 S3 的账本，不发任何请求**（§1-2）；空的时候整块不渲染，不打扰老用户。
    this.machinesContainer = document.createElement("div");
    this.machinesContainer.className = "remote-machines";
    group.appendChild(this.machinesContainer);

    // 空列表提示。文案跟着布局改：控件全在**上方**那条工具条上了，
    // 不再需要「点下方…或从上方…」这种同时指两个方向的说法。
    this.emptyHint = document.createElement("div");
    this.emptyHint.className = "settings-hint";
    this.emptyHint.textContent =
      copyText("remote.build.empty");
    this.emptyHint.style.display = "none";
    group.appendChild(this.emptyHint);

    // 4. Feature ②：远端 ↗ 拉前用的只读 ccm wrapper 片段

    return group;
  }

  // === 数据 ===

  /** 任一控件变化 → 组装 → merge 进 config.json → 提示重启。存进去了 ⇒ `true`（没存进去的原因已画在横幅上）。 */
  private async save(): Promise<boolean> {
    // 还没落过盘、主机或用户没填的卡（「＋ 添加机器」刚出来那张）不进补丁：
    // 写进去就是一条认不出、删不掉的空机器。它留在界面上，填完了再存。
    const saving = this.cards.filter((c) => {
      const h = c.collect();
      return c.persistedKey !== null || (!!h.host && !!h.user);
    });
    const next: RemoteConfig = { hosts: saving.map((c) => c.collect()) };
    // best-effort UI 校验：某台缺必填字段 → 软提示（不拦保存，后端会跳过该台）。
    const incompleteCount = next.hosts.filter((h) => !h.host || !h.user).length;
    // 指纹格式软校验：非空且不以 SHA256: 开头 → 大概率粘错字段。
    const fingerprintLooksOff = next.hosts.some(
      (h) =>
        !!h.hostKeyFingerprint && !h.hostKeyFingerprint.startsWith("SHA256:"),
    );

    if (incompleteCount > 0) {
      this.showBanner(
        copyText("remote.save.incomplete", { n: incompleteCount }),
      );
    } else if (fingerprintLooksOff) {
      this.showBanner(
        copyText("remote.save.fingerprintShape"),
      );
    }

    try {
      // ★ S1：**局部合并，不再整表覆盖**。
      //
      // 老写法是 `writeRemoteConfig(next)` —— 把 `cfg.remote` 整个换成本编辑器手上这份。 〔散文墓碑〕
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
      const liveCount = countBy(saving.map((c) => c.persistedKey));
      // 每张已在盘上的卡带上加载时那份（origin 在加载时恰好一台的才带）⇒ 数据层按格改、只交动过的格。
      const loadedCount = countBy(this.loadedKeys);
      const wasOf = (k: string | null): RemoteHostConfig | undefined =>
        k !== null && loadedCount.get(k) === 1 ? findHostByOrigin(this.original.hosts, k) ?? undefined : undefined;
      await patchRemoteConfig({
        upsert: saving.map((c) => ({
          key: c.persistedKey,
          value: c.collect(),
          was: wasOf(c.persistedKey),
        })),
        remove: [...countBy(this.loadedKeys)]
          .filter(([k, n]) => n > (liveCount.get(k) ?? 0))
          .map(([k]) => k),
      });
      // 落盘成功后卡片身份跟到新 origin 上（用户这次可能就是在改名）。
      const renames: [string, string][] = [];
      for (const c of saving) {
        const next = hostKey(c.collect());
        if (c.persistedKey && c.persistedKey !== next) renames.push([c.persistedKey, next]);
        c.persistedKey = next;
        c.renderStatusStrip();
      }
      this.loadedKeys = saving.map((c) => c.persistedKey!);

      const changed = !sameRemote(next, this.original);
      // 续跑命令起会话时现读，改了不用重启；但要说一声存上了。
      const resumeChanged = next.hosts.some((h, i) => h.resumeCommand !== this.original.hosts[i]?.resumeCommand);
      this.original = next;
      // 那台的默认账号 / 默认模型跟着改名走。
      for (const [from, to] of renames) await moveMachinePrefs(from, to);
      if ((changed || resumeChanged) && incompleteCount === 0 && !fingerprintLooksOff) {
        this.showBanner(copyText("remote.save.done"));
      }
      // 机器表改了 ⇒ 当场对齐那几条流（加的去连、删的 · 停用的断开、改了连接参数的重连），不要重启。
      if (changed) void this.reconcile();
      for (const c of saving) this.repaintRow(c);
      return true;
    } catch (e) {
      console.warn("save remote config failed:", e);
      this.showBanner(copyText("remote.save.failed", { e: String(e) }));
      return false;
    }
  }

  /** 照盘上的机器表对齐远端那几条流；这一趟断开的那几台点子跟着换。 */
  private async reconcile(): Promise<void> {
    try {
      await commands.remote_reconcile();
    } catch (e) {
      this.showBanner(copyText("machineList.reconcile.failed", { e: String(e) }));
    }
    this.pages?.machinesReconciled?.();
  }

  /** 照这台现在的「连接这台」重画那一行（停用 ⇒ 空心点 ＋「已停用」＋［连接］；开着 ⇒ 照上次问到的连接）。 */
  private repaintRow(card: MachineCard): void {
    const pageId = this.pageIdFor(card);
    if (!pageId) return;
    this.setConnected(pageId, this.connected.get(pageId) ?? null);
  }

  /** 这一页是不是停用了连接的那台。 */
  isDisabledPage(pageId: string): boolean {
    const card = [...this.pageIdOf.entries()].find(([, id]) => id === pageId)?.[0];
    return card ? !card.collect().connect : false;
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

// === config.json 读写（多机）===

/** 把一个任意 JSON 对象规整成 RemoteHostConfig（缺失/类型不对走默认）。 */
// F12：`coerceAddresses` / `coerceHost` / `readRemoteConfig` / `findHostByOrigin` /
// `resolveRemoteConfigByOrigin` / 写入口已移入 `src/frontend/ui/remote-config.ts`（数据层）。
// S1：写入口 = `patchRemoteConfig`（局部合并）；整表覆盖的 `writeRemoteConfig` 已收回该文件内部、不再导出（今天连函数都没了；增删改全按键认元素）。 〔散文墓碑〕
// `sameHost` / `sameRemote`（下方）是 UI dirty-check，留本文件。

function sameHost(a: RemoteHostConfig, b: RemoteHostConfig): boolean {
  return (
    a.label === b.label &&
    a.host === b.host &&
    a.port === b.port &&
    a.user === b.user &&
    a.keyPath === b.keyPath &&
    a.hostKeyFingerprint === b.hostKeyFingerprint &&
    a.jump === b.jump &&
    a.connect === b.connect &&
    // F45（Phase G 补）:仅改「备用地址」也算变更。此前独漏 addresses（jump 比了）
    // → 只改多地址、其它不动时「需重启生效」横幅被静默抑制，用户可能不重启、新地址不生效。
    a.addresses.length === b.addresses.length &&
    a.addresses.every((x, i) => x === b.addresses[i])
  );
}

function sameRemote(a: RemoteConfig, b: RemoteConfig): boolean {
  return (
    a.hosts.length === b.hosts.length &&
    a.hosts.every((h, i) => sameHost(h, b.hosts[i]))
  );
}

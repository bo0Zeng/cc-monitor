/**
 * 设置窗：各页、机器的各子页、各种状态。
 */
import { copyText } from "../../../src/frontend/ui/copy-table";
import type { Scene } from "./index";
import type { World } from "../fake/types";
import { defaultWorld } from "../fake/world";
import { PROFILE_ACCOUNTS } from "../fake/profiles";
import { emit } from "@tauri-apps/api/event";
import { byText, click, sleep, waitFor } from "./helpers";

function settings(id: string, title: string, desc: string, act: Scene["act"], world: () => World = defaultWorld, height = 740, width = 960): Scene {
  return { id, page: "settings", dir: "设置", title, desc, width, height, world, act };
}

/** 点导航里的一页（id 照路由器的 `settings-tab-<id>`），等它画好。 */
async function go(...ids: string[]): Promise<void> {
  await waitFor(".settings-nav");
  await sleep(600);
  for (const id of ids) {
    await click(`#${CSS.escape(`settings-tab-${id}`)}`);
    await sleep(700);
  }
  await sleep(600);
}

const page = (id: string, title: string, desc: string, ...route: string[]): Scene =>
  settings(id, title, desc, async () => go(...route));

/** 账号页：那台的额度账有数（work 被拒 · personal 63% / 41% · api 按量）。`verifyFail` ⇒ 打开时核出一处对不上。 */
function acctWorld(verifyFail = false): () => World {
  return () => {
    const w = defaultWorld();
    const now = Math.floor(Date.now() / 1000);
    const slots = (p5: number, p7: number) => [
      { slot: "5h", pct: p5, resetsAt: now + 5400, ...(p5 >= 100 ? { full: true } : {}) },
      { slot: "7d", pct: p7, resetsAt: now + 4 * 86400, ...(p7 >= 100 ? { full: true } : {}) },
    ];
    w.ops["quota-read"] = () => ({
      state: "present",
      reason: null,
      path: "/home/user/.cc-monitor/quota.json",
      now,
      accounts: [
        { agent: "claude-code", account: "work", seenAt: now - 120, kind: "sub", state: "refused", stale: false, limiting: "5h", slots: slots(100, 78), login: "ok" },
        { agent: "claude-code", account: "personal", seenAt: now - 120, kind: "sub", state: "refused", stale: false, limiting: "5h", slots: slots(58, 41), login: "ok" },
        { agent: "claude-code", account: "api", seenAt: now - 120, kind: "api", state: "ok", stale: false, slots: [], login: "ok" },
      ],
      unseen: [],
      usableNow: ["personal", "api"],
      earliestReturn: null,
    });
    if (verifyFail) {
      w.ops["accounts-verify"] = () => ({ pass: false, fails: 1, warns: 0, checks: [{ level: "fail", account: "personal", text: "登录信息缺失" }] });
    }
    return w;
  };
}

/** 配置文件那一页换一种样子（`fake/profiles.ts`）。 */
const profilesWorld = (state: NonNullable<World["profiles"]>) => (): World => ({ ...defaultWorld(), profiles: state });

/** 别名那一行露出来、清单读回之后（树或卡片画好）。 */
async function profilesPage(): Promise<void> {
  await go("machine:devbox", "machine:devbox#config");
  await waitFor(".settings-page:not([hidden]) [data-role=profiles] .prof-tree, .settings-page:not([hidden]) [data-role=file-problem], .settings-page:not([hidden]) [data-role=seed]");
  await sleep(500);
}

const pf = (sel: string): string => `.settings-page:not([hidden]) [data-role=profiles] ${sel}`;

/** 把那一块滚到视口顶上。 */
function top(sel: string): void {
  document.querySelector(sel)?.scrollIntoView({ block: "start" });
}

/** 改表单里一个下拉 / 输入并通知（与人改一样走 change）。 */
function setField(sel: string, value: string): void {
  const e = document.querySelector(sel) as HTMLInputElement | HTMLSelectElement | null;
  if (!e) return;
  e.value = value;
  e.dispatchEvent(new Event("change", { bubbles: true }));
}

function troubleWorld(): World {
  const w = defaultWorld();
  w.unseenMachines = ["gpu-01"];
  w.staleMachines = ["win-laptop"];
  return w;
}

function disabledWorld(): World {
  const w = defaultWorld();
  const hosts = (w.config.remote as { hosts: { label: string; connect: boolean }[] }).hosts;
  for (const h of hosts) if (h.label === "gpu-01") h.connect = false;
  w.unseenMachines = ["gpu-01"];
  return w;
}

function freshWorld(): World {
  const w = defaultWorld();
  w.machines = ["<local>"];
  w.sessions = w.sessions.filter((s) => s.origin === "<local>");
  w.config = {};
  return w;
}

function oddConfigWorld(): World {
  const w = defaultWorld();
  w.config = { ...w.config, backendPolicy: { exit: "keep" }, legacyTheme: "dark" };
  return w;
}

function hostKeyWorld(): World {
  const w = defaultWorld();
  w.hostKeyChanged = ["gpu-01"];
  return w;
}

function installingWorld(): World {
  const w = defaultWorld();
  w.installingMachines = ["gpu-01"];
  return w;
}

/** 一帧状态成品（推送那一路的样子；形状同金样）。 */
const pushed = (state: string, extra: Record<string, unknown> = {}): Record<string, unknown> => ({
  state,
  reason: null,
  stage: null,
  version: null,
  versionRelation: null,
  os: "Linux",
  fixes: [],
  seenHostKey: null,
  ...extra,
});

export const SETTINGS_SCENES: Scene[] = [
  settings("settings-ssh-import-taken", "设置 · 添加机器 · 撞名", "从 ~/.ssh/config 选：build-02 改名成 gpu-01（列表里已有），就地拦", async () => {
    await go("machines");
    await click(await byText("button", "添加机器"));
    await sleep(1200);
    const box = [...document.querySelectorAll<HTMLInputElement>(".add-machine-row input[type=text]")].find((i) => i.value === "build-02")!;
    box.value = "gpu-01";
    box.dispatchEvent(new Event("input", { bubbles: true }));
    await sleep(800);
  }),
  settings("settings-stop-interrupts", "设置 · 停止前问会打断什么", "devbox「这台上的 cc-monitor」点［停止…］：中断几项 · 保留会话照跑", async () => {
    await go("machine:devbox");
    await click(await byText(".settings-page:not([hidden]) button", copyText("machinePage.cc.title")));
    await sleep(500);
    await click(await byText(".settings-page:not([hidden]) button", "停止…"));
    await sleep(800);
  }),
  settings("settings-update-interrupts", "设置 · 更新前问会打断什么", "devbox 要更新时点［更新］：更新要停几秒，确认键「更新」", async () => {
    await go("machines");
    await click(await byText(".machine-problem button", "更新"));
    await sleep(800);
  }, () => {
    const w = defaultWorld();
    w.staleMachines = ["devbox"];
    return w;
  }),
  settings("settings-machines-hostkey", "设置 · 机器 · 主机指纹变了", "gpu-01 出示的指纹与记下的不一样：问题行［比对指纹…］", async () => {
    await go("machines");
  }, hostKeyWorld),
  settings("settings-hostkey-compare", "设置 · 比对主机指纹", "点［比对指纹…］：记下的 · 现在的 · 记于何时，默认焦点「不连接」", async () => {
    await go("machines");
    await click(await byText(".machine-problem button", "比对指纹…"));
    await sleep(800);
  }, hostKeyWorld),
  settings("settings-machines-installing", "设置 · 机器 · 正在装", "gpu-01 正在装 cc-monitor：虚线点 ＋「安装中」＋ 进度条", async () => {
    await go("machines");
  }, installingWorld),
  settings("settings-machines-pushed", "设置 · 机器 · 状态推送", "页面开着时壳推来两帧：devbox 正在连（启动 cc-monitor）、gpu-01 正在更新 —— 那两行原位换", async () => {
    await go("machines");
    await emit("machine-state", { origin: "devbox", machine: pushed("connecting", { stage: "attach" }) });
    await emit("machine-state", { origin: "gpu-01", machine: pushed("updating") });
    await sleep(800);
  }),

  settings("settings-landing", "设置 · 打开时", "设置窗打开时落在的那一页", async () => {
    await waitFor(".settings-nav");
    await sleep(1500);
  }),
  page("settings-general", "设置 · 通用", "通用：行为 · 恢复命令 · 高级", "general"),
  page("settings-appearance", "设置 · 外观", "外观：字 · 颜色 · 快捷键", "appearance"),
  page("settings-logs", "设置 · 日志", "日志：开关、级别、日志文件", "logs"),
  page("settings-data", "设置 · 文件与数据 · 要你动手", "文件与数据第一栏：顶上各类数 · 每台一段（急的在前）· 每件 点 · 名字 · 类 · 位置 · 现状 · 主按钮 · 已做的折起 · 连不上的那台离线未检查", "data"),
  settings("settings-data-chore-open", "设置 · 要你动手 · 实时显示点开", "点开一件：为什么 · 怎么做 · diff（钥匙遮住）· 复制这几行 / 复制改好的整份文件（含 2 件）· 不用了 · 存盘后自己认出", async () => {
    await go("data");
    await sleep(500);
    const row = document.querySelector<HTMLElement>('.settings-page:not([hidden]) [data-machine="devbox"] [data-chore="relay"]');
    row?.querySelector<HTMLButtonElement>("[aria-expanded]")?.click();
    await sleep(300);
    document.querySelector<HTMLElement>('.settings-page:not([hidden]) [data-machine="devbox"] [data-chore="relay"]')?.scrollIntoView({ block: "start" });
    await sleep(400);
  }),
  settings("settings-data-placed", "设置 · 文件与数据 · 放了什么", "第二栏：机器 chip · Claude 目录 · 改过你的文件（撤回在哪 ［前往］）· cc-monitor 的文件", async () => {
    await go("data");
    await click(await byText(".settings-page:not([hidden]) button", "cc-monitor 放置的文件"));
    await sleep(700);
  }),
  settings("settings-data-placed-remote", "设置 · 文件与数据 · 放了什么 · devbox", "第二栏选 devbox：只列那台改过你的文件", async () => {
    await go("data");
    await click(await byText(".settings-page:not([hidden]) button", "cc-monitor 放置的文件"));
    await sleep(400);
    await click(await byText(".settings-page:not([hidden]) .data-chip", "devbox"));
    await sleep(700);
  }),
  settings(
    "settings-data-placed-offline",
    "设置 · 文件与数据 · 放了什么 · 连不上的那台",
    "gpu-01 连不上：照本机后端记着的上次那一份画（改过你的文件 · cc-monitor 的文件）＋ 警告条说多旧 ［重试］",
    async () => {
      await go("data");
      await click(await byText(".settings-page:not([hidden]) button", "cc-monitor 放置的文件"));
      await sleep(400);
      await click(await byText(".settings-page:not([hidden]) .data-chip", "gpu-01"));
      await sleep(700);
    },
    troubleWorld,
  ),
  settings("settings-ext-install", "设置 · 扩展 · 一次装到两台", "github 那一行点开：安装位置每台一行（勾选 · 现状）· 勾 devbox 与 win-laptop ⇒ 一张卡：装到哪 · token 只填一次（win-laptop 已有）· 将写入的文件逐台 ·［装到 2 台］", async () => {
    await go("ext");
    await sleep(600);
    document.querySelector<HTMLElement>('.settings-page:not([hidden]) .ext-row[data-key="mcp/github"]')?.click();
    await sleep(300);
    for (const m of ["devbox", "win-laptop"]) {
      document.querySelector<HTMLInputElement>(`.settings-page:not([hidden]) .ext-pick[data-machine="${m}"]`)?.click();
      await sleep(300);
    }
    await sleep(500);
  }, troubleWorld),
  settings("settings-general-resume-open", "设置 · 通用 · 恢复命令下拉", "恢复命令点开：默认那一家的启动器（灰字默认）· 用过的 · 自定义…", async () => {
    await go("general");
    await sleep(500);
    document.querySelector<HTMLButtonElement>('.settings-page:not([hidden]) [data-role="resume-select"]')?.click();
    await sleep(400);
  }),
  settings("settings-general-terminal-open", "设置 · 通用 · 终端下拉", "Linux 上「终端」那一行点开：自动（灰字：会挑谁）· 探到的各个 · 自定义…", async () => {
    await go("general");
    await sleep(500);
    document.querySelector<HTMLButtonElement>('.settings-page:not([hidden]) [data-role="terminal-select"]')?.click();
    await sleep(400);
  }),
  settings(
    "settings-general-terminal-none",
    "设置 · 通用 · 终端 · 一个都没探到",
    "这台一个终端都没探到：自动那一项灰字「未找到」",
    async () => {
      await go("general");
      await sleep(600);
    },
    () => {
      const w = defaultWorld();
      w.commands = { ...w.commands, terminal_choices: () => ({ applies: true, auto: null, found: [], setting: "" }) };
      return w;
    },
  ),
  page("settings-machines", "设置 · 机器", "机器列表：本机 ＋ 三台远端，每台的连接 / 后端 / ccm / 账号四格", "machines"),
  page("settings-machine-local", "设置 · 本机", "本机那一页", "machine:（本机）"),
  page("settings-machine-remote", "设置 · 远端机器", "devbox 那一页（落在第一个子页）", "machine:devbox"),
  settings("settings-machine-conn", "设置 · 远端 · 连接设置", "devbox 卡头里展开「连接设置」", async () => {
    await go("machine:devbox");
    await click(await byText(".settings-page:not([hidden]) button", "连接设置"));
    await sleep(500);
  }),
  settings("settings-machine-cc", "设置 · 远端 · 这台上的 cc-monitor", "devbox 卡头里展开「这台上的 cc-monitor」", async () => {
    await go("machine:devbox");
    await click(await byText(".settings-page:not([hidden]) button", copyText("machinePage.cc.title")));
    await sleep(800);
  }),
  settings("settings-machine-local-cc", "设置 · 本机 · 这台上的 cc-monitor", "本机卡头里展开「这台上的 cc-monitor」：状态 · 随退出停止 · 恢复命令（仅本机 · 留空 = 通用设置）", async () => {
    await go("machine:（本机）");
    await click(await byText(".settings-page:not([hidden]) button", copyText("machinePage.cc.title")));
    await sleep(800);
  }),
  settings("settings-logs-restart", "设置 · 日志 · 改了要重启", "日志页拨「日志写入文件」：行内「重启 cc-monitor 后生效」＋ 顶上那条", async () => {
    await go("logs");
    const sw = [...document.querySelectorAll<HTMLLabelElement>(".settings-page:not([hidden]) label")].find((l) => l.textContent?.startsWith("日志写入文件"));
    sw?.querySelector<HTMLButtonElement>("[role=switch]")?.click();
    await sleep(800);
  }),
  settings("settings-machine-acct", "设置 · 远端 · 账号", "devbox 的「账号」栏：表头 · 一号一行（默认 · 5h · 7d · 按量）· 表下指路框", async () => go("machine:devbox", "machine:devbox#acct"), acctWorld()),
  settings("settings-acct-detail", "设置 · 账号 · 一行展开", "点 api 那一行：命令 · API key · 默认模型（仅 devbox · api）· 账号目录 · 删除", async () => {
    await go("machine:devbox", "machine:devbox#acct");
    await click('[data-account="api"] .acct-row');
    await sleep(600);
  }, acctWorld()),
  settings("settings-acct-new", "设置 · 账号 · 新建（API key）", "［新建账号］就地展开：名字 team · 选 API key", async () => {
    await go("machine:devbox", "machine:devbox#acct");
    await click(await byText(".settings-page:not([hidden]) button", "新建账号"));
    await sleep(300);
    const name = document.querySelector<HTMLInputElement>(".acct-new input")!;
    name.value = "team";
    name.dispatchEvent(new Event("input"));
    await click(await byText(".acct-new label", "API key"));
    await sleep(600);
  }, acctWorld()),
  settings("settings-acct-remove", "设置 · 账号 · 删默认号", "work ⋯ → 删除 work…：删 / 留两行 · 之后新会话默认 personal · 焦点在取消", async () => {
    await go("machine:devbox", "machine:devbox#acct");
    await click('[data-account="work"] .acct-row');
    await sleep(400);
    await click(await byText('[data-account="work"] button', "删除 work…"));
    await sleep(600);
  }, acctWorld()),
  settings("settings-acct-mcp-go", "设置 · 账号 · 共用 MCP 指过去", "账号表下「共用 MCP：别名与配置文件」点了：切到同一台的「别名与配置文件」栏", async () => {
    await go("machine:devbox", "machine:devbox#acct");
    await click(".settings-page:not([hidden]) .acct-mcp-line button");
    await sleep(600);
  }, acctWorld()),
  settings("settings-appearance-kb-anchor", "设置 · 外观 · 快捷键那一节（带锚点打开）", "主窗口快捷键一览的「改快捷键…」开 {page: appearance, anchor: keybindings}：落到快捷键那一节并高亮", async () => {
    await waitFor(".settings-nav");
    await sleep(1500);
    await emit("settings-target", JSON.stringify({ page: "appearance", anchor: "keybindings" }));
    await sleep(500);
  }),
  settings("settings-acct-verify", "设置 · 账号 · 打开时核出对不上", "personal 的登录信息缺失：表上方一条警告条 ＋［修复…］", async () => go("machine:devbox", "machine:devbox#acct"), acctWorld(true)),
  page("settings-machine-config", "设置 · 远端 · 别名与配置文件", "devbox 的「别名与配置文件」栏", "machine:devbox", "machine:devbox#config"),
  settings("settings-machine-menu", "设置 · 机器 ⋯ 菜单", "机器列表里 devbox 那一行的 ⋯", async () => {
    await go("machines");
    const row = document.querySelector<HTMLElement>('.remote-machine-row[data-page-id="machine:devbox"]')!;
    await click(row.querySelector<HTMLButtonElement>('button[aria-label]')!);
    await sleep(500);
  }),
  settings("settings-machine-removed", "设置 · 删机器 ＋ 8 秒撤销", "⋯ →「从列表删除」：那一行当场没了，底下一条撤销", async () => {
    await go("machines");
    const row = document.querySelector<HTMLElement>('.remote-machine-row[data-page-id="machine:gpu-01"]')!;
    await click(row.querySelector<HTMLButtonElement>('button[aria-label]')!);
    await sleep(300);
    await click(await byText("[role=menuitem], button", "从列表删除"));
    await sleep(600);
  }),
  page("settings-machine-win", "设置 · Windows 机器", "win-laptop 那一页", "machine:win-laptop"),
  page("settings-ext", "设置 · 扩展", "扩展：skill 与插件", "ext"),

  settings("settings-keybindings", "设置 · 快捷键编辑器", "外观页「快捷键」那一节点开编辑器", async () => {
    await go("appearance");
    await click(await byText('.settings-page:not([hidden]) [data-anchor="keybindings"] button', copyText("settingsPanel.keybindings.open")));
    await sleep(900);
  }),
  settings("settings-general-advanced", "设置 · 通用 · 高级", "通用页展开「高级：上下文上限」", async () => {
    await go("general");
    const head = document.querySelector<HTMLElement>('.settings-page:not([hidden]) [data-anchor="context-limits"] [aria-expanded]');
    if (head) {
      head.click();
      head.scrollIntoView({ block: "start" });
    }
    await sleep(700);
  }),
  settings("settings-machines-add", "设置 · 添加机器 · 自己填", "机器页点「添加机器」→「自己填」", async () => {
    await go("machines");
    await click(await byText("button", "添加机器"));
    await sleep(600);
    await click(await byText("[role=dialog] button", "自己填"));
    await sleep(400);
  }),
  settings("settings-port-forward", "设置 · 端口转发", "机器页点「端口转发…」：两条在跑的转发", async () => {
    await go("machines");
    await click(document.querySelector<HTMLButtonElement>('.settings-page:not([hidden]) > .settings-page-head button[aria-label]')!);
    await sleep(300);
    await click(await byText("[role=menuitem], button", "端口转发"));
    await sleep(1200);
  }),
  settings("settings-ssh-import", "设置 · 添加机器 · 从 ~/.ssh/config 选", "机器页点「添加机器」：已在列表的灰着", async () => {
    await go("machines");
    await click(await byText("button", "添加机器"));
    await sleep(1200);
  }),
  settings("settings-machines-disabled", "设置 · 机器 · 停用了连接的那台", "gpu-01 关了「连接这台」：空心点 ＋「已停用」＋［连接］", async () => {
    await go("machines");
  }, disabledWorld),
  settings("settings-machine-disabled-conn", "设置 · 停用的那台 · 连接设置", "gpu-01 卡头展开「连接设置」：「连接这台」关着", async () => {
    await go("machine:gpu-01");
    await click(await byText(".settings-page:not([hidden]) button", "连接设置"));
    await sleep(400);
    document.querySelector(".settings-page:not([hidden]) .machine-conn-switch")?.scrollIntoView({ block: "center" });
    await sleep(300);
  }, disabledWorld),
  settings("settings-machines-trouble", "设置 · 机器 · 一台连不上", "gpu-01 连不上：机器列表里那一行的样子", async () => {
    await go("machines");
  }, troubleWorld),
  settings("settings-machine-trouble", "设置 · 连不上的那台", "gpu-01 那一页（连不上）", async () => {
    await go("machine:gpu-01");
  }, troubleWorld),
  settings("profiles-01-tree", "设置 · 别名 · 01 清单按「基于」排成树", "devbox「别名与配置文件」：清单按基于排成树、每行只写自己那几项；cc · cct 标终端函数", async () => {
    await profilesPage();
    top(".settings-page:not([hidden]) .machine-aliases");
    await sleep(300);
  }, defaultWorld, 860),
  settings("profiles-02-merge", "设置 · 别名 · 02 点一行看合并表（宽窗）", "点 teamcct：右栏合并表（项 · 值 · 来自哪一段）· 假设在这个目录敲 · 等于", async () => {
    await profilesPage();
    await click(pf('.prof-trow[data-name="teamcct"]'));
    await sleep(700);
    top(pf("[data-role=tree]"));
    await sleep(300);
  }, defaultWorld, 860, 1180),
  settings("profiles-02-merge-narrow", "设置 · 别名 · 02 点一行看合并表（窄窗）", "窄窗：合并表落到那一行下面", async () => {
    await profilesPage();
    await click(pf('.prof-trow[data-name="teamcct"]'));
    await sleep(700);
    top(pf('.prof-trow[data-name="cct"]'));
    await sleep(300);
  }, defaultWorld, 860, 860),
  settings("profiles-03-new", "设置 · 别名 · 03 新建", "＋ 新增别名：名字 bcct2 · 基于 cct · 账号 team；在哪起 / 按目录灰着写继承值；等于问后端", async () => {
    await profilesPage();
    await click(await byText(pf(".prof-head button"), "新增别名"));
    await sleep(500);
    setField(pf("[data-role=profile-form] [data-role=name]"), "bcct2");
    await sleep(400);
    await click(pf("[data-role=profile-form] [data-role=from]"));
    await sleep(300);
    await click(await byText("[role^=menuitem]", /^cct/));
    await sleep(500);
    await click(await byText(pf("[data-role=profile-form] .prof-segb"), "team"));
    await sleep(700);
    top(pf("[data-role=profile-form]"));
    await sleep(300);
  }, defaultWorld, 900),
  settings("profiles-04-impact", "设置 · 别名 · 04 改父那一条先看连带谁", "改 cct 的在哪起：表单上方列出会跟着变的子、树里那几行淡黄、存旁写会连带 3 条", async () => {
    await profilesPage();
    await click(pf('.prof-trow[data-name="cct"] .cfg-link'));
    await sleep(600);
    setField(pf('[data-role=profile-form] [data-slot=tmux] select'), "fixed");
    await sleep(500);
    setField(pf('[data-role=profile-form] [data-slot=tmux] input'), "work");
    await sleep(700);
    await click(await byText(pf("[data-role=impact-note] .cfg-link"), "逐条对比"));
    await sleep(400);
    top(pf(".prof-notes"));
    await sleep(300);
  }, defaultWorld, 1000),
  settings("profiles-05-remove", "设置 · 别名 · 05 删一条被别人基于的", "删 cct：三个选择，推荐改成基于 cc；看改前改后", async () => {
    await profilesPage();
    await click(pf('.prof-trow[data-name="cct"] .cfg-link-danger'));
    await sleep(700);
    top(pf(".prof-notes"));
    await sleep(300);
  }, defaultWorld, 900),
  settings("profiles-06-edited", "设置 · 别名 · 06 配置文件被手改过", "后端说上次 cc-monitor 写过之后有人改过：清单头一行小字 ＋ 看配置文件", async () => {
    await profilesPage();
    top(pf(".prof-notes"));
    await sleep(300);
  }, profilesWorld("edited"), 860),
  settings("profiles-07-broken", "设置 · 别名 · 07 手改写错一段", "teamcct 写错：行上标现在不能用、摘要换成后端原话带行号；别的照常", async () => {
    await profilesPage();
    top(pf("[data-role=tree]"));
    await sleep(300);
  }, profilesWorld("broken"), 760),
  settings("profiles-07-syntax", "设置 · 别名 · 07 TOML 写坏", "整份不能用：清单换成一张卡（行号 · 原话 · 打开配置文件）", async () => {
    await profilesPage();
    top(".settings-page:not([hidden]) .machine-aliases");
    await sleep(300);
  }, profilesWorld("syntax"), 760),
  settings("profiles-08-stale", "设置 · 别名 · 08 保存时被别处改过", "改 teamcct 的账号存：配置文件已不是打开时那一份 ⇒ 一个字节不写、填的还在、重新读", async () => {
    await profilesPage();
    await click(pf('.prof-trow[data-name="teamcct"] .cfg-link'));
    await sleep(600);
    // 换成库里最后一个号（teamcct 原来是 team，换哪个都行，只要不是它）。
    await click(await byText(pf("[data-role=profile-form] .prof-segb"), PROFILE_ACCOUNTS[PROFILE_ACCOUNTS.length - 1]));
    await sleep(500);
    await click(pf("[data-role=profile-form] [data-role=save]"));
    await sleep(700);
    top(pf("[data-role=profile-form]"));
    await sleep(300);
  }, profilesWorld("stale"), 860),
  settings("profiles-09-function", "设置 · 别名 · 09 与系统命令同名的那一条", "点 cc：合并表下多一段「cc · 终端函数」：和 /usr/bin/cc 同名 · 写在哪", async () => {
    await profilesPage();
    await click(pf('.prof-trow[data-name="cc"]'));
    await sleep(700);
    top(pf("[data-role=tree]"));
    await sleep(300);
  }, defaultWorld, 860, 1180),
  settings("profiles-10-migrated", "设置 · 别名 · 10 第一次升级后", "页首绿卡：11 条已转进配置文件（知道了）；.bashrc 里同名函数三个选择", async () => {
    await profilesPage();
    top(pf(".prof-notes"));
    await sleep(300);
  }, profilesWorld("migrated"), 1000),
  settings("profiles-11-empty", "设置 · 别名 · 11 空态", "没有配置文件：首建两条的预览 · 建这两条 · 从空的开始", async () => {
    await profilesPage();
    top(".settings-page:not([hidden]) .machine-aliases");
    await sleep(300);
  }, profilesWorld("empty"), 700),
  settings("settings-machine-config-mcp", "设置 · 远端 · 共用 MCP 展开", "devbox「共用 MCP」那一行点开：两边都改的选一版 · 共用的两条 · 停止同步", async () => {
    await go("machine:devbox", "machine:devbox#config");
    await sleep(800);
    await click(".settings-page:not([hidden]) [data-role=mcp-row] .cfg-toggle");
    await sleep(400);
    document.querySelector(".settings-page:not([hidden]) [data-role=mcp-row]")?.scrollIntoView({ block: "start" });
    await sleep(300);
  }),
  settings("settings-machine-config-anchor", "设置 · 带锚点打开 · 接上终端", "主窗口 ↗［接上终端］开 {machine: devbox, tab: config, anchor: connect-terminal}", async () => {
    await waitFor(".settings-nav");
    await sleep(1500);
    await emit("settings-target", JSON.stringify({ machine: "devbox", tab: "config", anchor: "connect-terminal" }));
    await sleep(600);
  }),
  settings("settings-machine-name-taken", "设置 · 远端 · 改名撞名", "devbox 的名字改成 gpu-01（另一台已经叫这个）", async () => {
    await go("machine:devbox");
    await click(await byText(".settings-page:not([hidden]) button", "连接设置"));
    const name = document.querySelector<HTMLInputElement>('.settings-page:not([hidden]) input[type="text"]')!;
    name.value = "gpu-01";
    name.dispatchEvent(new Event("change"));
    await sleep(700);
  }),
  settings("settings-machine-port-range", "设置 · 远端 · 端口越界", "devbox 的端口填 70000", async () => {
    await go("machine:devbox");
    await click(await byText(".settings-page:not([hidden]) button", "连接设置"));
    const port = document.querySelector<HTMLInputElement>('.settings-page:not([hidden]) input[type="number"]')!;
    port.value = "70000";
    port.dispatchEvent(new Event("change"));
    await sleep(700);
  }),
  settings("settings-appearance-font-size-range", "设置 · 外观 · 字号越界", "基础字号填 1", async () => {
    await go("appearance");
    const size = document.querySelector<HTMLInputElement>('.settings-page:not([hidden]) input[type="number"]')!;
    size.value = "1";
    size.dispatchEvent(new Event("input"));
    size.dispatchEvent(new Event("change"));
    await sleep(700);
  }),
  settings("settings-unknown-keys", "设置 · 配置里有认不出的键", "config.json 里留着两个已经没人读的顶层键：设置窗顶上的提示条", async () => {
    await waitFor(".settings-nav");
    await sleep(1500);
  }, oddConfigWorld),
  settings("settings-narrow", "设置 · 窄窗", "窄于 640：导航收成内容区顶上的下拉", async () => {
    await go("general");
  }, defaultWorld, 600, 620),
  settings("settings-target", "设置 · 带目的地打开", "从别处带着目的地打开：devbox 的账号栏，高亮那一栏", async () => {
    await waitFor(".settings-nav");
    await sleep(1500);
    await emit("settings-target", JSON.stringify({ machine: "devbox", tab: "acct" }));
    await sleep(600);
  }),
  {
    ...settings("settings-fresh", "设置 · 刚装好", "只有本机、没配过任何东西、各格都没测过：机器页", async () => {
      await go("machines");
    }, freshWorld),
    storage: {},
  },
];

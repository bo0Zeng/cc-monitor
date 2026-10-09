/**
 * 主窗口上的面板与浮层：子 agent / 任务 / 账号 / 命令面板 / 会话内查找 / 大纲 / 监控板 / 历史 / 右键菜单 / 提示。
 */
import { copyText } from "../../../src/frontend/ui/copy-table";
import { copyPattern } from "../../test-support/copy-pattern";
import { hms } from "../fake/clock";
import type { Scene } from "./index";
import { Refuse, type World } from "../fake/types";
import { defaultWorld } from "../fake/world";
import { byText, click, key, mainReady, openTab, rightClick, sleep, type, waitFor } from "./helpers";

const ALL_TABS = 7;

function panel(id: string, title: string, desc: string, act: Scene["act"], world: () => World = defaultWorld): Scene {
  return { id, page: "index", dir: "主窗口-面板", title, desc, width: 1280, height: 800, world, act };
}

/** 这几张要那台的规则表（多选右键「轮换规则 ▸」· 起新会话框「轮换」）：日常（默认）· 夜间 · 省额度。 */
function rotRulesWorld(): World {
  const w = defaultWorld();
  const rot = { order: [{ start: true }, "personal", "work"], enabled: ["personal", "work"], when: "full", atLimit: "continue", wait: 40 };
  const rule = (id: string, name: string, isDefault: boolean) => ({
    id,
    name,
    isDefault,
    rotation: rot,
    rev: 1,
    updatedAt: 0,
    summary: "personal → work · 满",
    explain: "起始账号先用 · 被拒才换",
    missing: [],
    atLimitApplies: false,
    users: { live: 0, ended: 0, follow: 0, doing: {}, sids: [], endedSids: [] },
  });
  w.ops["rotation-rules-read"] = () => ({
    state: "present",
    reason: null,
    defaultRule: "r_daily",
    rules: [rule("r_daily", "日常", true), rule("r_night", "夜间", false), rule("r_save", "省额度", false)],
  });
  return w;
}

async function scrollStreamTop(): Promise<void> {
  const box = document.querySelector<HTMLElement>("#message-stream");
  for (const e of box ? [box, ...box.querySelectorAll<HTMLElement>("*")] : []) if (e.scrollHeight > e.clientHeight + 50) e.scrollTop = 0;
  await sleep(500);
}

async function openCommandBar(): Promise<void> {
  await mainReady(ALL_TABS);
  await click(".status-cmdk");
  await waitFor("[data-role=command-input]");
}

/** devbox 上的「账单导出」那条已结束：tab 上的账号徽标来自那台记着的「上次用的号」。 */
function remoteEndedWorld(): World {
  const w = defaultWorld();
  w.sessions[3].ended = true;
  return w;
}

/** 同上，且那台说这条会话上次用的号（personal）现在选不了，替代是它的默认号 work。 */
function accountGoneWorld(): World {
  const w = remoteEndedWorld();
  w.ops["launch-render-cli"] = () => {
    throw new Refuse("account_unavailable", "账号 personal 不可用，未启动", {
      requested: "personal",
      pinned: true,
      listKnown: true,
      alternative: "work",
    });
  };
  return w;
}

/** 分叉沿用的那个号选不了：那台回 `account_unavailable`、带替代号（不起、不悄悄换号）。 */
function forkUnavailableWorld(): World {
  const w = defaultWorld();
  w.ops["session-new"] = () => {
    throw new Refuse("account_unavailable", "personal 选不了", {
      field: "account",
      unavailable: { requested: "personal", pinned: true, listKnown: true, alternative: "work" },
    });
  };
  return w;
}

/** 起新会话框：目录那一格那台说不在。 */
function noDirWorld(): World {
  const w = defaultWorld();
  w.ops["session-new"] = () => {
    throw new Refuse("no_dir", "目录不存在", { field: "cwd", unavailable: null });
  };
  return w;
}

/** 点了［新建］、那台起好了（tmux 里）但会话一直没报到：那个 tmux 会话还在，画面末几行是启动器的报错。 */
function unarrivedWorld(): World {
  const w = defaultWorld();
  const list = w.ops["terminals-list"];
  w.ops["terminals-list"] = (o, r, ww) => {
    const got = list(o, r, ww) as { terminals: Record<string, unknown>[] };
    return { ...got, terminals: [...got.terminals, { ...got.terminals[0], terminal: "tmux-orders-cc-2", tmux_name: "orders-cc-2", title: "orders-cc-2", session: null, clients: [] }] };
  };
  w.ops["terminal-preview"] = () => ({
    lines: ["$ ccm -- claude --modle opus", "claude: error: unknown option '--modle'", "(Did you mean --model?)", "$ "].map((text) => ({ text })),
    screen: "00000000000000b2",
    captured_at: Math.floor(Date.parse("2026-10-06T10:42:05") / 1000),
    captured_at_text: hms(Date.parse("2026-10-06T10:42:05")),
  });
  return w;
}

/** 起新会话：命令面板开框、点［新建］（`window` ⇒ 先点「新的终端窗口」）。 */
async function startNewSession(place: "tmux" | "window" = "tmux"): Promise<void> {
  await openCommandBar();
  await type("[data-role=command-input]", "新建会话");
  await key("Enter");
  await waitFor('[role="dialog"] button[aria-label="账号"]:not([data-value=""])');
  await sleep(300);
  if (place === "window") await click('[role="dialog"] input[value="window"]');
  await click(await byText('[role="dialog"] button', "新建"));
}

/** gpu-01 连不上，而它那个会话是固定着的 ⇒ 复活出来「说不清」。 */
function unseenPinnedWorld(): World {
  const w = defaultWorld();
  w.unseenMachines = ["gpu-01"];
  const s = w.sessions[5];
  w.config = {
    ...w.config,
    tabBar: { pinned: [{ sid: s.sid, origin: "gpu-01", cwd: s.cwd, jsonlPath: `/home/user/.claude/projects/x/${s.sid}.jsonl`, title: "排序模型训练脚本" }] },
  };
  return w;
}

/** 「表格分页」那个会话有一个子 agent、没有任务。 */
function agentsOnlyWorld(): World {
  const w = defaultWorld();
  w.sessions[1].runs = [{ run: "agent-b1", label: "量一下现在的渲染耗时", kind: "general-purpose", state: "running", last: { t: "tool", name: "Bash" } }];
  return w;
}

/** 「自动跟随」绑了 J。 */
function behaviorKeyWorld(): World {
  const w = defaultWorld();
  w.config = { ...w.config, keybindings: { "behavior.toggle-auto-follow": "KeyJ" } };
  return w;
}

function closedWorld(): World {
  const w = defaultWorld();
  w.closedMachines = ["devbox"];
  return w;
}

/** 活卡：第一个会话正在出一轮新的应答（思考 → 正文 → 调工具），还没收尾。 */
async function streamLiveReply(ctx: Parameters<Scene["act"]>[0]): Promise<void> {
  const sid = ctx.backend.world.sessions[0].sid;
  let n = 0;
  const ev = (e: unknown): unknown => ({ origin: "<local>", stream: sid, resp: 1, n: n++, ev: e });
  ctx.backend.pushTap("<local>", [
    ev({ t: "start", rid: "req_live0001" }),
    ev({ t: "block", i: 0, kind: "thinking" }),
    ev({ t: "text", i: 0, s: "文档里的配置表要和 settings 里的默认值对得上，先读一下 settings.py 再写。" }),
    ev({ t: "block", i: 1, kind: "text" }),
    ev({ t: "text", i: 1, s: "好，我来更新 `docs/config.md`：把三个新配置项加进表里，并写一段" }),
    ev({ t: "text", i: 1, s: "「什么时候该调大重试次数」的说明。" }),
    ev({ t: "block", i: 2, kind: "tool", tool: "Read" }),
  ]);
  await sleep(900);
}

export const PANEL_SCENES: Scene[] = [
  panel("panel-live-card", "活卡：正在出的一轮应答", "当前会话正在流式出一轮新的应答（思考、正文、调工具），还没写进记录", async (ctx) => {
    await mainReady(ALL_TABS);
    await streamLiveReply(ctx);
  }),
  panel("panel-agent-run-open", "子 agent 面板 · 点开一个", "子 agent 面板里点开第一行（在跑的那一个排在前面）：它的时间线", async () => {
    await mainReady(ALL_TABS);
    await click(".status-agents");
    await waitFor("#bottom-drawer .agent-row");
    await click("#bottom-drawer .agent-row");
    await sleep(1200);
  }),
  panel("panel-new-session", "起新会话", "命令面板「新建会话…」：当前标签页那台 · 那个目录 · 默认账号在前；运行于两张单选；更多里是 tmux 会话名（那台铸的作占位）与启动命令", async () => {
    await openCommandBar();
    await type("[data-role=command-input]", "新建会话");
    await key("Enter");
    await waitFor('[role="dialog"] button[aria-label="账号"]:not([data-value=""])');
    await sleep(600);
  }),
  panel("panel-new-session-rot", "起新会话 · 轮换", "账号下一行「轮换」点开：跟随默认（日常）· 那台的各条规则（默认那条带「默认」）；没有「本会话」", async () => {
    await openCommandBar();
    await type("[data-role=command-input]", "新建会话");
    await key("Enter");
    await waitFor('[role="dialog"] button[aria-label="轮换"]');
    await sleep(400);
    await click('[role="dialog"] button[aria-label="轮换"]');
    await sleep(500);
  }, rotRulesWorld),
  panel("panel-batch-rot", "多选右键 · 轮换规则 ▸", "本机三个标签页多选、右键「轮换规则」：跟随默认（日常）· 各条规则 · 管理规则…", async () => {
    await mainReady(ALL_TABS);
    const tabs = [...document.querySelectorAll<HTMLElement>("#tab-bar .tab")].slice(0, 3);
    for (const t of tabs) {
      const r = t.getBoundingClientRect();
      t.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, ctrlKey: true, clientX: r.left + 10, clientY: r.top + 5 }));
      await sleep(60);
    }
    await rightClick(tabs[0]);
    await sleep(600);
    const item = await byText("[role^=menuitem]", copyText("tabBatch.menu.rot", { n: 3 }));
    item.closest<HTMLElement>("[role=none]")?.dispatchEvent(new MouseEvent("mouseenter"));
    await sleep(500);
  }, rotRulesWorld),
  panel("panel-cmdk-rot", "命令面板 · 轮换", "命令面板里输入「轮换」：套用规则… · 本机 轮换规则", async () => {
    await openCommandBar();
    await type("[data-role=command-input]", "轮换");
    await sleep(300);
  }),
  panel("panel-new-session-account", "起新会话 · 挑账号", "账号那一格点开：头像在前 · 当前项打勾 · 默认 / 5h 用量灰字跟在名字后", async () => {
    await openCommandBar();
    await type("[data-role=command-input]", "新建会话");
    await key("Enter");
    await waitFor('[role="dialog"] button[aria-label="账号"]:not([data-value=""])');
    await click('[role="dialog"] button[aria-label="账号"]');
    await sleep(500);
  }),
  panel("panel-new-session-more", "起新会话 · 更多", "「更多」展开：tmux 会话名（那台铸的作占位）· 启动命令", async () => {
    await openCommandBar();
    await type("[data-role=command-input]", "新建会话");
    await key("Enter");
    await waitFor('[role="dialog"] button[aria-label="账号"]:not([data-value=""])');
    await click(await byText('[role="dialog"] button', copyText("newSession.more.plain")));
    await sleep(500);
  }),
  panel("panel-new-session-nodir", "起新会话 · 目录那一格不行", "点［新建］、那台说目录不在：错误落在目录那一格下，框不关", async () => {
    await openCommandBar();
    await type("[data-role=command-input]", "新建会话");
    await key("Enter");
    await waitFor('[role="dialog"] button[aria-label="账号"]:not([data-value=""])');
    await sleep(300);
    await click(await byText('[role="dialog"] button', "新建"));
    await sleep(600);
  }, noDirWorld),
  panel("panel-new-slot-starting", "起新会话 · 正在启动", "点［新建］、那台回「起好了」：框关掉，标签页栏末尾长出占位标签页「正在启动」（转圈 · 项目名），主区换成它那一页；报到了原位换成真的", async () => {
    await startNewSession();
    await waitFor("#tab-bar [data-slot]");
    await sleep(600);
  }),
  panel("panel-new-slot-missed", "起新会话 · 20s 未报到", "那台起好了、20 秒没看到会话报到：占位标签页变红点「未报到」，那一页是报错卡 —— tmux 会话在不在 ＋ 画面末几行 ＋［终端画面］［在终端里打开］（远端才有）［结束 tmux 会话］·［关闭标签页］；之后报到了照样换成真的", async () => {
    await startNewSession();
    await waitFor("#tab-bar [data-slot][data-state=missed]", 30_000);
    await sleep(600);
  }, unarrivedWorld),
  panel("panel-new-slot-window", "起新会话 · 开窗起的未报到", "运行于「新的终端窗口」、20 秒没报到：只有「已发启动命令 · 20s 未报到」＋［关闭标签页］（原话在那个窗口里，这边读不到）", async () => {
    await startNewSession("window");
    await waitFor("#tab-bar [data-slot][data-state=missed]", 30_000);
    await sleep(600);
  }),
  panel("panel-fork-new", "从这一轮分叉", "已结束的会话里，用户那句话上的分叉按钮：开起新会话框的分叉那一形 —— 顶上说分叉自哪一轮；源会话的号说不出 ⇒ 账号那一格是「跟随原会话上次的号」；不给终端名那一格", async () => {
    await mainReady(ALL_TABS);
    await openTab(2);
    await scrollStreamTop();
    const btn = await waitFor(".stream.active .viewer-branch-btn");
    await click(btn);
    await waitFor('[role="dialog"] button[aria-label="账号"]:not([data-value=""])');
    await sleep(700);
  }),
  panel("panel-fork-unavailable", "分叉 · 原会话的号选不了", "在跑的会话上分叉、它用的号选不了：不起、不悄悄换号 —— 那一格下「原会话账号 · 不自动换」＋［改用 work］［登录…］", async () => {
    await mainReady(ALL_TABS);
    await openTab(0);
    await scrollStreamTop();
    const btn = await waitFor(".stream.active .viewer-branch-btn");
    await click(btn);
    await waitFor('[role="dialog"] button[aria-label="账号"]:not([data-value=""])');
    await sleep(300);
    await click(await byText('[role="dialog"] button', "新建"));
    await sleep(600);
  }, forkUnavailableWorld),
  panel("panel-kill-confirm", "结束会话前的确认", "tab 右键「结束会话…」：标题点名会话，中断 / 保留逐项写，tmux 会话名在正文里，焦点在「取消」", async () => {
    await mainReady(ALL_TABS);
    await rightClick("#tab-bar .tab");
    await sleep(800);
    await click(await byText("[role^=menuitem]", "结束会话…"));
    await waitFor("[aria-modal='true']");
    await sleep(400);
  }),
  panel("panel-close-undo", "W 关掉已结束的 tab 之后", "切到已结束的「周报草稿」按 W：tab 先摘下来，右下角给 8 秒「撤销」", async () => {
    await mainReady(ALL_TABS);
    await openTab(2);
    document.querySelector<HTMLElement>("#message-stream")?.focus();
    (document.activeElement as HTMLElement | null)?.blur();
    await key("w");
    await sleep(600);
  }),
  panel("panel-unseen-menu", "说不清的 tab 的右键菜单", "gpu-01 连不上、它那个固定着的会话说不清：右键菜单里恢复置灰、说为什么", async () => {
    await mainReady(ALL_TABS);
    const t = [...document.querySelectorAll<HTMLElement>("#tab-bar .tab")].find((x) => (x.textContent ?? "").includes("排序模型"));
    await rightClick(t ?? "#tab-bar .tab");
    await sleep(900);
  }, unseenPinnedWorld),
  {
    ...panel("panel-batch-kill-short", "批量结束的确认框 · 矮窗口", "全选之后右键「结束会话（n）」，窗口只有 220 高：清单在框里滚、按钮够得着，焦点在「取消」", async () => {
      await mainReady(ALL_TABS);
      const tabs = [...document.querySelectorAll<HTMLElement>("#tab-bar .tab")];
      for (const t of tabs) {
        const r = t.getBoundingClientRect();
        t.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, ctrlKey: true, clientX: r.left + 10, clientY: r.top + 5 }));
        await sleep(60);
      }
      await rightClick(tabs[0]);
      await sleep(600);
      await click(await byText("[role^=menuitem]", copyPattern("tabBatch.menu.stop")));
      await waitFor("[aria-modal='true']");
      await sleep(400);
    }),
    height: 220,
  },
  panel("panel-agents", "子 agent 面板", "状态栏点「agent 2 · 1 在跑」：本会话的子 agent 列表（一个跑完、一个在跑）", async () => {
    await mainReady(ALL_TABS);
    await click(".status-agents");
    await waitFor("#bottom-drawer .agent-row");
    await sleep(500);
  }),
  panel("panel-tasks", "任务面板", "状态栏点「任务 2/4」：本会话的任务清单", async () => {
    await mainReady(ALL_TABS);
    await click(".status-tasks");
    await waitFor("#bottom-drawer .tasks-item");
    await sleep(500);
  }),
  panel("panel-account", "账号面板", "状态栏点账号按钮（本会话）：本会话的「账号」面板（缺省世界里中转还没见过这个会话）", async () => {
    await mainReady(ALL_TABS);
    await click(".status-account");
    await waitFor('aside[role="dialog"]');
    await sleep(500);
  }),
  panel("panel-cmdk", "命令面板", "Ctrl+K：空输入时分组（需要你 · 当前会话 · 打开 · 窗口 · 账号），会话行带数字键", async () => {
    await openCommandBar();
    await sleep(300);
  }),
  panel("panel-cmdk-filter", "命令面板 · 过滤", "命令面板里输入「设置」", async () => {
    await openCommandBar();
    await type("[data-role=command-input]", "设置");
  }),
  panel("panel-cmdk-empty", "命令面板 · 没有匹配", "命令面板里输入一串匹配不到的字", async () => {
    await openCommandBar();
    await type("[data-role=command-input]", "zzzzqqq");
  }),
  panel("panel-cmdk-machine", "命令面板 · 按机器名找会话", "命令面板里输入「gpu-01」：切到那台上的会话，机器名只出一次", async () => {
    await openCommandBar();
    await type("[data-role=command-input]", "gpu-01");
  }),
  panel("panel-find", "会话内查找", "Ctrl+F：停 300ms 自己找「重试」；每条「谁 · 第几轮 · 时刻」＋ 片段，回车选中第一条并跳过去（高亮 1.5 秒）", async () => {
    await mainReady(ALL_TABS);
    await key("f", { ctrl: true });
    await waitFor("[data-role=find-input]");
    await type("[data-role=find-input]", "重试");
    await sleep(700);
    await key("Enter");
    await sleep(500);
  }),
  panel("panel-find-missing", "会话内查找 · 跳到找不到的那条", "选中最后一条（那张卡不在这份会话的流里）：那一行下面写一句", async () => {
    await mainReady(ALL_TABS);
    await key("f", { ctrl: true });
    await waitFor("[data-role=find-input]");
    await type("[data-role=find-input]", "重试");
    await sleep(700);
    await key("ArrowUp");
    await sleep(700);
  }),
  panel("panel-outline", "大纲", "Ctrl+F 面板的「大纲」页：本会话里你说过的每一句", async () => {
    await mainReady(ALL_TABS);
    await key("f", { ctrl: true });
    await waitFor("[data-role=session-find-panel] [role=tab]");
    const tab = [...document.querySelectorAll<HTMLElement>("[data-role=session-find-panel] [role=tab]")].find((t) => t.dataset.key === "outline")!;
    await click(tab);
    await sleep(800);
  }),
  panel("panel-keys", "快捷键一览", "按 ?：动作 · 当前键 · 何时生效，按分组；顶上按名称或按键过滤，底下［改快捷键…］", async () => {
    await mainReady(ALL_TABS);
    await key("?", { shift: true, code: "Slash" });
    await waitFor("[data-role=keys-filter]");
    await sleep(400);
  }),
  panel("panel-grid", "监控板", "右上田字格：所有会话一屏看", async () => {
    await mainReady(ALL_TABS);
    await click(".grid-monitor-trigger");
    await waitFor(".grid-monitor");
    await sleep(600);
  }),
  panel("panel-grid-peek", "监控板 · 选中一格", "监控板里点开第一格：右侧看那个会话的模型、碰过的文件、子 agent", async () => {
    await mainReady(ALL_TABS);
    await click(".grid-monitor-trigger");
    await waitFor(".grid-monitor");
    await sleep(400);
    const cell = document.querySelector<HTMLElement>(".grid-monitor-body [class*='cell']");
    if (cell) await click(cell);
    await sleep(600);
  }),
  panel("panel-history", "历史", "右上时钟：按项目列出本机与远端的历史会话", async () => {
    await mainReady(ALL_TABS);
    await click(".history-trigger");
    await waitFor(".history-view");
    await sleep(1200);
  }),
  panel("panel-cc-bus", "cc-bus 驾驶舱", "命令面板里打开「cc-bus 驾驶舱」：各台机器上 claude 实例之间的消息", async () => {
    await openCommandBar();
    await type("[data-role=command-input]", "cc-bus");
    const item = await byText("[data-role=command-item]", /cc-bus/);
    await click(item);
    await sleep(1200);
  }),
  panel("panel-tab-menu", "tab 右键菜单 · 本机会话", "在第一个 tab 上点右键", async () => {
    await mainReady(ALL_TABS);
    await rightClick("#tab-bar .tab");
    await sleep(900);
  }),
  panel("panel-tab-menu-remote", "tab 右键菜单 · 远端会话", "在 devbox 那个 tab 上点右键", async () => {
    await mainReady(ALL_TABS);
    await rightClick(document.querySelectorAll("#tab-bar .tab")[3]);
    await sleep(900);
  }),
  panel("panel-tab-menu-ended", "tab 右键菜单 · 已结束的会话", "在已结束那个 tab 上点右键", async () => {
    await mainReady(ALL_TABS);
    await rightClick(document.querySelectorAll("#tab-bar .tab")[2]);
    await sleep(900);
  }),
  panel("panel-batch-menu", "多选 tab 的右键菜单", "按住 Ctrl 选中三个 tab，再在其中一个上点右键：对选中的那几个一起做", async () => {
    await mainReady(ALL_TABS);
    const tabs = document.querySelectorAll<HTMLElement>("#tab-bar .tab");
    await click(tabs[0]);
    for (const i of [1, 3]) {
      const r = tabs[i].getBoundingClientRect();
      tabs[i].dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, ctrlKey: true, clientX: r.left + 30, clientY: r.top + 5 }));
      await sleep(150);
    }
    await rightClick(tabs[3]);
    await sleep(900);
  }),
  panel("panel-sftp-picker", "文件按钮 · 选机器", "右上文件夹：有多台机器时先选去哪台", async () => {
    await mainReady(ALL_TABS);
    await click(".sftp-trigger");
    await sleep(600);
  }),
  panel("panel-stream-closed", "会话流断了的提示", "devbox 的会话流被那台后端关掉：右下角报错提示", async () => {
    await mainReady(ALL_TABS - 2);
    await sleep(800);
  }, closedWorld),
  panel("panel-agents-tasks", "先开任务面板、再开子 agent 面板", "先点「任务 2/4」再点「agent 2 · 1 在跑」：同一个抽屉换页", async () => {
    await mainReady(ALL_TABS);
    await click(".status-tasks");
    await waitFor("#bottom-drawer .tasks-item");
    await click(".status-agents");
    await waitFor("#bottom-drawer .agent-row");
    await sleep(500);
  }),
  panel("panel-agents-switch-back", "在别的 tab 上开了子 agent 面板再切回来", "第一个 tab 开着任务面板，切到没有任务的「表格分页」点开子 agent 面板，再切回第一个 tab：开着的仍是子 agent 面板", async () => {
    await mainReady(ALL_TABS);
    await click(".status-tasks");
    await waitFor("#bottom-drawer .tasks-item");
    await openTab(1);
    await click(".status-agents");
    await waitFor("#bottom-drawer .agent-row");
    await openTab(0);
    await sleep(500);
  }, agentsOnlyWorld),
  {
    ...panel("panel-tasks-esc", "任务页 · 启动就开着时按 Esc", "上次退出时抽屉开在任务页：这次启动直接开着，按一下 Esc 收起", async () => {
      await mainReady(ALL_TABS);
      await sleep(400);
      (document.activeElement as HTMLElement | null)?.blur();
      await key("Escape");
      await sleep(400);
    }),
    storage: { "cc-monitor.tab-bar-w": "260", "cc-monitor.cmdk-hint.seen": "1", "cc-monitor.bottom-drawer": '{"page":"tasks","height":240}' },
  },
  panel("panel-account-ended-last", "账号徽标 · 已结束的远端会话", "devbox 上已结束的「账单导出」：tab 上的账号徽标是那台记着的上次用的号（虚线）", async () => {
    await mainReady(ALL_TABS);
    await openTab(3);
    await sleep(600);
  }, remoteEndedWorld),
  panel("panel-account-unavailable", "Resume · 上次用的号选不了", "右键已结束的「账单导出」→ Resume → 直连：那台说上次用的号 personal 选不了，不起、给「改用 work」的选择", async () => {
    await mainReady(ALL_TABS);
    await rightClick(document.querySelectorAll<HTMLElement>("#tab-bar .tab")[3]);
    await click(await byText("[role^=menuitem]", copyText("tabMenu.item.resume")));
    await click(await byText("[role^=menuitem]", copyText("resumeMenu.run.direct")));
    await waitFor("#kit-toast-stack > [data-level=error]");
    await sleep(600);
  }, accountGoneWorld),
  panel("panel-batch-start-menu", "批量菜单 · 起会话", "Ctrl 点选两个 tab、右键：批量菜单里「在 tmux 里后台起 / 各开一个终端」", async () => {
    await mainReady(ALL_TABS);
    const tabs = document.querySelectorAll<HTMLElement>("#tab-bar .tab");
    for (const i of [2, 3]) {
      const r = tabs[i].getBoundingClientRect();
      const at = { bubbles: true, cancelable: true, clientX: r.left + 20, clientY: r.top + r.height / 2, button: 0, ctrlKey: true };
      tabs[i].dispatchEvent(new MouseEvent("click", at));
      await sleep(150);
    }
    await rightClick(tabs[3]);
    await sleep(600);
  }, remoteEndedWorld),
  panel("panel-cmdk-set-default", "命令面板 · 设当前账号", "命令面板里输入「当前账号」：每个号一条「设为当前账号」（写那台账号库清单的默认号）", async () => {
    await openCommandBar();
    await type("[data-role=command-input]", "当前账号");
  }),
  panel("panel-account-reclick", "账号面板 · 再点一下按钮", "面板开着时再点一下状态栏账号按钮：面板收起", async () => {
    await mainReady(ALL_TABS);
    await click(".status-account");
    await waitFor('aside[role="dialog"]');
    await sleep(300);
    await click(".status-account");
    await sleep(500);
  }),
  panel("panel-sftp-reclick", "文件按钮 · 再点一下", "选机器的小单开着时再点一下文件按钮：小单收起", async () => {
    await mainReady(ALL_TABS);
    await click(".sftp-trigger");
    await waitFor("[role=menu]");
    await sleep(300);
    await click(".sftp-trigger");
    await sleep(500);
  }),
  panel("panel-tab-menu-edge", "贴着窗口底边点右键", "在窗口最底下右键一个 tab：菜单往上翻，不伸出窗口", async () => {
    await mainReady(ALL_TABS);
    const tabs = document.querySelectorAll<HTMLElement>("#tab-bar .tab");
    const t = tabs[tabs.length - 1];
    const r = t.getBoundingClientRect();
    t.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: r.left + 20, clientY: window.innerHeight - 12, button: 2 }));
    await sleep(900);
  }),
  panel("panel-behavior-toggle", "按快捷键翻「自动跟随」", "给「自动跟随」绑了 J，按一下：右下角说翻成了什么", async () => {
    await mainReady(ALL_TABS);
    (document.activeElement as HTMLElement | null)?.blur();
    await key("j");
    await sleep(700);
  }, behaviorKeyWorld),
  panel("panel-messages", "状态栏「消息」", "关掉一个已结束的标签页、按快捷键翻一下自动跟随，再点状态栏最左「消息」：最近的提示（时刻 · 图标 · 一句 · 还能做的动作）", async () => {
    await mainReady(ALL_TABS);
    await openTab(2);
    (document.activeElement as HTMLElement | null)?.blur();
    await key("w");
    await sleep(300);
    await key("j");
    await sleep(300);
    await click("[data-role=status-messages]");
    await waitFor("[data-role=messages-list]");
    await sleep(400);
  }, behaviorKeyWorld),
  panel("panel-batch-kill", "批量结束 · 确认框", "全选之后右键「结束会话（n）」：中断 / 保留逐项写，清单前 8 个 ＋ 另外几个，清单下一行已结束的跳过", async () => {
    await mainReady(ALL_TABS);
    const tabs = [...document.querySelectorAll<HTMLElement>("#tab-bar .tab")];
    for (const t of tabs) {
      const r = t.getBoundingClientRect();
      t.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, ctrlKey: true, clientX: r.left + 10, clientY: r.top + 5 }));
      await sleep(60);
    }
    await rightClick(tabs[0]);
    await sleep(600);
    await click(await byText("[role^=menuitem]", copyPattern("tabBatch.menu.stop")));
    await waitFor("[aria-modal='true']");
    await sleep(400);
  }),
  panel("panel-batch-result-view", "批量结束 · 结果［查看］", "确认之后：toast 只一句汇总（已结束 4 · 失败 2）＋［查看］；点了打开「消息」、展开那一条，逐条原因在那里", async () => {
    await mainReady(ALL_TABS);
    const tabs = [...document.querySelectorAll<HTMLElement>("#tab-bar .tab")];
    for (const t of tabs) {
      const r = t.getBoundingClientRect();
      t.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, ctrlKey: true, clientX: r.left + 10, clientY: r.top + 5 }));
      await sleep(60);
    }
    await rightClick(tabs[0]);
    await sleep(600);
    await click(await byText("[role^=menuitem]", copyPattern("tabBatch.menu.stop")));
    await waitFor("[aria-modal='true']");
    await sleep(300);
    await click(await waitFor("[aria-modal='true'] button[data-kind='danger']"));
    await click(await byText("#kit-toast-stack button", copyText("tabBatch.result.view")));
    await waitFor("[data-role=message-more]");
    await sleep(500);
  }, () => {
    const w = defaultWorld();
    w.ops["sessions-stop"] = (origin, req) => {
      if (origin === "devbox") throw { err: { Hop: { idx: 0, tag: "open", reach: "NotSent", why: "Unreachable" } }, body: [] };
      return { results: (req.sids as string[]).map((sid) => ({ sid, outcome: "done", why: null, detail: "", session: null, bus: null, cmd: null, account: null, unavailable: null })) };
    };
    return w;
  }),
  panel(
    "panel-filewin-open-failed",
    "打开文件 · 窗口没起来",
    "文件窗口进程一启动就退了：toast 标题是壳写好的那一句（只说原因词），灰字是那台，［复制详情］里是退出状态与它的错误输出",
    async () => {
      await mainReady(ALL_TABS);
      await click(".sftp-trigger");
      const item = await byText("[role^=menuitem]", "devbox", 2000).catch(() => null);
      if (item) await click(item);
      await byText("[role=alert]", /文件窗口未启动/);
      await sleep(300);
    },
    () => {
      const w = defaultWorld();
      w.commands.open_file_window = () =>
        Promise.reject({
          said: "文件窗口未启动 · 程序出错",
          detail: "时刻：2026-10-08 15:20:44 +08:00\n本机：cc-monitor 4.1.5 (p13-shots) · Linux x86_64\n命令：open_file_window\n码：exit status: 1\n原话：thread 'main' panicked at src/main.rs:12:5:\nwinit: no display",
        });
      return w;
    },
  ),
  panel(
    "panel-settings-open-failed",
    "设置窗口没开起来",
    "壳命令失败：toast 标题是壳写好的那一句（哪件事 ＋ 原因词），不再在界面那句后面接原文；［复制详情］里是时刻 · 本机 · 命令 · 原话",
    async () => {
      await mainReady(ALL_TABS);
      await click(".settings-trigger");
      await byText("[role=alert]", /打开设置窗口失败/);
      await sleep(300);
    },
    () => {
      const w = defaultWorld();
      w.commands.open_settings_window = () =>
        Promise.reject({
          said: "打开设置窗口失败 · 程序出错",
          detail: "时刻：2026-10-08 16:05:12 +08:00\n本机：cc-monitor 4.1.5 (p13-shots) · Linux x86_64\n命令：open_settings_window\n原话：a webview with label `settings` already exists",
        });
      return w;
    },
  ),
  panel("panel-first-run", "首次打开", "第一次开：命令面板入口高亮、tab 栏默认宽度", async () => {
    await mainReady(ALL_TABS);
  }),
];

/** ↗ 那几张：装成 Windows 上的 cc-monitor；壳 / 那台 / 本机后端各答什么由 `tune` 定。 */
function frontScene(id: string, title: string, desc: string, act: Scene["act"], tune: (w: World) => void): Scene {
  const w = (): World => {
    const x = defaultWorld();
    tune(x);
    return x;
  };
  return { ...panel(id, title, desc, act, w), hostOs: "windows" };
}

async function clickHeadFront(): Promise<void> {
  await click("[data-role=head-front]");
}

/** devbox 那一问回「那台旧」（`Unsupported`）；部署那一下答什么由 `deploy` 定。 */
function tooOld(deploy: () => unknown): (w: World) => void {
  return (w) => {
    w.ops["session-terminals"] = () => {
      throw { err: "Unsupported", body: [] };
    };
    w.commands.deploy_remote_backend = deploy;
    // 换上了就重拨那条流（主窗口 `reconnect`）。
    w.commands.backend_start = () => undefined;
  };
}

async function frontTooOld(): Promise<void> {
  await mainReady(ALL_TABS);
  await openTab(3);
  await sleep(400);
  await clickHeadFront();
  await waitFor("[data-role=front-result]");
  await sleep(400);
}

/** 浮层［更新］→ 会打断什么的确认框里点「更新」（devbox 上有经它的会话）。 */
async function clickUpdate(): Promise<void> {
  await click(await byText("[data-role=front-result] button", "更新"));
  await click(await byText("[role=dialog] button", /^更新$/));
}

/** 高 DPI：1.5x / 2x 下 1px 线、图标、字形照常（同一屏，像素比不同）。 */
export const DPI_SCENES: Scene[] = [1.5, 2].map((scale) => ({
  ...panel(`main-dpi-${scale}x`, `主窗口 · ${scale}x`, `设备像素比 ${scale}：标签页栏、会话头、消息流、状态栏的 1px 线与图标都照常`, async () => {
    await mainReady(ALL_TABS);
    await sleep(400);
  }),
  scale,
}));

export const FRONT_SCENES: Scene[] = [
  frontScene("panel-front-several", "↗ · 分不清是哪个窗口", "本机会话：Windows Terminal 开着 3 个窗口、这个终端没登记 ⇒ 不挑一个切、不闪；浮层锚在会话头的 ↗ 下，照实说拉不了、带候选个数", async () => {
    await mainReady(ALL_TABS);
    await clickHeadFront();
    await waitFor("[data-role=front-result]");
    await sleep(400);
  }, (w) => {
    w.commands.bring_terminal_to_front = () => ({ kind: "several", program: "WindowsTerminal.exe", count: 3 });
  }),
  { ...frontScene("panel-front-wayland", "↗ · Wayland 桌面上切不了", "Linux 的 Wayland 会话（GNOME）：别的程序的窗口 cc-monitor 看不见也切不了 ⇒ 照实说，给［在 cc-monitor 里打开］", async () => {
    await mainReady(ALL_TABS);
    await clickHeadFront();
    await waitFor("[data-role=front-result]");
    await sleep(400);
  }, (w) => {
    w.commands.bring_terminal_to_front = () => ({ kind: "desktop-wont-switch", desktop: "GNOME" });
  }), hostOs: "linux" },
  { ...frontScene("panel-front-wayland-open-here", "↗ · ［在 cc-monitor 里打开］", "上一张点了［在 cc-monitor 里打开］⇒ 浮层收起，底部抽屉开到这个会话的「终端」页", async () => {
    await mainReady(ALL_TABS);
    await clickHeadFront();
    await waitFor("[data-role=front-result]");
    await click(await byText("[data-role=front-result] button", "在 cc-monitor 里打开"));
    await sleep(800);
  }, (w) => {
    w.commands.bring_terminal_to_front = () => ({ kind: "desktop-wont-switch", desktop: "GNOME" });
  }), hostOs: "linux" },
  frontScene("panel-front-background-tab", "↗ · 终端在后台标签页", "单独起的 PowerShell 被 Win11 交给「终端」应用（进程链断）、借它的控制台挂了记号标题，却没有窗口带着它（那个标签页不在前台）：浮层照实说找不到窗口，灰字给改法", async () => {
    await mainReady(ALL_TABS);
    await clickHeadFront();
    await waitFor("[data-role=front-result]");
    await sleep(400);
  }, (w) => {
    w.commands.bring_terminal_to_front = () => ({ kind: "background-tab", program: "ssh.exe" });
  }),
  frontScene("panel-front-hosted", "↗ · 终端由 Windows 托管", "进程链断在被交给「终端」应用的 PowerShell 上、连它的控制台也借不到：浮层说定位失败，灰字给改法", async () => {
    await mainReady(ALL_TABS);
    await clickHeadFront();
    await waitFor("[data-role=front-result]");
    await sleep(400);
  }, (w) => {
    w.commands.bring_terminal_to_front = () => ({ kind: "hosted-by-wt", program: "ssh.exe" });
  }),
  frontScene("panel-front-unbound", "↗ · 本机终端没登记", "在接上终端之前开的 PowerShell：浮层给［接上终端］（直达设置那一节）", async () => {
    await mainReady(ALL_TABS);
    await clickHeadFront();
    await waitFor("[data-role=front-result]");
    await sleep(400);
  }, (w) => {
    w.commands.bring_terminal_to_front = () => ({ kind: "unbound" });
  }),
  frontScene("panel-front-remote-detached", "↗ · 远端 tmux 里没有终端连着", "devbox 那台说：tmux 会话在、没有终端连着 ⇒ 无终端窗口 ＋［在终端里打开］（灰）", async () => {
    await mainReady(ALL_TABS);
    await openTab(3);
    await sleep(400);
    await clickHeadFront();
    await waitFor("[data-role=front-result]");
    await sleep(400);
  }, (w) => {
    w.ops["session-terminals"] = () => ({ terminals: [], why: "detached" });
  }),
  frontScene("panel-front-remote-offline", "↗ · 连不上那台", "devbox 那一问够不着 ⇒ 未切换 · devbox 离线 ＋［重新连接］（红）", async () => {
    await mainReady(ALL_TABS);
    await openTab(3);
    await sleep(400);
    await clickHeadFront();
    await waitFor("[data-role=front-result]");
    await sleep(400);
  }, (w) => {
    w.ops["session-terminals"] = () => {
      throw { err: { Hop: { idx: 0, tag: "open", reach: "NotSent", why: "Unreachable" } }, body: [] };
    };
  }),
  frontScene("panel-front-update", "↗ · 那台后端旧", "devbox 那台后端不认这一问 ⇒「devbox 要更新」＋［更新］（就地做，不开设置）", async () => {
    await frontTooOld();
  }, tooOld(() => new Promise(() => {}))),
  frontScene("panel-front-updating", "↗ · ［更新］在做", "点了［更新］、确认框里点「更新」⇒ 同一个浮层说正在更新 devbox", async () => {
    await frontTooOld();
    await clickUpdate();
    await waitFor("[data-role=front-result]");
    await sleep(400);
  }, tooOld(() => new Promise(() => {}))),
  frontScene("panel-front-updated", "↗ · ［更新］换上了", "部署回来了、重拨成了 ⇒ 浮层说 devbox 已更新、带后端那句结果，只留关闭", async () => {
    await frontTooOld();
    await clickUpdate();
    await byText("[data-role=front-result]", /已更新/);
    await sleep(400);
  }, tooOld(() => "已安装后端（p13，x86_64）到 ~/.cc-monitor/bin/ccm（原 p12 · 换为 p13）。")),
  frontScene("panel-front-updated-redial-failed", "↗ · ［更新］换上了、重拨没成", "部署回来了、重拨那台没成 ⇒ 已更新那句后面说重新连接失败，给［重新连接］", async () => {
    await frontTooOld();
    await clickUpdate();
    await byText("[data-role=front-result]", /重新连接失败/);
    await sleep(400);
  }, (w) => {
    tooOld(() => "已安装后端（p13，x86_64）到 ~/.cc-monitor/bin/ccm（原 p12 · 换为 p13）。")(w);
    w.commands.backend_start = () => Promise.reject("拨不通");
  }),
  frontScene("panel-front-update-failed", "↗ · ［更新］没成", "那台答不出系统（默认 shell 没有 uname）⇒ 浮层红着说更新失败、那一句只说原因词，［重试］再更新一次 ·［复制详情］（那台的原话在详情里）", async () => {
    await frontTooOld();
    await clickUpdate();
    await byText("[data-role=front-result]", /更新失败/);
    await sleep(400);
  }, tooOld(() =>
    Promise.reject({
      said: "devbox 系统未知 · 未安装 · 应答报错",
      detail: "时刻：2026-10-08 14:02:11 +08:00\n机器：Linux x86_64 · 后端 p13-shots\n命令：deploy-plan\n码：refused\n原话：uname : The term 'uname' is not recognized",
    }),
  )),
  frontScene("panel-front-busy", "↗ · 在找终端", "壳那一跳超过 300ms 还没回 ⇒ ↗ 转圈、旁边「查找终端…」", async () => {
    await mainReady(ALL_TABS);
    await clickHeadFront();
    await waitFor("[data-role=front-busy]");
    await sleep(300);
  }, (w) => {
    w.commands.bring_terminal_to_front = () => new Promise(() => {});
  }),
];

// 首次打开那一张不带默认存储（tab 栏默认宽、命令面板提示没看过）
PANEL_SCENES[PANEL_SCENES.length - 1].storage = {};

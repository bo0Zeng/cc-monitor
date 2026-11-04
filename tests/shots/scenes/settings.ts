/**
 * 设置窗：各页、机器的各子页、各种状态。
 *
 * 每一幕只按条件等（判据 `tests/shots/scene-waits.vitest.ts`）：等那一页露出来、页里没有在读的那一块（[`shown`]），
 * 等点了之后该出来的那个元素 · 那个状态；找不着就抛（这一张算没截成），不按钟点、不 `?.` 放过。
 * 折叠头按结构找（`[data-fold-head]` · `aria-expanded`），不按头里的字（头里带摘要）。
 */
import { copyText } from "../../../src/frontend/ui/copy-table";
import type { Scene } from "./index";
import type { World } from "../fake/types";
import { defaultWorld, LOCAL } from "../fake/world";
import { HOME, putAccounts, putPrefs, putQuota } from "../disk";
import { ACCOUNTS } from "../fake/ops";
import { PROFILE_ACCOUNTS, PROFILES_FILE, putProfileAccounts, putProfiles, staleBook, type ProfilesLook } from "../disk/profiles";
import { emit } from "@tauri-apps/api/event";
import { byText, click, until, waitCount, waitFor } from "./helpers";
import { WALL_CEILING_MS } from "../../test-support/wall-ceiling";
import { budgetWithin, callTyped } from "../../../src/frontend/ui/ipc/chan-caller";
import { readChores } from "../../../src/frontend/ui/settings/data-reads";

function settings(id: string, title: string, desc: string, act: Scene["act"], world: () => World = defaultWorld, height = 740, width = 960): Scene {
  return { id, page: "settings", dir: "设置", title, desc, width, height, world, act };
}

/** 此刻露着的那一页（连机器页里露着的那一栏）。 */
const PAGE = ".settings-page:not([hidden])";

/** 页里还在读的那几句（「读取中」后面跟「断」的是连接读取中断那一句，是成品不是在读）。 */
const READING = new RegExp(
  [copyText("kit.state.reading"), copyText("extPage.card.loading"), copyText("extPage.hooks.loading")]
    .map((w) => `${w.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}(?!断)`)
    .join("|"),
);

const visible = (e: HTMLElement): boolean => e.checkVisibility();

/** 那一块里还在读的那一处（骨架 `aria-busy` · 读取中那几句）；读完了 ⇒ `null`。 */
function reading(el: HTMLElement): string | null {
  if ([...el.querySelectorAll<HTMLElement>('[aria-busy="true"]')].some(visible)) return "骨架还在";
  return READING.exec(el.innerText)?.[0] ?? null;
}

/** 等那一块露出来、读完；到兜底上限 ⇒ 抛，说清卡在哪一样。 */
async function quiet(get: () => HTMLElement | null, what: string): Promise<HTMLElement> {
  const end = performance.now() + WALL_CEILING_MS;
  for (;;) {
    const el = get();
    const why = el === null ? "不在" : !visible(el) ? "没露出来" : reading(el);
    if (why === null && el !== null) return el;
    if (performance.now() > end) throw new Error(`等不到 ${what} 露出来且读完（${why}）`);
    await new Promise((r) => requestAnimationFrame(() => r(null)));
  }
}

/** 此刻露着的最外一页。 */
function activePage(): HTMLElement | null {
  for (const p of document.querySelectorAll<HTMLElement>(".settings-page")) if (p.closest("[hidden]") === null && visible(p)) return p;
  return null;
}

/** 等路由 `id` 那一页露出来、读完；不给 ⇒ 此刻露着的那一页。 */
function shown(id?: string): Promise<HTMLElement> {
  if (id === undefined) return quiet(activePage, "设置窗的那一页");
  return quiet(() => document.getElementById(`settings-panel-${id}`), `「${id}」那一页`);
}

/**
 * 点导航里的一页（id 照路由器的 `settings-tab-<id>`），等它露出来、读完。不给 ⇒ 等打开时落在的那一页。
 * 机器那几页随机器表读回会注销重建（`removeRoute` 落回父页）：点的那一下若落在旧的那颗上，那一颗不再是选中的 ⇒ 照新的那颗再点，
 * 直到它选中、那一页露出来。
 */
async function go(...ids: string[]): Promise<HTMLElement> {
  await waitFor(".settings-nav");
  let page = await shown();
  for (const id of ids) {
    const tab = `#${CSS.escape(`settings-tab-${id}`)}`;
    page = await quiet(() => {
      const b = document.querySelector<HTMLElement>(tab);
      if (b !== null && b.getAttribute("aria-selected") !== "true") b.click();
      return document.getElementById(`settings-panel-${id}`);
    }, `「${id}」那一页`);
  }
  return page;
}

/** 点了之后：那一页上再没有在读的。 */
const settle = (): Promise<HTMLElement> => shown();

/** 展开此刻那一页里 `sel` 那一折（按结构：它的 `[data-fold-head]`），等折里那一块露出来、读完。 */
async function openFold(sel: string): Promise<HTMLElement> {
  const head = await waitFor(`${PAGE} ${sel} > [data-fold-head]`);
  if (head.getAttribute("aria-expanded") !== "true") await click(head);
  await until(() => (head.getAttribute("aria-expanded") === "true" ? head : null));
  return quiet(() => head.nextElementSibling as HTMLElement | null, `${sel} 那一折`);
}

/** 卡头里「这台上的 cc-monitor」那一折。 */
const openCc = (): Promise<HTMLElement> => openFold(".machine-monitor-fold");

/** 卡头里「连接设置」（只远端）：右上那颗开关键，等那一块露出来。 */
async function openConn(): Promise<HTMLElement> {
  const toggle = await waitFor(`${PAGE} .machine-conn-toggle`);
  if (toggle.getAttribute("aria-expanded") !== "true") await click(toggle);
  return quiet(() => document.querySelector<HTMLElement>(`${PAGE} .machine-conn:not([hidden])`), "连接设置那一块");
}

/** 弹出来的确认框 / 对话框。 */
const dialog = (): Promise<HTMLElement> => waitFor('[aria-modal="true"]');

/** 弹出来的菜单。 */
const menu = (): Promise<HTMLElement> => waitFor('[role="menu"]');

/** 等 `el` 的某个属性成某个值。 */
const attr = <T extends Element>(el: T, name: string, value: string): Promise<T> => until(() => (el.getAttribute(name) === value ? el : null));

/**
 * 等这一页上那几台的状态字都到了（本机后端那几条常驻流真去接各台，接到哪一样要一会儿；按那几个字等，不按钟点猜）。
 * `keys` 是 `machineState.word.*` 的末段（`installing` · `needsUpdate` · `connecting` · `hostKey` …）。
 */
async function says(...keys: string[]): Promise<void> {
  for (const k of keys) await byText(PAGE, copyText(`machineState.word.${k}` as Parameters<typeof copyText>[0]));
}

const page = (id: string, title: string, desc: string, ...route: string[]): Scene =>
  settings(id, title, desc, async () => {
    await go(...route);
  });

/** 账号页：那台的额度账有数（work 被拒 · personal 63% / 41% · api 按量）。`verifyFail` ⇒ 打开时核出一处对不上。 */
function acctWorld(verifyFail = false): () => World {
  return () => {
    const w = defaultWorld();
    const now = Math.floor(Date.now() / 1000);
    const win = (p5: number, p7: number) => [
      { name: "five_hour", used: p5 / 100, resetsAt: now + 5400 },
      { name: "seven_day", used: p7 / 100, resetsAt: now + 4 * 86400 },
    ];
    // work 用满被拒 · personal 被拒而没用满 · api 按量（窗口都没有）。
    putQuota(w.disk[LOCAL], [
      { account: "work", seenAt: now - 120, status: "rejected", refused: true, limiting: "five_hour", resetsAt: now + 5400, windows: win(100, 78) },
      { account: "personal", seenAt: now - 120, status: "rejected", refused: true, limiting: "five_hour", resetsAt: now + 5400, windows: win(58, 41) },
      { account: "api", seenAt: now - 120, status: "allowed", windows: [] },
    ]);
    if (verifyFail) {
      w.ops["accounts-verify"] = () => ({ pass: false, fails: 1, warns: 0, checks: [{ level: "fail", account: "personal", text: "登录信息缺失" }] });
    }
    return w;
  };
}

/** 配置文件那一页（devbox）照一种样子放好原文、账号库换成表单那几个号（`disk/profiles.ts`；读法与成品全在真后端）。默认世界那台放的是典型的那一份。 */
const profilesWorld = (look: ProfilesLook) => (): World => {
  const w = defaultWorld();
  putProfileAccounts(w.disk.devbox);
  delete w.disk.devbox.files[PROFILES_FILE];
  putProfiles(w.disk.devbox, look);
  return w;
};

/** 别名那一行露出来、清单读回之后（树或卡片画好）。 */
async function profilesPage(): Promise<void> {
  await go("machine:devbox", "machine:devbox#config");
  await waitFor(`${PAGE} [data-role=profiles] .prof-tree, ${PAGE} [data-role=file-problem], ${PAGE} [data-role=seed]`);
  await settle();
}

const pf = (sel: string): string => `${PAGE} [data-role=profiles] ${sel}`;

/** 点清单里一行、等右栏（或那一行下面）的合并表出来。 */
async function pickProfile(name: string): Promise<void> {
  await click(pf(`.prof-trow[data-name="${name}"]`));
  await waitFor(pf("[data-role=merge]"));
  await settle();
}

/** 点一行的「改」、等表单出来。 */
async function editProfile(name: string): Promise<void> {
  await click(pf(`.prof-trow[data-name="${name}"] .cfg-link`));
  await waitFor(pf("[data-role=profile-form]"));
}

/** 点表单里那一颗分段键（账号），等它按下。 */
async function pressSeg(text: string): Promise<void> {
  await click(await byText(pf("[data-role=profile-form] .prof-segb"), text));
  await byText(pf('[data-role=profile-form] .prof-segb[aria-pressed="true"]'), text);
}

/** 把那一块滚到视口顶上（不在 ⇒ 抛）。 */
async function top(sel: string): Promise<void> {
  (await waitFor(sel)).scrollIntoView({ block: "start" });
}

/** 改表单里一个下拉 / 输入并通知（与人改一样走 change；不在 ⇒ 抛）。 */
async function setField(sel: string, value: string): Promise<void> {
  const e = await waitFor<HTMLInputElement | HTMLSelectElement>(sel);
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


/** 起本机后端被拒（口上是上一版留下的后端）那一刻壳推的本机后端那一格（`local-backend`）；字照壳的文案拼。 */
const localRefused = {
  state: "older",
  word: copyText("machineState.word.needsUpdate"),
  said: copyText("front.title.update", { machine: copyText("detail.label.local") }),
  // 壳起不来那几样只有［重试］（`local_backend_host::local_backend`）。
  fixes: [{ fix: "retry", label: copyText("front.act.retry") }],
  next: copyText("rsLocalBackendHost.start.refusedNext", {
    why: copyText("rsLocalBackendHost.hello.buildMismatch", { theirs: "p7a-4.1.0", ours: "p8c-4.1.1" }),
    pid: "4242",
    sock: "/home/user/.cc-monitor/run/backend.sock",
  }),
};

/** 壳推来本机后端被拒那一格，等问题行露出来。 */
async function pushLocalRefused(): Promise<void> {
  await emit("local-backend", localRefused);
  await byText(`${PAGE} .machine-problem`, localRefused.said);
  await settle();
}

/** 带着目的地打开（`settings-target`）：等那一处亮起来。 */
async function target(dest: Record<string, string>): Promise<void> {
  await go();
  await emit("settings-target", JSON.stringify(dest));
  await waitFor(".settings-highlight");
  await settle();
}

/** 「cc-monitor 放置的文件」那一栏；`machine` ⇒ 再点那台的 chip、等它按下。 */
async function placed(machine?: string): Promise<HTMLElement> {
  await go("data");
  await click(await byText(`${PAGE} button`, "cc-monitor 放置的文件"));
  const pane = await quiet(() => document.querySelector<HTMLElement>(`${PAGE} [data-pane=placed]`), "放置的文件那一栏");
  if (machine === undefined) return pane;
  await click(await byText(`${PAGE} .data-chip`, machine));
  await byText(`${PAGE} .data-chip[aria-pressed="true"]`, machine);
  return settle();
}

/** 那一栏里露着的「cc-monitor 的文件」那一节滚到顶上。 */
async function ownSection(): Promise<void> {
  const title = await until(() =>
    [...document.querySelectorAll<HTMLElement>(`${PAGE} [data-pane=placed] .data-section-title`)].find((e) => (e.textContent ?? "").includes("cc-monitor 的文件") && visible(e)),
  );
  title.scrollIntoView({ block: "start" });
}

/** 待办里 devbox 那一件点开（按那一行与展开那一格等）。 */
async function openChore(key: string): Promise<void> {
  const row = `${PAGE} [data-machine="devbox"] [data-chore="${key}"]`;
  await go("data");
  await click(await waitFor(`${row} [aria-expanded]`));
  await waitFor(`${row} .chore-detail`);
  (await waitFor(row)).scrollIntoView({ block: "start" });
  await settle();
}

/** 扩展页筛到 MCP（那颗筛选键按下）。 */
async function extMcp(): Promise<void> {
  await go("ext");
  await click(await byText(`${PAGE} .ext-filter button`, "MCP"));
  await byText(`${PAGE} .ext-filter button.is-active`, "MCP");
  await settle();
}

/** 扩展页点开一行：右边抽屉露出来、读完。 */
async function extDrawer(key: string): Promise<void> {
  await click(await waitFor(`${PAGE} .ext-row[data-key="${key}"]`));
  await quiet(() => document.querySelector<HTMLElement>(`${PAGE} .ext-drawer:not([hidden])`), "扩展抽屉");
}

/** 账号栏里点开一行。 */
async function openAcct(name: string): Promise<void> {
  await click(await waitFor(`${PAGE} [data-account="${name}"] .acct-row`));
  // 那一行随展开重画（换了一个节点）：按选择器等展开的那一行。
  await waitFor(`${PAGE} [data-account="${name}"] .acct-row[aria-expanded="true"]`);
  await settle();
}

/** 机器列表里那一台的 ⋯ 菜单（机器表是本机后端读盘答的：等那一行画出来再点）。 */
async function machineMenu(id: string): Promise<void> {
  await go("machines");
  await click(await waitFor(`.remote-machine-row[data-page-id="machine:${id}"] button[aria-label]`));
  await menu();
}

/** 「添加机器」那一框（从 ~/.ssh/config 选那一栏的行要等真后端逐个 `ssh -G` 读完）。 */
async function addMachine(): Promise<HTMLElement> {
  await go("machines");
  await click(await byText(`${PAGE} button`, "添加机器"));
  const box = await dialog();
  await waitFor(".add-machine-row");
  return box;
}

/** 那一格由后端判、落回来标红。 */
async function invalid(input: HTMLInputElement): Promise<void> {
  await attr(input, "aria-invalid", "true");
}

export const SETTINGS_SCENES: Scene[] = [
  settings("settings-ssh-import-taken", "设置 · 添加机器 · 撞名", "从 ~/.ssh/config 选：build-02 改名成 gpu-01（列表里已有），就地拦", async () => {
    await addMachine();
    const box = await until(() => [...document.querySelectorAll<HTMLInputElement>(".add-machine-row input[type=text]")].find((i) => i.value === "build-02"));
    box.value = "gpu-01";
    box.dispatchEvent(new Event("input", { bubbles: true }));
    await invalid(box);
  }),
  settings("settings-stop-interrupts", "设置 · 停止前问会打断什么", "devbox「这台上的 cc-monitor」点［停止…］：中断几项 · 保留会话照跑", async () => {
    await go("machine:devbox");
    await openCc();
    await click(await byText(`${PAGE} .machine-monitor-fold button`, "停止…"));
    await dialog();
  }),
  settings("settings-update-interrupts", "设置 · 更新前问会打断什么", "devbox 要更新时点［更新］：更新要停几秒，确认键「更新」", async () => {
    await go("machines");
    await click(await byText(".machine-problem button", "更新"));
    await dialog();
  }, () => {
    const w = defaultWorld();
    w.staleMachines = ["devbox"];
    return w;
  }),
  settings("settings-machines-hostkey", "设置 · 机器 · 主机指纹变了", "gpu-01 出示的指纹与记下的不一样：问题行［比对指纹…］", async () => {
    await go("machines");
    await says("hostKey");
  }, hostKeyWorld),
  settings("settings-hostkey-compare", "设置 · 比对主机指纹", "点［比对指纹…］：记下的 · 现在的 · 记于何时，默认焦点「不连接」", async () => {
    await go("machines");
    await click(await byText(".machine-problem button", "比对指纹…"));
    await dialog();
  }, hostKeyWorld),
  settings("settings-machines-installing", "设置 · 机器 · 正在装", "gpu-01 正在装 cc-monitor：虚线点 ＋「安装中」＋ 进度条", async () => {
    await go("machines");
    await says("installing");
  }, installingWorld),
  settings("settings-machines-pushed", "设置 · 机器 · 正在连 · 旧了", "本机后端那几条常驻流交来的两态：devbox 正在连（启动 cc-monitor）、gpu-01 比这一版旧", async () => {
    await go("machines");
    await says("connecting", "needsUpdate");
  }, () => {
    const w = defaultWorld();
    w.connectingMachines = ["devbox"];
    w.staleMachines = ["gpu-01"];
    return w;
  }),

  settings("settings-machines-local-refused", "设置 · 机器 · 本机后端被拒", "升级后旧的本机后端还占着口：本机那一行「要更新」＋［重试］，问题行下面是壳写好的下一步（结束哪个进程 · 进程号记在哪）", async () => {
    await go("machines");
    await pushLocalRefused();
  }),
  settings("settings-machine-local-refused", "设置 · 本机 · 本机后端被拒", "同上，本机那一页卡头：问题行 ＋ 下一步", async () => {
    await go("machine:<local>");
    await pushLocalRefused();
  }),
  settings("settings-landing", "设置 · 打开时", "设置窗打开时落在的那一页", async () => {
    await go();
  }),
  page("settings-general", "设置 · 通用", "通用只一组「行为」：两个开关 ＋（Linux）终端那一行", "general"),
  page("settings-appearance", "设置 · 外观", "外观：字 · 颜色 · 快捷键", "appearance"),
  page("settings-logs", "设置 · 日志", "日志：开关、级别、日志文件", "logs"),
  page("settings-data", "设置 · 文件与数据 · 待办", "文件与数据第一栏：顶上各类数 · 每台一段（急的在前）· 每件 点 · 名字 · 类 · 位置 · 现状 · 主按钮 · 已做的折起 · 断开的那台离线未检查", "data"),
  settings("settings-data-chore-open", "设置 · 待办 · 实时显示点开", "点开一件：为什么 · 怎么做 · diff · 复制这几行 / 复制改好的整份文件（含 2 件）· 不用了 · 存盘后自己认出", async () => {
    await openChore("relay:claude-code");
  }),
  settings("settings-data-chore-codex", "设置 · 待办 · Codex 实时显示点开", "Codex 那一件：位置 config.toml · openai_base_url · 在最前面加一行 · diff · 复制这一行 / 复制改好的整份文件", async () => {
    await openChore("relay:codex");
  }),
  settings("settings-data-placed", "设置 · 文件与数据 · 放了什么", "第二栏：机器 chip · Claude 目录 · 改过你的文件（撤回在哪 ［前往］）· cc-monitor 的文件", async () => {
    await placed();
  }),
  settings("settings-data-placed-own", "设置 · 文件与数据 · 放了什么 · 本机 cc-monitor 的文件", "本机那一块滚到「cc-monitor 的文件」：本机后端家里那几样照本机后端 data-report 的 own 排（与远端同一套行，打开 · 在文件夹中显示）· 再接界面自己那几份", async () => {
    await placed();
    await ownSection();
  }),
  settings("settings-data-placed-remote-own", "设置 · 文件与数据 · 放了什么 · devbox 的 cc-monitor 的文件", "devbox 那一块滚到「cc-monitor 的文件」：照那台 data-report 的 own 排，打开 ＝ 在文件窗口打开", async () => {
    await placed("devbox");
    await ownSection();
  }),
  settings("settings-data-placed-remote", "设置 · 文件与数据 · 放了什么 · devbox", "第二栏选 devbox：只列那台改过你的文件", async () => {
    await placed("devbox");
  }),
  settings(
    "settings-data-placed-offline",
    "设置 · 文件与数据 · 放了什么 · 断开的那台",
    "gpu-01 断开：照本机后端记着的上次那一份画（改过你的文件 · cc-monitor 的文件）＋ 警告条说多旧 ［重试］",
    async () => {
      // 本机后端记一份 gpu-01 的上次值（它连着的时候读成过一次）：经产品那条路读回本机那份成品、记成 gpu-01 的。
      const local = (await readChores([LOCAL])).machines[0]?.report;
      if (!local) throw new Error("本机那份「文件与数据」没读回来，记不了上次值");
      await callTyped(LOCAL, "last-seen-write", { origin: "gpu-01", kind: "data", value: local }, budgetWithin(10_000));
      await placed("gpu-01");
      // 那台断开 ⇒ 读回不成、警告条落下：等它出来。
      await waitFor(`${PAGE} .data-offline:not([hidden]) [role]`);
    },
    troubleWorld,
  ),
  settings("settings-ext-install", "设置 · 扩展 · 一次装到两台", "github 那一行点开：安装位置每台一行（勾选 · 现状）· 勾 devbox 与 win-laptop ⇒ 一张卡：装到哪 · token 只填一次（win-laptop 已有）· 将写入的文件逐台 ·［装到 2 台］", async () => {
    await go("ext");
    await extDrawer("mcp/github");
    for (const m of ["devbox", "win-laptop"]) {
      (await waitFor<HTMLInputElement>(`${PAGE} .ext-pick[data-machine="${m}"]`)).click();
      await waitFor(`${PAGE} .ext-pick[data-machine="${m}"]:checked`);
    }
    // 两台的「将写入的文件」都回来、［装到 2 台］按得下：那张卡读完了。
    await waitCount(`${PAGE} .ext-install .ext-install-files`, 2);
    await waitFor(`${PAGE} .ext-install [data-action=install-many]:not(:disabled)`);
    await settle();
  }, troubleWorld),
  settings("settings-ext-mcp-table", "设置 · 扩展 · MCP 小标", "筛到 MCP：github 本机的点右上角黄标（需登录）、devbox 红标（连接失败）、另两台没小标 · 图例多两项", async () => {
    await extMcp();
  }, troubleWorld),
  settings("settings-ext-mcp", "设置 · 扩展 · MCP 需登录 / 连接失败 · 抽屉", "筛到 MCP：github 本机的点右上角黄标（需登录）、devbox 红标（断开）· 图例多两项 · 点开：本机那一行「需登录 · 号 · 几点记下」（［登录…］见「需手动与登录」那一组）· devbox 那一行「会话「…」里连接失败 · 几点」＋ 原话 ＋［复制详情］", async () => {
    await extMcp();
    await extDrawer("mcp/github");
    await waitFor(`${PAGE} .ext-drawer .ext-mcp.is-fail`);
  }, troubleWorld),
  settings("settings-general-terminal-open", "设置 · 通用 · 终端下拉", "Linux 上「终端」那一行点开：自动（灰字：会挑谁）· 探到的各个 · 自定义…", async () => {
    await go("general");
    await click(await waitFor(`${PAGE} [data-role="terminal-select"]`));
    await menu();
  }),
  settings(
    "settings-general-terminal-none",
    "设置 · 通用 · 终端 · 一个都没探到",
    "这台一个终端都没探到：自动那一项灰字「未找到」",
    async () => {
      await go("general");
      await byText(PAGE, copyText("settingsPanel.terminal.autoNone"));
    },
    () => {
      const w = defaultWorld();
      w.commands = { ...w.commands, terminal_choices: () => ({ applies: true, auto: null, found: [], setting: "" }) };
      return w;
    },
  ),
  page("settings-machines", "设置 · 机器", "机器列表：本机 ＋ 三台远端，每台的连接 / 后端 / ccm / 账号四格", "machines"),
  page("settings-machine-local", "设置 · 本机", "本机那一页", "machine:<local>"),
  page("settings-machine-remote", "设置 · 远端机器", "devbox 那一页（落在第一个子页）", "machine:devbox"),
  settings("settings-machine-conn", "设置 · 远端 · 连接设置", "devbox 卡头里展开「连接设置」", async () => {
    await go("machine:devbox");
    await openConn();
  }),
  settings("settings-machine-cc", "设置 · 远端 · 这台上的 cc-monitor", "devbox 卡头里展开「这台上的 cc-monitor」", async () => {
    await go("machine:devbox");
    await openCc();
  }),
  settings("settings-machine-local-cc", "设置 · 本机 · 这台上的 cc-monitor", "本机卡头里展开「这台上的 cc-monitor」：状态 · 随退出停止 · 恢复会话的启动程序 · 恢复到 tmux 里 · 子会话 / 分身会话", async () => {
    await go("machine:<local>");
    await openCc();
  }),
  settings("settings-machine-local-cc-launcher-open", "设置 · 本机 · 启动程序下拉", "本机「这台上的 cc-monitor」里启动程序点开：默认那一家的启动器（灰字默认）· 用过的 · 自定义…", async () => {
    await go("machine:<local>");
    await openCc();
    await click(await waitFor(`${PAGE} [data-role="resume-select"]`));
    await menu();
  }),
  settings(
    "settings-machine-local-cc-exit-unreadable",
    "设置 · 本机 · 退出策略读不出",
    "本机「这台上的 cc-monitor」：随退出停止那一行下，后端写好的那一句（读的哪份 · 原因词 · 按默认）＋［复制详情］",
    async () => {
      await go("machine:<local>");
      const cc = await openCc();
      await until(() => [...cc.querySelectorAll<HTMLElement>("button")].find((b) => b.textContent === copyText("detail.act.copy")));
    },
    () => {
      const w = defaultWorld();
      // 盘上那份退出策略写坏了（写到一半断掉）：真后端读它、出「读不出」那一形与那一句。
      w.disk[LOCAL].files[".cc-monitor/backend.json"] = '{"killOnExit": tr';
      return w;
    },
  ),
  settings("settings-logs-restart", "设置 · 日志 · 改了要重启", "日志页拨「日志写入文件」：行内「重启 cc-monitor 后生效」＋ 顶上那条", async () => {
    await go("logs");
    const sw = await until(() => [...document.querySelectorAll<HTMLLabelElement>(`${PAGE} label`)].find((l) => (l.textContent ?? "").startsWith("日志写入文件")));
    await click(await until(() => sw.querySelector<HTMLButtonElement>("[role=switch]")));
    await byText(PAGE, copyText("diagnostics.file.restart"));
    await waitFor(".settings-restart-bar");
  }),
  settings("settings-machine-acct", "设置 · 远端 · 账号", "devbox 的「账号」栏：表头 · 一号一行（默认 · 5h · 7d · 按量）· 表下「共用 MCP」一行", async () => {
    await go("machine:devbox", "machine:devbox#acct");
  }, acctWorld()),
  settings("settings-acct-detail", "设置 · 账号 · 一行展开", "点 api 那一行：命令 · API key · 默认模型（仅 devbox · api）· 账号目录 · 删除", async () => {
    await go("machine:devbox", "machine:devbox#acct");
    await openAcct("api");
  }, acctWorld()),
  settings("settings-acct-no-zero", "设置 · 账号 · 清单缺默认账号", "devbox 的账号清单里没有账号 0：顶上一行提示去账号页维护里点修复", async () => {
    await go("machine:devbox", "machine:devbox#acct");
  }, () => {
    const w = acctWorld()();
    putAccounts(w.disk.devbox, ACCOUNTS.map((a) => ({ name: a.name, kind: a.authKind === "api-key" ? "api" : "sub", email: a.email || undefined, isDefault: a.isDefault, baseUrl: a.baseUrl })), { zero: false });
    return w;
  }),
  settings("settings-acct-new", "设置 · 账号 · 新建（API key）", "［新建账号］就地展开：名字 team · 选 API key", async () => {
    await go("machine:devbox", "machine:devbox#acct");
    await click(await byText(`${PAGE} button`, "新建账号"));
    const name = await waitFor<HTMLInputElement>(".acct-new input");
    name.value = "team";
    name.dispatchEvent(new Event("input"));
    await click(await byText(".acct-new label", "API key"));
    await byText(".acct-new label:has(input:checked)", "API key");
    await settle();
  }, acctWorld()),
  settings("settings-acct-remove", "设置 · 账号 · 删默认号", "work ⋯ → 删除 work…：删 / 留两行 · 之后新会话默认 personal · 焦点在取消", async () => {
    await go("machine:devbox", "machine:devbox#acct");
    await openAcct("work");
    await click(await byText('[data-account="work"] button', "删除 work…"));
    await dialog();
  }, acctWorld()),
  settings("settings-acct-mcp-go", "设置 · 账号 · 共用 MCP 指过去", "账号表下「共用 MCP：别名与配置文件」点了：切到同一台的「别名与配置文件」栏", async () => {
    await go("machine:devbox", "machine:devbox#acct");
    await click(`${PAGE} .acct-mcp-line button`);
    await shown("machine:devbox#config");
  }, acctWorld()),
  settings("settings-appearance-kb-anchor", "设置 · 外观 · 快捷键那一节（带锚点打开）", "主窗口快捷键一览的「改快捷键…」开 {page: appearance, anchor: keybindings}：落到快捷键那一节并高亮", async () => {
    await target({ page: "appearance", anchor: "keybindings" });
  }),
  settings("settings-acct-verify", "设置 · 账号 · 打开时核出对不上", "personal 的登录信息缺失：表上方一条警告条 ＋［修复…］", async () => {
    await go("machine:devbox", "machine:devbox#acct");
    await byText(`${PAGE} button`, copyText("acctPage.verify.fix"));
  }, acctWorld(true)),
  page("settings-machine-config", "设置 · 远端 · 别名与配置文件", "devbox 的「别名与配置文件」栏", "machine:devbox", "machine:devbox#config"),
  settings("settings-machine-menu", "设置 · 机器 ⋯ 菜单", "机器列表里 devbox 那一行的 ⋯", async () => {
    await machineMenu("devbox");
  }),
  settings("settings-machine-removed", "设置 · 删机器 ＋ 8 秒撤销", "⋯ →「从列表删除」：那一行当场没了，底下一条撤销", async () => {
    // 机器表读回会重画那几行（开着的 ⋯ 菜单跟着收）：点了「从列表删除」那一行还在 ⇒ 照新的那一行再开 ⋯ 再点，直到那一行没了。
    await go("machines");
    const row = '.remote-machine-row[data-page-id="machine:gpu-01"]';
    const end = performance.now() + WALL_CEILING_MS;
    while (document.querySelector(row) !== null) {
      if (performance.now() > end) throw new Error("gpu-01 那一行点了「从列表删除」还在");
      const item = [...document.querySelectorAll<HTMLElement>('[role="menu"] [role="menuitem"]')].find((e) => (e.textContent ?? "").includes("从列表删除"));
      const more = document.querySelector<HTMLElement>(`${row} button[aria-label]`);
      if (item !== undefined) await click(item);
      else if (document.querySelector('[role="menu"]') === null && more !== null) await click(more);
      await new Promise((r) => requestAnimationFrame(() => r(null)));
    }
    await waitFor('[role="status"]');
  }),
  page("settings-machine-win", "设置 · Windows 机器", "win-laptop 那一页", "machine:win-laptop"),
  page("settings-ext", "设置 · 扩展", "扩展：skill 与插件", "ext"),

  settings("settings-keybindings", "设置 · 快捷键编辑器", "外观页「快捷键」那一节点开编辑器", async () => {
    await go("appearance");
    await click(await byText(`${PAGE} [data-anchor="keybindings"] button`, copyText("settingsPanel.keybindings.open")));
    await waitFor(".kb-editor-window");
  }),
  settings("settings-machine-local-context-limits", "设置 · 本机 · 上下文上限", "本机「这台上的 cc-monitor」里展开「上下文上限」", async () => {
    await go("machine:<local>");
    await openCc();
    await openFold('[data-anchor="context-limits"]');
    await top(`${PAGE} [data-anchor="context-limits"]`);
  }),
  settings("settings-machine-local-pb-dir", "设置 · 本机 · pb 路径", "本机「这台上的 cc-monitor」里「pb 路径」那一行：设过的那个插件根目录（留空 ⇒ 用已装的插件）", async () => {
    await go("machine:<local>");
    await openCc();
    const row = await waitFor(`${PAGE} [data-anchor="pb-dir"]`);
    const input = await until(() => row.querySelector<HTMLInputElement>("input"));
    await until(() => (input.value === `${HOME}/tools/planned-build` ? input : null));
    row.scrollIntoView({ block: "center" });
  }, () => {
    const w = defaultWorld();
    putPrefs(w.disk[LOCAL], { pbDir: `${HOME}/tools/planned-build` });
    return w;
  }),
  settings("settings-machine-local-pb-dir-bad", "设置 · 本机 · pb 路径不能用", "「pb 路径」填一个底下没有 pb 的目录：那台说不能用、不存，行下是那台那一句", async () => {
    await go("machine:<local>");
    await openCc();
    const row = await waitFor(`${PAGE} [data-anchor="pb-dir"]`);
    row.scrollIntoView({ block: "center" });
    const input = await until(() => row.querySelector<HTMLInputElement>("input"));
    input.value = `${HOME}/tools`;
    input.dispatchEvent(new Event("change"));
    // 那台判了、落回那一格：标红 ＋ 行下那一句。
    await invalid(input);
  }, () => {
    // 那台起一次 pb 问能不能用（真后端）：填的是一个在的目录，底下没有 pb。
    const w = defaultWorld();
    w.disk[LOCAL].files["tools/notes.txt"] = "\n";
    return w;
  }),
  settings("settings-machines-add", "设置 · 添加机器 · 自己填", "机器页点「添加机器」→「自己填」", async () => {
    const box = await addMachine();
    await click(await byText("[role=dialog] button", "自己填"));
    await until(() => [...box.querySelectorAll<HTMLElement>(".add-machine-pane")].find((p) => !p.hidden && p.querySelector("input") !== null));
  }),
  settings("settings-port-forward", "设置 · 端口转发", "机器页点「端口转发…」：两条在跑的转发", async () => {
    await go("machines");
    await click(await waitFor(`${PAGE} > .settings-page-head button[aria-label]`));
    await menu();
    await click(await byText("[role=menuitem], button", "端口转发"));
    await waitCount(".pf-panel .pf-row", 2);
  }),
  settings("settings-ssh-import", "设置 · 添加机器 · 从 ~/.ssh/config 选", "机器页点「添加机器」：已在列表的灰着", async () => {
    await addMachine();
  }),
  settings("settings-machines-disabled", "设置 · 机器 · 停用了连接的那台", "gpu-01 关了「连接这台」：空心点 ＋「已停用」＋［连接］", async () => {
    await go("machines");
    await byText(`${PAGE} .remote-machine-row[data-page-id="machine:gpu-01"]`, copyText("machinePage.state.disabled"));
  }, disabledWorld),
  settings("settings-machine-disabled-conn", "设置 · 停用的那台 · 连接设置", "gpu-01 卡头展开「连接设置」：「连接这台」关着", async () => {
    await go("machine:gpu-01");
    await openConn();
    (await waitFor(`${PAGE} .machine-conn-switch`)).scrollIntoView({ block: "center" });
  }, disabledWorld),
  settings("settings-machines-trouble", "设置 · 机器 · 一台断开", "gpu-01 断开：机器列表里那一行的样子", async () => {
    await go("machines");
    await says("needsUpdate");
  }, troubleWorld),
  settings("settings-machine-trouble", "设置 · 断开的那台", "gpu-01 那一页（断开）", async () => {
    await go("machine:gpu-01");
  }, troubleWorld),
  settings("profiles-01-tree", "设置 · 别名 · 01 清单按「基于」排成树", "devbox「别名与配置文件」：清单按基于排成树、每行只写自己那几项；cc · cct 标终端函数", async () => {
    await profilesPage();
    await top(`${PAGE} .machine-aliases`);
  }, profilesWorld("normal"), 860),
  settings("profiles-02-merge", "设置 · 别名 · 02 点一行看合并表（宽窗）", "点 teamcct：右栏合并表（项 · 值 · 来自哪一段）· 假设在这个目录敲 · 等于", async () => {
    await profilesPage();
    await pickProfile("teamcct");
    await top(pf("[data-role=tree]"));
  }, profilesWorld("normal"), 860, 1180),
  settings("profiles-02-copied", "设置 · 别名 · 02 点「复制整条」", "点 teamcct 再点「敲 teamcct 等于」那一行右端的复制整条：整条脚本进剪贴板、出一条「已复制整条命令」", async () => {
    await profilesPage();
    await pickProfile("teamcct");
    await top(pf("[data-role=tree]"));
    await click(pf("[data-role=equals-copy]"));
    await byText('[role="status"]', copyText("profilesPage.merge.copied"));
  }, () => {
    // 这一张要看写进去之后那一下：剪贴板当作写得进（默认的台架当写不进）。
    const w = profilesWorld("normal")();
    w.commands.clipboard_write = () => null;
    return w;
  }, 860, 1180),
  settings("profiles-02-merge-narrow", "设置 · 别名 · 02 点一行看合并表（窄窗）", "窄窗：合并表落到那一行下面", async () => {
    await profilesPage();
    await pickProfile("teamcct");
    await top(pf('.prof-trow[data-name="cct"]'));
  }, profilesWorld("normal"), 860, 860),
  settings("profiles-03-new", "设置 · 别名 · 03 新建", "＋ 新增别名：名字 bcct2 · 基于 cct · 账号 team；在哪起 / 按目录灰着写继承值；等于问后端", async () => {
    await profilesPage();
    await click(await byText(pf(".prof-head button"), "新增别名"));
    await setField(pf("[data-role=profile-form] [data-role=name]"), "bcct2");
    await click(pf("[data-role=profile-form] [data-role=from]"));
    await click(await byText("[role^=menuitem]", /^cct/));
    await byText(pf("[data-role=profile-form] [data-role=from]"), /cct/);
    await pressSeg("team");
    await settle();
    await top(pf("[data-role=profile-form]"));
  }, profilesWorld("normal"), 900),
  settings("profiles-04-impact", "设置 · 别名 · 04 改父那一条先看连带谁", "改 cct 的在哪起：表单上方列出会跟着变的子、树里那几行淡黄、存旁写会连带 3 条", async () => {
    await profilesPage();
    await editProfile("cct");
    await setField(pf("[data-role=profile-form] [data-slot=tmux] select"), "fixed");
    await setField(pf("[data-role=profile-form] [data-slot=tmux] input"), "work");
    await click(await byText(pf("[data-role=impact-note] .cfg-link"), "逐条对比"));
    await waitFor(pf("[data-role=impact]"));
    await settle();
    await top(pf(".prof-notes"));
  }, profilesWorld("normal"), 1000),
  settings("profiles-05-remove", "设置 · 别名 · 05 删一条被别人基于的", "删 cct：三个选择，推荐改成基于 cc；看改前改后", async () => {
    await profilesPage();
    await click(pf('.prof-trow[data-name="cct"] .cfg-link-danger'));
    await waitFor(pf("[data-role=remove]"));
    await settle();
    await top(pf(".prof-notes"));
  }, profilesWorld("normal"), 900),
  settings("profiles-06-edited", "设置 · 别名 · 06 配置文件被手改过", "后端说上次 cc-monitor 写过之后有人改过：清单头一行小字 ＋ 看配置文件", async () => {
    await profilesPage();
    await waitFor(pf("[data-role=edited-elsewhere]"));
    await top(pf(".prof-notes"));
  }, profilesWorld("edited"), 860),
  settings("profiles-07-broken", "设置 · 别名 · 07 手改写错一段", "teamcct 写错：行上标现在不能用、摘要换成后端原话带行号；别的照常", async () => {
    await profilesPage();
    await top(pf("[data-role=tree]"));
  }, profilesWorld("broken"), 760),
  settings("profiles-07-syntax", "设置 · 别名 · 07 TOML 写坏", "整份不能用：清单换成一张卡（行号 · 原话 · 打开配置文件）", async () => {
    await profilesPage();
    await waitFor(pf("[data-role=file-problem]"));
    await top(`${PAGE} .machine-aliases`);
  }, profilesWorld("syntax"), 760),
  settings("profiles-08-stale", "设置 · 别名 · 08 保存时被别处改过", "改 teamcct 的账号存：配置文件已不是打开时那一份 ⇒ 一个字节不写、填的还在、重新读", async (ctx) => {
    await profilesPage();
    await editProfile("teamcct");
    // 换成库里最后一个号（teamcct 原来是 team，换哪个都行，只要不是它）。
    await pressSeg(PROFILE_ACCOUNTS[PROFILE_ACCOUNTS.length - 1]);
    // 打开之后、存之前，别处（手改 / 另一扇窗）改了配置文件。
    await ctx.backend.writeFile("devbox", PROFILES_FILE, `${staleBook()}\n# 别处补的一行\n`);
    const before = await waitFor(pf("[data-role=tree]"));
    await click(pf("[data-role=profile-form] [data-role=save]"));
    // 存被拒、清单照别处改过的那一份重读（树换了一棵）。
    await until(() => {
      const now = document.querySelector(pf("[data-role=tree]"));
      return now !== null && now !== before ? now : null;
    });
    await settle();
    await top(pf("[data-role=profile-form]"));
  }, profilesWorld("stale"), 860),
  settings("profiles-11-typing-push", "设置 · 别名 · 11 打字中遇推送", "开着 teamcct 的表单、名字打了一半（没交）：别处改了配置文件、后端推来一次变动 ⇒ 清单重读重画，名字那一格的字与焦点还在", async (ctx) => {
    await profilesPage();
    await editProfile("teamcct");
    const name = await waitFor<HTMLInputElement>(pf("[data-role=profile-form] [data-role=name]"));
    name.focus();
    name.value = "teamcct-打了一半";
    const before = await waitFor(pf("[data-role=tree]"));
    await ctx.backend.writeFile("devbox", PROFILES_FILE, `${staleBook()}\n# 别处补的一行\n`);
    // 推来那一次变动、清单重画（树换了一棵）。
    await until(() => {
      const now = document.querySelector(pf("[data-role=tree]"));
      return now !== null && now !== before ? now : null;
    });
    await settle();
    await top(pf("[data-role=profile-form]"));
  }, profilesWorld("stale"), 860),
  settings("profiles-09-function", "设置 · 别名 · 09 与系统命令同名的那一条", "点 cc：合并表下多一段「cc · 终端函数」：和 /usr/bin/cc 同名 · 写在哪", async () => {
    await profilesPage();
    await pickProfile("cc");
    await waitFor(pf("[data-role=function-note]"));
    await top(pf("[data-role=tree]"));
  }, profilesWorld("normal"), 860, 1180),
  settings("profiles-11-empty", "设置 · 别名 · 11 空态", "没有配置文件：首建两条的预览 · 建这两条 · 从空的开始", async () => {
    await profilesPage();
    await waitFor(pf("[data-role=seed]"));
    await top(`${PAGE} .machine-aliases`);
  }, profilesWorld("empty"), 700),
  settings("settings-machine-config-mcp", "设置 · 远端 · 共用 MCP 展开", "devbox「共用 MCP」那一行点开：两边都改的选一版 · 共用的两条 · 停止同步", async () => {
    await go("machine:devbox", "machine:devbox#config");
    const row = await waitFor(`${PAGE} [data-role=mcp-row]`);
    await click(await until(() => row.querySelector<HTMLElement>(".cfg-toggle")));
    await until(() => row.querySelector("[data-role=mcp-server], [data-role=mcp-conflict]"));
    await settle();
    row.scrollIntoView({ block: "start" });
  }),
  settings("settings-machine-config-anchor", "设置 · 带锚点打开 · 接上终端", "主窗口 ↗［接上终端］开 {machine: devbox, tab: config, anchor: connect-terminal}", async () => {
    await target({ machine: "devbox", tab: "config", anchor: "connect-terminal" });
  }),
  settings("settings-machine-name-taken", "设置 · 远端 · 改名撞名", "devbox 的名字改成 gpu-01（另一台已经叫这个）", async () => {
    await go("machine:devbox");
    const conn = await openConn();
    const name = await until(() => conn.querySelector<HTMLInputElement>('input[type="text"]'));
    name.value = "gpu-01";
    name.dispatchEvent(new Event("change"));
    await invalid(name);
  }),
  settings("settings-machine-port-range", "设置 · 远端 · 端口越界", "devbox 的端口填 70000", async () => {
    await go("machine:devbox");
    const conn = await openConn();
    const port = await until(() => conn.querySelector<HTMLInputElement>('input[type="number"]'));
    port.value = "70000";
    port.dispatchEvent(new Event("change"));
    await invalid(port);
  }),
  settings("settings-appearance-font-size-range", "设置 · 外观 · 字号越界", "基础字号填 1", async () => {
    await go("appearance");
    const size = await waitFor<HTMLInputElement>(`${PAGE} input[type="number"]`);
    size.value = "1";
    size.dispatchEvent(new Event("input"));
    size.dispatchEvent(new Event("change"));
    await invalid(size);
  }),
  settings("settings-unknown-keys", "设置 · 配置里有认不出的键", "config.json 里留着两个已经没人读的顶层键：设置窗顶上的提示条", async () => {
    await go();
    await waitFor(".settings-unknown-keys-bar");
  }, oddConfigWorld),
  settings("settings-narrow", "设置 · 窄窗", "窄于 640：导航收成内容区顶上的下拉", async () => {
    await go("general");
  }, defaultWorld, 600, 620),
  settings("settings-target", "设置 · 带目的地打开", "从别处带着目的地打开：devbox 的账号栏，高亮那一栏", async () => {
    await target({ machine: "devbox", tab: "acct" });
  }),
  {
    ...settings("settings-fresh", "设置 · 刚装好", "只有本机、没配过任何东西、各格都没测过：机器页", async () => {
      await go("machines");
    }, freshWorld),
    storage: {},
  },
];

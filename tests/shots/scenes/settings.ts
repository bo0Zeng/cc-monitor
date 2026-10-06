/**
 * 设置窗：各页、机器的各子页、各种状态。
 */
import type { Scene } from "./index";
import type { World } from "../fake/types";
import { defaultWorld } from "../fake/world";
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

function troubleWorld(): World {
  const w = defaultWorld();
  w.unseenMachines = ["gpu-01"];
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

export const SETTINGS_SCENES: Scene[] = [
  settings("settings-landing", "设置 · 打开时", "设置窗打开时落在的那一页", async () => {
    await waitFor(".settings-nav");
    await sleep(1500);
  }),
  page("settings-general", "设置 · 通用", "通用：行为 · 恢复命令 · 高级", "general"),
  page("settings-appearance", "设置 · 外观", "外观：字 · 颜色 · 快捷键", "appearance"),
  page("settings-logs", "设置 · 日志", "日志：开关、级别、日志文件", "logs"),
  page("settings-data", "设置 · 文件与数据", "文件与数据：Claude 目录 · cc-monitor 自己的东西", "data"),
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
    await click(await byText(".settings-page:not([hidden]) button", "这台上的 cc-monitor"));
    await sleep(800);
  }),
  page("settings-machine-acct", "设置 · 远端 · 账号", "devbox 的「账号」栏", "machine:devbox", "machine:devbox#acct"),
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

  settings("settings-keybindings", "设置 · 快捷键编辑器", "外观页点「打开快捷键编辑器」", async () => {
    await go("appearance");
    await click(await byText("button", "快捷键编辑器"));
    await sleep(900);
  }),
  settings("settings-general-advanced", "设置 · 通用 · 高级", "通用页展开「高级：上下文上限」", async () => {
    await go("general");
    const d = document.querySelector<HTMLDetailsElement>(".settings-page:not([hidden]) details");
    if (d) {
      d.open = true;
      d.dispatchEvent(new Event("toggle"));
      d.scrollIntoView({ block: "start" });
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
  settings("settings-machine-term-aliases", "设置 · 远端 · 终端 · 别名展开", "devbox 的「终端」子页，展开「别名」", async () => {
    await go("machine:devbox", "machine:devbox#config");
    const d = document.querySelector<HTMLDetailsElement>(".settings-page:not([hidden]) details.machine-aliases");
    if (!d) throw new Error("终端子页里没有别名那一块");
    d.open = true;
    d.dispatchEvent(new Event("toggle"));
    await sleep(1500);
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

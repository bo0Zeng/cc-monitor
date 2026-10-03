/**
 * 设置窗：各页、机器的各子页、各种状态。
 */
import type { Scene } from "./index";
import type { World } from "../fake/types";
import { defaultWorld } from "../fake/world";
import { byText, click, sleep, waitFor } from "./helpers";

function settings(id: string, title: string, desc: string, act: Scene["act"], world: () => World = defaultWorld, height = 760): Scene {
  return { id, page: "settings", dir: "设置", title, desc, width: 1100, height, world, act };
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
  page("settings-app", "设置 · 应用", "应用：总的那几项（开机自启 · 行为）", "app"),
  page("settings-appearance", "设置 · 外观", "外观：颜色、字体、字号", "app-appearance"),
  page("settings-logs", "设置 · 日志", "日志：开关、级别、日志文件", "app-logs"),
  page("settings-data", "设置 · 数据位置", "数据位置：本机与后端各存了什么、在哪", "app-data"),
  page("settings-machines", "设置 · 机器", "机器列表：本机 ＋ 三台远端，每台的连接 / 后端 / ccm / 账号四格", "machines"),
  page("settings-machine-local", "设置 · 本机", "本机那一页", "machine:（本机）"),
  page("settings-machine-remote", "设置 · 远端机器", "devbox 那一页（落在第一个子页）", "machine:devbox"),
  page("settings-machine-conn", "设置 · 远端 · 连接", "devbox 的「连接」子页：地址、端口、用户、钥匙、跳板", "machine:devbox", "machine:devbox#conn"),
  page("settings-machine-comp", "设置 · 远端 · 组件", "devbox 的「组件」子页：后端与 ccm", "machine:devbox", "machine:devbox#comp"),
  page("settings-machine-acct", "设置 · 远端 · 账号", "devbox 的「账号」子页", "machine:devbox", "machine:devbox#acct"),
  page("settings-machine-term", "设置 · 远端 · 终端", "devbox 的「终端」子页：别名、shell 接入", "machine:devbox", "machine:devbox#term"),
  page("settings-machine-footprint", "设置 · 远端 · 足迹", "devbox 的「足迹」子页：在那台机器上装了 / 写了什么", "machine:devbox", "machine:devbox#footprint"),
  page("settings-machine-win", "设置 · Windows 机器", "win-laptop 那一页", "machine:win-laptop"),
  page("settings-ext", "设置 · 扩展", "扩展：skill 与插件", "ext"),

  settings("settings-keybindings", "设置 · 快捷键编辑器", "应用页点「打开快捷键编辑器」", async () => {
    await go("app");
    await click(await byText("button", "快捷键编辑器"));
    await sleep(900);
  }),
  settings("settings-appearance-advanced", "设置 · 外观 · 高级", "外观页展开「高级：上下文上限」", async () => {
    await go("app-appearance");
    const d = document.querySelector<HTMLDetailsElement>(".settings-page:not([hidden]) details");
    if (d) {
      d.open = true;
      d.dispatchEvent(new Event("toggle"));
      d.scrollIntoView({ block: "start" });
    }
    await sleep(700);
  }),
  settings("settings-machines-add", "设置 · 添加机器", "机器页点「＋添加机器」", async () => {
    await go("machines");
    await click(await byText("button", "添加机器"));
    await sleep(900);
    // 新的那一行加在列表末尾：滚到底看它
    for (const el of document.querySelectorAll<HTMLElement>(".settings-content, .settings-page:not([hidden])")) el.scrollTop = el.scrollHeight;
    await sleep(500);
  }),
  settings("settings-port-forward", "设置 · 端口转发", "机器页点「端口转发…」：两条在跑的转发", async () => {
    await go("machines");
    await click(await byText("button", "端口转发"));
    await sleep(1200);
  }),
  settings("settings-ssh-import", "设置 · 从 ssh 配置批量导入", "机器页点「批量导入…」：按 ~/.ssh/config 里的主机分组", async () => {
    await go("machines");
    await click(await byText("button", "批量导入"));
    await sleep(1200);
  }),
  settings("settings-machines-trouble", "设置 · 机器 · 一台连不上", "gpu-01 连不上：机器列表里那一行的样子", async () => {
    await go("machines");
  }, troubleWorld),
  settings("settings-machine-trouble", "设置 · 连不上的那台", "gpu-01 那一页（连不上）", async () => {
    await go("machine:gpu-01");
  }, troubleWorld),
  settings("settings-machine-term-aliases", "设置 · 远端 · 终端 · 别名展开", "devbox 的「终端」子页，展开「别名」", async () => {
    await go("machine:devbox", "machine:devbox#term");
    const d = document.querySelector<HTMLDetailsElement>(".settings-page:not([hidden]) details.machine-aliases");
    if (!d) throw new Error("终端子页里没有别名那一块");
    d.open = true;
    d.dispatchEvent(new Event("toggle"));
    await sleep(1500);
  }),
  settings("settings-machine-name-taken", "设置 · 远端 · 改名撞名", "devbox 的名字改成 gpu-01（另一台已经叫这个）", async () => {
    await go("machine:devbox", "machine:devbox#conn");
    const name = document.querySelector<HTMLInputElement>('.settings-page:not([hidden]) input[type="text"]')!;
    name.value = "gpu-01";
    name.dispatchEvent(new Event("change"));
    await sleep(700);
  }),
  settings("settings-machine-port-range", "设置 · 远端 · 端口越界", "devbox 的端口填 70000", async () => {
    await go("machine:devbox", "machine:devbox#conn");
    const port = document.querySelector<HTMLInputElement>('.settings-page:not([hidden]) input[type="number"]')!;
    port.value = "70000";
    port.dispatchEvent(new Event("change"));
    await sleep(700);
  }),
  settings("settings-appearance-font-size-range", "设置 · 外观 · 字号越界", "基础字号填 1", async () => {
    await go("app-appearance");
    const size = document.querySelector<HTMLInputElement>('.settings-page:not([hidden]) input[type="number"]')!;
    size.value = "1";
    size.dispatchEvent(new Event("input"));
    size.dispatchEvent(new Event("change"));
    await sleep(700);
  }),
  settings("settings-machine-new-acct", "设置 · 刚加的机器 · 账号", "先看过 devbox 的账号，再「＋ 添加机器」、进新那台的「账号」栏", async () => {
    await go("machine:devbox", "machine:devbox#acct");
    await go("machines");
    await click(await byText("button", "添加机器"));
    await sleep(600);
    await go("machine:新机器", "machine:新机器#acct");
    await sleep(800);
  }),
  settings("settings-unknown-keys", "设置 · 配置里有认不出的键", "config.json 里留着两个已经没人读的顶层键：设置窗顶上的提示条", async () => {
    await waitFor(".settings-nav");
    await sleep(1500);
  }, oddConfigWorld),
  {
    ...settings("settings-fresh", "设置 · 刚装好", "只有本机、没配过任何东西、各格都没测过：机器页", async () => {
      await go("machines");
    }, freshWorld),
    storage: {},
  },
];

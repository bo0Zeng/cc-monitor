/**
 * 设置 → 机器 →「轮换」栏：规则列表 · ⋯ · 在用展开 · 批量条 · 新建 · 删除框 · 空态 · 窄窗（稿 `轮换规则.md` §5.5、截图 05）。
 * 假后端答 `rotation-rules-read`（三条规则，名字与摘要是编的）；会话标题取图集那份会话清单（`history-list`）。
 */
import { emit } from "@tauri-apps/api/event";
import type { Scene } from "./index";
import type { World } from "../fake/types";
import { defaultWorld } from "../fake/world";
import { click, sleep, waitFor } from "./helpers";

const S1 = "5e550001-0000-4000-8000-000000000001";
const S2 = "5e550002-0000-4000-8000-000000000002";
const S3 = "5e550003-0000-4000-8000-000000000003";

interface R {
  id: string;
  name: string;
  summary: string;
  live?: string[];
  ended?: string[];
  isDefault?: boolean;
  follow?: number;
}

const ROT = { order: [{ start: true }, "personal", "work"], enabled: ["personal", "work"], when: "full", atLimit: "continue", wait: 40 };

const THREE: R[] = [
  { id: "r_daily", name: "日常", summary: "personal → work → team · ≥90% · 停 · 封顶 2", live: [S1, S2], ended: [S3], isDefault: true, follow: 2 },
  { id: "r_night", name: "夜间", summary: "team → lab → work · 满 · 抢回 · work 17:00–02:00 停用", live: [S2] },
  { id: "r_saver", name: "省额度", summary: "lab → team → api · ≥80% · 单段 10" },
];

function rulesWorld(list: R[] = THREE): () => World {
  return () => {
    const w = defaultWorld();
    w.ops["rotation-rules-read"] = () => ({
      state: "present",
      reason: null,
      path: "/home/user/.cc-monitor/rotation.json",
      defaultRule: list.find((r) => r.isDefault)?.id ?? list[0].id,
      rules: list.map((r) => ({
        id: r.id,
        name: r.name,
        rotation: ROT,
        rev: 2,
        updatedAt: 0,
        isDefault: r.isDefault ?? false,
        users: { live: r.live?.length ?? 0, ended: r.ended?.length ?? 0, follow: r.follow ?? 0, sids: r.live ?? [], endedSids: r.ended ?? [] },
        summary: r.summary,
        explain: "",
        missing: [],
        atLimitApplies: false,
      })),
    });
    return w;
  };
}

function scene(id: string, title: string, desc: string, act: Scene["act"], world: () => World = rulesWorld(), width = 960, height = 620): Scene {
  return { id, page: "settings", dir: "额度与账号", title, desc, width, height, world, act };
}

/** 带目的地开到本机的「轮换」栏（窄窗时导航收起，点不着导航项 ⇒ 一律走目的地）。 */
async function goRules(): Promise<void> {
  await waitFor(".settings-nav");
  await sleep(1500);
  await emit("settings-target", JSON.stringify({ machine: "<local>", tab: "rot" }));
  await sleep(900);
  await waitFor("[data-rule]");
  await sleep(1700); // 落地那一下的高亮 1.5 秒后撤
}

const more = (id: string) => `[data-rules-more="${id}"]`;

export const RULES_SCENES: Scene[] = [
  scene("rules-list", "设置 · 轮换 · 规则列表", "本机的规则：日常（默认）· 夜间 · 省额度；摘要后端写、一行截断；在用 2 会话 / 1 会话 / —", goRules),
  scene("rules-users", "设置 · 轮换 · 在用展开", "点日常的「2 会话」：行下名单（标题取会话清单）· 已结束 1 折着 ·［全部改用 ▾］［全部转为本会话］", async () => {
    await goRules();
    await click('[data-rules-use="r_daily"]');
    await sleep(900);
  }),
  scene("rules-users-picked", "设置 · 轮换 · 在用展开 · 勾一个", "勾上一个会话：脚上「已勾 1 ·［改用 ▾］［转为本会话］」", async () => {
    await goRules();
    await click('[data-rules-use="r_daily"]');
    await sleep(900);
    await click(`[data-rules-users="r_daily"] [data-sid="${S1}"] input`);
    await sleep(400);
  }),
  scene("rules-more", "设置 · 轮换 · ⋯ 菜单", "夜间那一行的 ⋯：复制 · 改名 · 设为默认 · 复制到 · 删除", async () => {
    await goRules();
    await click(more("r_night"));
    await sleep(500);
  }),
  scene("rules-more-default", "设置 · 轮换 · 默认那条的 ⋯", "日常（默认）的 ⋯：设为默认打勾不可点 · 删除灰、第二行「默认规则 · 先设别的为默认」", async () => {
    await goRules();
    await click(more("r_daily"));
    await sleep(500);
  }),
  scene("rules-batch", "设置 · 轮换 · 批量条", "勾了夜间与省额度：表头换成「已选 2 ·［复制到 ▸］［删除］」", async () => {
    await goRules();
    await click('[data-rule="r_night"] input[type="checkbox"]');
    await sleep(200);
    await click('[data-rule="r_saver"] input[type="checkbox"]');
    await sleep(400);
  }),
  scene("rules-new", "设置 · 轮换 · 新建", "［+ 新建规则］：小浮层 名称 ＋ 从［日常 默认 ▾］·［取消］［建］", async () => {
    await goRules();
    await click("[data-rules-new]");
    await sleep(500);
  }),
  scene("rules-delete", "设置 · 轮换 · 删一条在用的", "删夜间（在用 1 会话）：确认框 改动 ＋ 单选（转为本会话 · 改为跟随默认）；焦点在取消", async () => {
    await goRules();
    await click(more("r_night"));
    await sleep(400);
    const del = [...document.querySelectorAll<HTMLButtonElement>('[role="menu"] [role^="menuitem"]')].find((b) => b.textContent?.startsWith("删除"))!;
    await click(del);
    await sleep(900);
  }),
  scene("rules-rename", "设置 · 轮换 · 行内改名", "夜间 ⋯ 改名：名称格变成输入框（Enter 存 · Esc 取消 · 失焦存）", async () => {
    await goRules();
    await click(more("r_night"));
    await sleep(400);
    const it = [...document.querySelectorAll<HTMLButtonElement>('[role="menu"] [role^="menuitem"]')].find((b) => b.textContent === "改名")!;
    await click(it);
    await sleep(500);
  }),
  scene("rules-empty", "设置 · 轮换 · 只有默认那一条", "空态：表下一行灰字「会话面板里『存为规则…』也能建」＋［新建规则］", goRules, rulesWorld([{ id: "r_daily", name: "默认", summary: "起始 · 满", isDefault: true, live: [S1], follow: 1 }])),
  scene("rules-narrow", "设置 · 轮换 · 窄窗", "设置窗 <640：摘要折到名称下一行，在用 · ⋯ 仍在行尾", goRules, rulesWorld(), 600, 620),
];

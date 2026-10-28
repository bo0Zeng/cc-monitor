/**
 * 编辑已发的一句（桌面）：悬停出「编辑」· 开框（读中）· 退法就地摆（在干活 · 一项灰着 / 空着）· 不支持 · 拒 · 发送中 · 没回执收框。
 * `session-rewind-options` 与 `session-queue-put` 由这里合成：台架上的会话不活着，真后端答不出退法；字取那台会写的那几个键。
 */
import type { Scene } from "./index";
import { Refuse, type World } from "../fake/types";
import { copyText } from "../../../src/frontend/ui/copy-table";
import { defaultWorld, LOCAL, richConvo, session } from "../fake/world";
import { click, mainReady, openTab, sleep, waitFor } from "./helpers";
import type { RewindOptions } from "../../../src/frontend/ui/generated/RewindOptions";

const W = 1280;
const H = 800;

const HOW_BOTH: RewindOptions["how"] = [
  { how: "conversation", text: copyText("beRewind.how.conversation"), default: true, enabled: true },
  { how: "code_and_conversation", text: copyText("beRewind.how.codeAndConversation"), default: false, enabled: true },
];
const HOW_NO_CODE: RewindOptions["how"] = [
  HOW_BOTH[0]!,
  { ...HOW_BOTH[1]!, enabled: false, why: copyText("beRewind.off.noCodeChange") },
];
const WHEN: RewindOptions["when"] = [
  { when: "after_turn", text: copyText("beRewind.when.afterTurn"), default: true },
  { when: "now", text: copyText("beRewind.when.now"), default: false },
];
const BUSY: RewindOptions = { how: HOW_NO_CODE, when: WHEN, said: copyText("beRewind.note.runsAfter", { n: 1 }) };
const IDLE: RewindOptions = { how: HOW_BOTH, when: [] };
const NONE: RewindOptions = { how: [], when: [], said: copyText("beRewind.no.unsupported") };

const never = (): Promise<never> => new Promise(() => {});

/** 一条会话（订单服务那一段）；退法 · 交出去那两口照 `options` / `put` 答。 */
function world(options: () => unknown = () => IDLE, put: () => unknown = never, activity: "working" | "idle" = "working"): () => World {
  return () => {
    const w = defaultWorld();
    const sid = "5ed17000-0000-4000-8000-000000000001";
    const { convo } = richConvo(sid, "/home/user/work/orders");
    w.sessions = [session(1, LOCAL, "/home/user/work/orders", convo, { sid, activity })];
    w.ops["session-rewind-options"] = options;
    w.ops["session-queue-put"] = put;
    return w;
  };
}

function scene(id: string, title: string, desc: string, act: Scene["act"], w: () => World, pointer?: string): Scene {
  return { id, page: "index", dir: "编辑已发", title, desc, width: W, height: H, world: w, act, ...(pointer ? { pointer } : {}) };
}

async function ready(): Promise<void> {
  await mainReady(1);
  await openTab(0);
  // 流开在底部：把第一句人说的那一句滚进眼前（真鼠标停在它中央，悬停才算数）。
  (await waitFor(".has-said-edit")).scrollIntoView({ block: "center" });
  await sleep(400);
}

/** 点第一句人说的那一句上的「编辑」，改几个字。 */
async function openEditor(): Promise<HTMLTextAreaElement> {
  await ready();
  await click('[data-role="said-edit"]');
  const box = await waitFor<HTMLTextAreaElement>('[data-role="said-edit-box"]');
  box.value = box.value.replace(/，顺便补测试。$/, "，测试先不补。");
  box.dispatchEvent(new Event("input"));
  await sleep(300);
  return box;
}

async function confirm(): Promise<void> {
  await openEditor();
  await waitFor('[data-role="said-edit-confirm"]:not([aria-disabled="true"])');
  await click('[data-role="said-edit-confirm"]');
  await sleep(300);
}

export const SAID_EDIT_SCENES: Scene[] = [
  scene("edit-01-hover", "悬停人说的那一句", "鼠标停在人说的气泡上：左边出「编辑」", ready, world(), ".has-said-edit"),
  scene("edit-02-loading", "开框 · 正在读退法", "点「编辑」：气泡变编辑框、原文填好、后面的卡淡掉；那台还没回退法，主按钮灰着", async () => void (await openEditor()), world(never)),
  scene("edit-03-pick-busy", "退法（在干活 · 一项灰着）", "主 agent 在干活：回退方式两项（与代码那一项灰着、原因同一行）＋ 回退时机两项 ＋ 那台那一句", async () => void (await openEditor()), world(() => BUSY)),
  scene("edit-04-pick-idle", "退法（空着）", "主 agent 空着：只有回退方式，两项都能选", async () => void (await openEditor()), world(() => IDLE, never, "idle")),
  scene("edit-05-unsupported", "这一家不支持", "回包空项：只写为什么，没有主按钮", async () => void (await openEditor()), world(() => NONE)),
  scene(
    "edit-06-refused",
    "那台拒了",
    "队满：框不收、字留着，那一句错误色写在按钮上方",
    confirm,
    world(
      () => IDLE,
      () => {
        throw new Refuse("queue_full", copyText("reason.code.overLimit"));
      },
    ),
  ),
  scene("edit-07-sending", "交了、等回执", "主按钮「发送中」灰着，选项锁住", confirm, world(() => IDLE, never)),
  scene(
    "edit-08-no-receipt",
    "没回执 · 收框",
    "那台够不到：框收起，那一句留在待发，气泡末尾一行小字指过去",
    confirm,
    world(
      () => IDLE,
      () => {
        throw new Refuse("offline", copyText("sessionState.unseen.tooltip"));
      },
    ),
  ),
];

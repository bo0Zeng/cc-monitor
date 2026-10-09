// P7a-3（#61）：标签页集合的**纯**那半 + 它写到哪个存储。
//
// ★ 最要紧的一条不是「能存能读」——存进 localStorage 它也能存能读，而那是错路
//（localStorage 住 WebView2 用户数据目录，与 cache 同处；集合名是用户手写的真相，
// 必须活过一次清缓存）。所以判据要钉**它写的是 config.json 那条路**，
// 并钉 **localStorage 一个字都没写**。
//
// 「分组不应该单独存会话记录」⇒ 组只存 `{id, name}`；组员关系改住 tab 自己身上
//（`Tab.group` ＋ `tabBar.groupOf.<sid>`）。测成员名单 / 成员上界 / 「组员满了」的格随被测的东西一起删了，
// 组员那一半的判据住 `tabs.vitest.ts`「组员关系是 tab 自己的属性」那一组。
import { describe, it, expect, vi, beforeEach } from "vitest";

const store = vi.hoisted(() => ({ cfg: {} as Record<string, unknown>, saves: 0 }));
// 写只交补丁（`patchConfig`）；按与 Rust 写口同一份金样的语义（`tests/frontend/ui/config-patch-fake.ts`）应用到 `store.cfg`。
vi.mock("../../../src/frontend/ui/config", async (orig) => {
  const actual = await orig<Record<string, unknown>>();
  const { applyConfigEdits } = await import("./config-patch-fake");
  return {
    ...actual,
    loadConfig: vi.fn(async () => store.cfg),
    patchConfig: vi.fn(async (edits: Parameters<typeof applyConfigEdits>[1]) => {
      store.cfg = JSON.parse(applyConfigEdits(JSON.stringify(store.cfg), edits)) as Record<string, unknown>;
      store.saves += 1;
    }),
  };
});

import {
  sanitizeCollections,
  createCollection,
  renameCollection,
  deleteCollection,
  getCollections,
  collectionsEdit,
  COLLECTION_CAP,
  NAME_MAX,
  createRefusal,
  collectionRefusalText,
  type TabCollection,
} from "../../../src/frontend/ui/tab-collections";
import { patchConfig } from "../../../src/frontend/ui/config";
import { groupMoveForDrop, type DropTarget, type GroupMove } from "../../../src/frontend/ui/tab-drop";
import { copyText } from "../../../src/frontend/ui/copy-table";

const c = (id: string, name: string): TabCollection => ({ id, name });

describe("P7a-3 集合：纯操作", () => {
  it("★ P7a3-Y3：四个动作齐全，且各自的空/越界口都堵住", () => {
    let l: TabCollection[] = [];
    l = createCollection(l, "  工作  ", "c1");
    expect(l).toEqual([c("c1", "工作")]); // trim
    // 空名不是一个集合。
    expect(createCollection(l, "   ", "c2")).toEqual(l);
    expect(renameCollection(l, "c1", "  ")).toEqual(l);
    l = renameCollection(l, "c1", "白天");
    expect(l[0].name).toBe("白天");
    expect(deleteCollection(l, "c1")).toEqual([]);
  });

  it("★ P7a3-Y1：盘上脏值逐种收拾干净", () => {
    expect(sanitizeCollections("nope")).toEqual([]);
    expect(sanitizeCollections([1, null, [], { name: "无 id" }, { id: "x" }])).toEqual([]);
    // 重名 id 只留第一个。
    const got = sanitizeCollections([
      { id: "a", name: "A" },
      { id: "a", name: "重复 id" },
      { id: "b", name: "B" },
    ]);
    expect(got).toEqual([c("a", "A"), c("b", "B")]);
    // 名字截断到上界。
    expect(sanitizeCollections([{ id: "x", name: "n".repeat(200) }])[0].name).toHaveLength(
      NAME_MAX,
    );
  });

  it("🔴 旧形状的 `members` 不读、不带出去（`no-legacy-compat`：不迁移不兼容）—— 每一项的键恰好是 {id, name}", () => {
    const got = sanitizeCollections([
      { id: "a", name: "A", members: ["s1", "s2"] },
      { id: "b", name: "B", extra: 1 },
    ]);
    expect(got).toEqual([c("a", "A"), c("b", "B")]);
    for (const g of got) expect(Object.keys(g).sort(), "组只存 {id, name}").toEqual(["id", "name"]);
  });

  it("★ P7a3-Y1b：**上界真的裁**（灌满再验）", () => {
    const many = Array.from({ length: COLLECTION_CAP + 10 }, (_, i) => c(`c${i}`, `n${i}`));
    expect(sanitizeCollections(many)).toHaveLength(COLLECTION_CAP);
    expect(createCollection(sanitizeCollections(many), "再来一个", "zz")).toHaveLength(
      COLLECTION_CAP,
    );
  });
});

describe("P7a-3 集合：存到哪儿", () => {
  beforeEach(() => {
    store.cfg = {};
    store.saves = 0;
    localStorage.clear();
  });

  it("★★ P7a3-Y1：写的是 `config.json` 那条路，**localStorage 一个字都不写**", async () => {
    await patchConfig([collectionsEdit([c("a", "A")])]);
    expect(store.saves, "必须真的走 patchConfig").toBe(1);
    expect(store.cfg.tabCollections).toEqual([c("a", "A")]);
    // ⚠ 这一条才是本 DoD 的正题：localStorage 是最顺手的错路（tab 偏好全在那儿），
    // 而它住 WebView2 用户数据目录 —— 清一次缓存，用户手写的集合名就没了。
    expect(localStorage.length, "集合不许落进 localStorage").toBe(0);
    expect(await getCollections()).toEqual([c("a", "A")]);
  });

  it("落盘时也过一遍 sanitize（别把脏东西写进 config.json）", async () => {
    const dirty = [
      { id: "a", name: "A", members: ["s1"] },
      { id: "a", name: "重复 id" },
    ] as unknown as TabCollection[];
    await patchConfig([collectionsEdit(dirty)]);
    expect(store.cfg.tabCollections).toEqual([c("a", "A")]);
  });

  it("config.json 里没有这个键 ⇒ 空列表，不是 undefined", async () => {
    expect(await getCollections()).toEqual([]);
  });
});

// 要求：「一条都不许静默忽略」—— 到上界时数据层照旧原样返回，
// 但得有一个判定说得出「为什么没做」，调用方据它出声。期望手写（不从被测函数生成），正反各一格。
// 「组员满了」那一种随成员上界作废；只剩组数到上界这一种。
describe("〔TL2 · E13〕到上界：为什么没做", () => {
  const many = (n: number): TabCollection[] => Array.from({ length: n }, (_, i) => c(`c${i}`, `组${i}`));

  it("建集合：满了 ⇒ collections-full；差一个 ⇒ null", () => {
    expect(createRefusal(many(COLLECTION_CAP))).toEqual({ kind: "collections-full" });
    expect(createRefusal(many(COLLECTION_CAP - 1))).toBeNull();
    // 与数据层对拍：判定说「满」的那一格，`createCollection` 恰好原样返回。
    expect(createCollection(many(COLLECTION_CAP), copyText("extPage.row.new")).length).toBe(COLLECTION_CAP);
  });

  it("那一句：带上界数、零占位符残留", () => {
    const a = collectionRefusalText({ kind: "collections-full" });
    expect(a.title).toBe(copyText("tabCollections.full.collectionsTitle"));
    expect(a.body).toContain(String(COLLECTION_CAP));
    for (const t of [a.title, a.body]) {
      expect(t).not.toMatch(/[{〔]/);
    }
  });
});

// 拖放对组的后果（`tab-drop.ts::groupMoveForDrop`）—— 手写表，期望不从被测函数生成。
describe("〔GRP1〕拖放：归属跟着落点宿主走（`§D.7`），只回「被拖那个 tab 怎么动」", () => {
  // a 在 g1；b 在 g1；x 在 g2；s、t 散着。
  const groups: Record<string, string | null> = { a: "g1", b: "g1", x: "g2", s: null, t: null };
  const groupOf = (sid: string): string | null => groups[sid] ?? null;
  const cases: [string, string, DropTarget, GroupMove][] = [
    ["压在散 tab 上 ⇒ 现建", "s", { kind: "onto", sid: "t" }, { kind: "found", with: "t" }],
    ["组里的压在散 tab 上 ⇒ 现建（离开原组）", "a", { kind: "onto", sid: "t" }, { kind: "found", with: "t" }],
    ["散 tab 压在组里的上 ⇒ 进那个组", "s", { kind: "onto", sid: "a" }, { kind: "join", gid: "g1" }],
    ["压在别组的上 ⇒ 换组", "a", { kind: "onto", sid: "x" }, { kind: "join", gid: "g2" }],
    ["压在同组的上 ⇒ 不变", "a", { kind: "onto", sid: "b" }, { kind: "stay" }],
    ["压在自己身上 ⇒ 不变", "s", { kind: "onto", sid: "s" }, { kind: "stay" }],
    ["插到组里的前面 ⇒ 进那个组", "s", { kind: "insert", at: { sid: "a", side: "before" }, gid: "g1" }, { kind: "join", gid: "g1" }],
    ["插到散 tab 前面 ⇒ 拖出组", "a", { kind: "insert", at: { sid: "s", side: "before" }, gid: null }, { kind: "leave" }],
    ["散 tab 插到散 tab 前面 ⇒ 不变", "s", { kind: "insert", at: { sid: "t", side: "before" }, gid: null }, { kind: "stay" }],
    ["同组里挪位置 ⇒ 不变", "a", { kind: "insert", at: { sid: "b", side: "before" }, gid: "g1" }, { kind: "stay" }],
    ["落到末尾 ⇒ 拖出组", "a", { kind: "insert", at: null, gid: null }, { kind: "leave" }],
    ["散 tab 落到末尾 ⇒ 不变", "s", { kind: "insert", at: null, gid: null }, { kind: "stay" }],
  ];
  it.each(cases)("%s", (_label, sid, target, want) => {
    expect(groupMoveForDrop(groupOf, [sid], target)).toEqual(want);
  });
});

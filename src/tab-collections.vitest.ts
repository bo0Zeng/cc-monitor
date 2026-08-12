// P7a-3（#61）：标签页集合的**纯**那半 + 它写到哪个存储。
//
// ★ 最要紧的一条不是「能存能读」——存进 localStorage 它也能存能读，而那是错路
//（localStorage 住 WebView2 用户数据目录，与 cache 同处；集合名是用户手写的真相，
// 必须活过一次清缓存）。所以判据要钉**它写的是 config.json 那条路**，
// 并钉 **localStorage 一个字都没写**。
import { describe, it, expect, vi, beforeEach } from "vitest";

const store = { cfg: {} as Record<string, unknown>, saves: 0 };
vi.mock("./config", () => ({
  loadConfig: vi.fn(async () => store.cfg),
  saveConfig: vi.fn(async (c: Record<string, unknown>) => {
    store.cfg = c;
    store.saves += 1;
  }),
}));

import {
  sanitizeCollections,
  createCollection,
  renameCollection,
  deleteCollection,
  addMember,
  removeMember,
  collectionOf,
  getCollections,
  setCollections,
  COLLECTION_CAP,
  MEMBER_CAP,
  NAME_MAX,
  type TabCollection,
} from "./tab-collections";

const c = (id: string, name: string, members: string[] = []): TabCollection => ({
  id,
  name,
  members,
});

describe("P7a-3 集合：纯操作", () => {
  it("★ P7a3-Y3：五个动作齐全，且各自的空/越界口都堵住", () => {
    let l: TabCollection[] = [];
    l = createCollection(l, "  工作  ", "c1");
    expect(l).toEqual([c("c1", "工作")]); // trim
    // 空名不是一个集合。
    expect(createCollection(l, "   ", "c2")).toEqual(l);
    expect(renameCollection(l, "c1", "  ")).toEqual(l);
    l = renameCollection(l, "c1", "白天");
    expect(l[0].name).toBe("白天");
    l = addMember(l, "c1", "s1");
    l = addMember(l, "c1", "s1"); // 幂等
    expect(l[0].members).toEqual(["s1"]);
    l = removeMember(l, "s1");
    expect(l[0].members).toEqual([]);
    expect(deleteCollection(l, "c1")).toEqual([]);
  });

  it("★ P7a3-Y3b：一个 tab 只属一个集合 —— 加进新的会从旧的移出", () => {
    let l = [c("a", "A", ["s1"]), c("b", "B")];
    l = addMember(l, "b", "s1");
    expect(l[0].members).toEqual([]);
    expect(l[1].members).toEqual(["s1"]);
    expect(collectionOf(l, "s1")?.id).toBe("b");
    expect(collectionOf(l, "s9")).toBeNull();
  });

  it("★ P7a3-Y1：盘上脏值逐种收拾干净", () => {
    expect(sanitizeCollections("nope")).toEqual([]);
    expect(sanitizeCollections([1, null, [], { name: "无 id" }, { id: "x" }])).toEqual([]);
    // 重名 id 只留第一个；成员去重；一个 sid 只准属一个集合（后来的丢掉）。
    const got = sanitizeCollections([
      { id: "a", name: "A", members: ["s1", "s1", 7, "  ", "s2"] },
      { id: "a", name: "重复 id" },
      { id: "b", name: "B", members: ["s2", "s3"] },
    ]);
    expect(got).toEqual([c("a", "A", ["s1", "s2"]), c("b", "B", ["s3"])]);
    // 名字截断到上界。
    expect(sanitizeCollections([{ id: "x", name: "n".repeat(200) }])[0].name).toHaveLength(
      NAME_MAX,
    );
  });

  it("★ P7a3-Y1b：**上界真的裁**（灌满再验）", () => {
    const many = Array.from({ length: COLLECTION_CAP + 10 }, (_, i) => c(`c${i}`, `n${i}`));
    expect(sanitizeCollections(many)).toHaveLength(COLLECTION_CAP);
    expect(createCollection(sanitizeCollections(many), "再来一个", "zz")).toHaveLength(
      COLLECTION_CAP,
    );
    const big = [c("a", "A", Array.from({ length: MEMBER_CAP + 10 }, (_, i) => `s${i}`))];
    expect(sanitizeCollections(big)[0].members).toHaveLength(MEMBER_CAP);
  });
});

describe("P7a-3 集合：存到哪儿", () => {
  beforeEach(() => {
    store.cfg = {};
    store.saves = 0;
    localStorage.clear();
  });

  it("★★ P7a3-Y1：写的是 `config.json` 那条路，**localStorage 一个字都不写**", async () => {
    await setCollections([c("a", "A", ["s1"])]);
    expect(store.saves, "必须真的走 saveConfig").toBe(1);
    expect(store.cfg.tabCollections).toEqual([c("a", "A", ["s1"])]);
    // ⚠ 这一条才是本 DoD 的正题：localStorage 是最顺手的错路（tab 偏好全在那儿），
    // 而它住 WebView2 用户数据目录 —— 清一次缓存，用户手写的集合名就没了。
    expect(localStorage.length, "集合不许落进 localStorage").toBe(0);
    expect(await getCollections()).toEqual([c("a", "A", ["s1"])]);
  });

  it("落盘时也过一遍 sanitize（别把脏东西写进 config.json）", async () => {
    await setCollections([c("a", "A", ["s1", "s1"]), c("a", "重复 id")]);
    expect(store.cfg.tabCollections).toEqual([c("a", "A", ["s1"])]);
  });

  it("config.json 里没有这个键 ⇒ 空列表，不是 undefined", async () => {
    expect(await getCollections()).toEqual([]);
  });
});

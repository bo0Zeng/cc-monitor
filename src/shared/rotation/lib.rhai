// 轮换策略的积木库（随后端带、只读；策略里 `lib::xxx(…)` 调）。只算、不决定：该用哪个号、什么时候换，是策略自己的代码。
//
// 这几个函数认的设置写法（策略想用就照它写，不用也行）：
//   #{ caps: #{ "5h": [#{from, to, v}], "7d": [...] },   // 总体默认上限，一天里的分钟分段，落空 ＝ 100
//      capKind: "soft" | "hard",                          // 总体默认上限类型
//      accounts: [ #{ start?: true, id?: "lab", caps?, capKind?, ... } ] }   // 号列表；先后就是同权时的先后

// 某组分段在某个钟点（一天里第几分钟）的上限；落空 ＝ 100。
fn cap_at(segs, clock) {
  if segs == () { return 100.0; }
  for s in segs {
    if s.from <= clock && clock < s.to { return s.v.to_float(); }
  }
  100.0
}

// 某组分段从此刻起到下一条边界还有几分钟（没有分段 ⇒ ()）。边界两边上限一样的不算（一整天一段的，零点不是边界）。
fn next_edge(segs, clock) {
  if segs == () { return (); }
  let best = ();
  for s in segs {
    for e in [s.from, s.to] {
      let e = e % 1440;
      let d = (e + 1440 - clock) % 1440;
      if d == 0 { continue; }
      if cap_at(segs, (e + 1439) % 1440) == cap_at(segs, e) { continue; }
      if best == () || d < best { best = d; }
    }
  }
  best
}

// 这个号某窗的分段（号另设 → 总体）。r ＝ 它在号列表里那一行。
fn segs_of(s, r, w) {
  if r != () && r.caps != () && r.caps[w] != () { return r.caps[w]; }
  if s.caps != () { return s.caps[w]; }
  ()
}

// 上限类型（号另设 → 总体 → 软）。
fn kind_of(s, r) {
  if r != () && r.capKind != () { return r.capKind; }
  if s.capKind != () { s.capKind } else { "soft" }
}

// 此刻的上限；lift（放开软上限）且是软上限 ⇒ 100。
fn cap_of(s, r, w, lift, clock) {
  if lift && kind_of(s, r) == "soft" { return 100.0; }
  cap_at(segs_of(s, r, w), clock)
}

// 余量：两窗「上限 − 用量」取小。
fn left_of(s, r, a, lift, clock) {
  let l5 = cap_of(s, r, "5h", lift, clock) - a.used.w5h;
  let l7 = cap_of(s, r, "7d", lift, clock) - a.used.w7d;
  if l5 < l7 { l5 } else { l7 }
}

// 有额度：事实上发得出去，且（订阅号）没到此刻的上限。
fn has_quota(s, r, a, lift, clock) {
  if a == () || !a.sendable { return false; }
  if a.kind == "api" { return true; }
  left_of(s, r, a, lift, clock) > 0.0
}

// 按分排先后：每次在剩下的号里找分最高的，比它低、但较高的超出它不到 close% 的都算同权，取 ids 里最靠前的。
// pri：#{号: 数}，没给 ⇒ 0；close 不给 ⇒ 0（只有分一样才同权）。
fn rank(ids, pri, close) {
  let c = if close != () { close.to_float() } else { 0.0 };
  let rest = [];
  for id in ids { rest.push(id); }
  let out = [];
  while rest.len() > 0 {
    let best = ();
    for id in rest {
      let p = if pri[id] != () { pri[id].to_float() } else { 0.0 };
      if best == () || p > best { best = p; }
    }
    let floor = if best > 0.0 { best / (1.0 + c / 100.0) } else { best };
    let i = 0;
    while i < rest.len() {
      let p = if pri[rest[i]] != () { pri[rest[i]].to_float() } else { 0.0 };
      if p >= floor { break; }
      i += 1;
    }
    out.push(rest[i]);
    rest.remove(i);
  }
  out
}

// 启用延迟的起点：条件（例如「其余的号都没额度」）此刻成立 ⇒ 上一判记的起点（没有 ⇒ 此刻）；不成立 ⇒ ()。起点由策略存在 memo 里带回来。
fn delay_start(prev, holds, now) {
  if !holds { return (); }
  if prev != () { prev } else { now }
}

// 起点 + 延迟（分钟）到点没有：没起点 ⇒ false。
fn delay_over(start, minutes, now) {
  if start == () { return false; }
  let d = if minutes != () { minutes } else { 0 };
  now >= start + d * 60
}

// 一个都没有时停到几点：ids 里最早的恢复时刻（被拒到期 · 到上限那一窗重置）与 extra（分段边界、启用延迟到点……）里最早的；没有 ⇒ ()。
// rows · accts：#{号: 号列表那一行} · #{号: 事实}。
fn hold_from(s, rows, accts, ids, now, clock, extra) {
  let best = ();
  let cands = [];
  for t in extra { cands.push(t); }
  for id in ids {
    let a = accts[id];
    if a == () { continue; }
    let r = rows[id];
    cands.push(a.refusedUntil);
    if a.kind != "api" {
      if a.used.w5h >= cap_of(s, r, "5h", false, clock) { cands.push(a.resets.w5h); }
      if a.used.w7d >= cap_of(s, r, "7d", false, clock) { cands.push(a.resets.w7d); }
    }
  }
  for t in cands {
    if t != () && t > now && (best == () || t < best) { best = t; }
  }
  best
}

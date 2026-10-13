// 轮换策略的积木库（随后端带、只读；策略里 `lib::xxx(…)` 调）。只算这一条要的东西（`轮换-v5-分层.md` §2.5）：
// 号牌分成上下几排 —— 同一排里不抢（在用的号用满了才换到这一排里排第一的、又有额度的号），上排的号一有额度就换上去，
// 下排只在上面所有排都没额度时才轮到。上限（分时段 · 跟总体）· 软硬 · 启用延迟 · 一个都没有时停到几点，也在这里。
//
// 设置的形状（`by_rows` 认的那一套）：
//   #{ caps: #{ "5h": [#{from, to, v}], "7d": [...] },   // 总体默认上限，分钟分段，落空 ＝ 100
//      capKind: "soft" | "hard",                          // 总体默认上限类型
//      accounts: [ #{ start?: true, id?: "lab", row?: 0, caps?, capKind?, delay?: 分钟 } ] }

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

fn kind_of(s, r) {
  if r != () && r.capKind != () { return r.capKind; }
  if s.capKind != () { s.capKind } else { "soft" }
}

// 此刻的上限；放开软上限（lift）且是软上限 ⇒ 100。
fn cap_of(s, r, w, lift, clock) {
  if lift && kind_of(s, r) == "soft" { return 100.0; }
  cap_at(segs_of(s, r, w), clock)
}

// 余量：两窗上限减用量取小。
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

// 按分降序、同分照列表先后。scores：#{号: 数}，没给 ⇒ 0。
fn rank(ids, scores) {
  let rows = [];
  let i = 0;
  for id in ids {
    let sc = scores[id];
    if sc == () { sc = 0.0; }
    rows.push(#{ id: id, s: sc.to_float(), i: i });
    i += 1;
  }
  rows.sort(|a, b| if a.s > b.s { -1 } else if a.s < b.s { 1 } else { a.i - b.i });
  rows.map(|r| r.id)
}

// ★ 号牌分排那一条。scores 只管同一排里谁先（没给 ⇒ 照号牌左右）。
// 回 #{order, why, hold_until?, wake: #{at?, when}, memo}：order 第一个就是此刻要用的号（在用的号排第一 ＝ 不换）。
fn by_rows(ctx, scores) {
  let s = ctx.settings;
  let now = ctx.now;
  let clock = ctx.clock;
  // 一次走完 ctx 建好索引，往下不再把整份 ctx 传进小函数（Rhai 传参照值拷）。
  let a_of = #{};
  for a in ctx.accounts { a_of[a.id] = a; }
  let ids = [];
  let r_of = #{};
  let row_of = #{};
  let rows = [];
  for r in s.accounts {
    let id = if r.start == true { ctx.start } else { r.id };
    if id == () || ids.contains(id) || a_of[id] == () { continue; }
    ids.push(id);
    r_of[id] = r;
    let k = if r.row != () { r.row } else { 0 };
    row_of[id] = k;
    if !rows.contains(k) { rows.push(k); }
  }
  rows.sort();
  // 有额度：先照上限看；一个都没有 ⇒ 软上限的号当用满为止再看。
  let ok = #{};
  let lift = true;
  for id in ids {
    ok[id] = has_quota(s, r_of[id], a_of[id], false, clock);
    if ok[id] { lift = false; }
  }
  if lift {
    for id in ids { ok[id] = has_quota(s, r_of[id], a_of[id], true, clock); }
  }
  // 每一排照分排好（同分照号牌左右）。
  let in_row = #{};
  for k in rows {
    let m = [];
    for id in ids { if row_of[id] == k { m.push(id); } }
    in_row[k.to_string()] = rank(m, scores);
  }
  // 启用延迟：上面的排全都没额度满 D 分钟，才换到它（最上一排上面没有排 ⇒ 不等）。起点记在 memo。
  let old = if ctx.memo != () && ctx.memo.since != () { ctx.memo.since } else { #{} };
  let since = #{};
  let ready = #{};
  let above_out = true;
  let first_row = true;
  for k in rows {
    for id in in_row[k.to_string()] {
      let r = r_of[id];
      let d = if r.delay != () { r.delay * 60 } else { 0 };
      if first_row || d == 0 {
        ready[id] = true;
      } else if above_out {
        let st = if old[id] != () { old[id] } else { now };
        since[id] = st;
        ready[id] = now - st >= d;
      } else {
        ready[id] = false;
      }
    }
    for id in in_row[k.to_string()] { if ok[id] { above_out = false; } }
    first_row = false;
  }
  // 排的先后（排 ＋ 排里的先后），只留有额度又过了延迟的号。
  let c = ctx.current;
  let line = [];
  for k in rows {
    for id in in_row[k.to_string()] { if ok[id] && ready[id] { line.push(id); } }
  }
  let order = [];
  let why = #{};
  if ids.contains(c) && ok[c] {
    // 在用的号有额度：上面几排有号能用 ⇒ 换到最上那一排排第一的；否则不换。
    let up = ();
    if line.len() > 0 && row_of[line[0]] < row_of[c] { up = line[0]; }
    if up != () { order.push(up); why[up] = "上面一排的号有额度了"; }
    order.push(c);
    if up == () { why[c] = "在用，还有额度"; }
  }
  for id in line { if !order.contains(id) { order.push(id); } }
  for id in ids {
    if why[id] != () { continue; }
    let a = a_of[id];
    why[id] = if a == () || !a.sendable { "此刻发不出去" }
      else if !ok[id] { "到了用量上限" }
      else if !ready[id] { "还在启用延迟里" }
      else { "有额度" };
  }
  // 叫醒：分段边界 · 延迟到点 · 有额度的号读数到上限。
  let at = ();
  let when = [];
  for id in ids {
    let r = r_of[id];
    for w in ["5h", "7d"] {
      let e = next_edge(segs_of(s, r, w), clock);
      if e != () {
        let t = now - now % 60 + e * 60;
        if at == () || t < at { at = t; }
      }
      if ok[id] && a_of[id].kind != "api" {
        when.push(#{ acct: id, w: w, used_at_least: cap_of(s, r, w, lift, clock) });
      }
    }
    if since[id] != () {
      let t = since[id] + r.delay * 60;
      if t > now && (at == () || t < at) { at = t; }
    }
  }
  let out = #{ order: order, why: why, wake: #{ at: at, when: when }, memo: #{ since: since } };
  // 一个都没有：最早恢复的那一刻（被拒到期 · 窗口重置 · 分段边界 · 延迟到点）。
  if order.len() == 0 {
    let best = ();
    for id in ids {
      let a = a_of[id];
      let cands = [a.refusedUntil, a.resets.w5h, a.resets.w7d];
      if since[id] != () { cands.push(since[id] + r_of[id].delay * 60); }
      for t in cands { if t != () && t > now && (best == () || t < best) { best = t; } }
    }
    if at != () && (best == () || at < best) { best = at; }
    out.hold_until = best;
  }
  out
}

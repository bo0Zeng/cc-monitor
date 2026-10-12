// 轮换策略的积木库（随包、只读；策略里 `lib::xxx(…)` 调）。只算不决定：上限 · 余量 · 分段边界 · 启用延迟 · 回切 ·
// 都到上限时软硬怎么办，这几样「随包三份共用的想法」写在这里一处，后端不认它们。
//
// 设置的形状（随包三份同一套）：
//   #{ caps: #{ "5h": [#{from, to, v}], "7d": [...] },   // 默认上限，分钟分段，落空 ＝ 100
//      capKind: "soft" | "hard",
//      accounts: [ #{ start?: true, id?: "lab", caps?, capKind?, delay?: 分钟, back?: % } ],
//      … 各份自己的几项 }

// 某组分段在某个钟点（一天里第几分钟）的上限；落空 ＝ 100。
fn cap_at(segs, clock) {
  if segs == () { return 100.0; }
  for s in segs {
    if s.from <= clock && clock < s.to { return s.v.to_float(); }
  }
  100.0
}

// 某组分段从此刻起到下一条边界还有几分钟（没有分段 ⇒ ()）。
fn next_edge(segs, clock) {
  if segs == () { return (); }
  let best = ();
  for s in segs {
    for e in [s.from, s.to] {
      let e = e % 1440;
      let d = (e + 1440 - clock) % 1440;
      if d == 0 { continue; }
      if best == () || d < best { best = d; }
    }
  }
  best
}

// 号列表里这个号那一行（`start` 那一行认开会话的号）。
fn row(s, ctx, id) {
  for r in s.accounts {
    if (r.start == true && id == ctx.start) || r.id == id { return r; }
  }
  ()
}

// 号列表（照先后，`start` 换成开会话的号；只留此刻事实里有的号）。
fn list(s, ctx) {
  let out = [];
  for r in s.accounts {
    let id = if r.start == true { ctx.start } else { r.id };
    if id == () || out.contains(id) { continue; }
    for a in ctx.accounts { if a.id == id { out.push(id); break; } }
  }
  out
}

fn acct(ctx, id) {
  for a in ctx.accounts { if a.id == id { return a; } }
  ()
}

// 这个号某窗的分段（号另设 → 默认）。
fn segs(s, ctx, id, w) {
  let r = row(s, ctx, id);
  if r != () && r.caps != () && r.caps[w] != () { return r.caps[w]; }
  if s.caps != () { return s.caps[w]; }
  ()
}

fn kind(s, ctx, id) {
  let r = row(s, ctx, id);
  if r != () && r.capKind != () { return r.capKind; }
  if s.capKind != () { s.capKind } else { "soft" }
}

// 此刻的上限；放开软上限（lift）且是软上限 ⇒ 100。
fn cap(s, ctx, id, w, lift) {
  if lift && kind(s, ctx, id) == "soft" { return 100.0; }
  cap_at(segs(s, ctx, id, w), ctx.clock)
}

// 余量：两窗上限减用量取小。
fn left(s, ctx, id, lift) {
  let a = acct(ctx, id);
  let l5 = cap(s, ctx, id, "5h", lift) - a.used.w5h;
  let l7 = cap(s, ctx, id, "7d", lift) - a.used.w7d;
  if l5 < l7 { l5 } else { l7 }
}

// 照这份设置此刻能不能用：事实上发得出去，且（订阅号）还没到上限。
fn usable(s, ctx, id, lift) {
  let a = acct(ctx, id);
  if a == () || !a.sendable { return false; }
  if a.kind == "api" { return true; }
  left(s, ctx, id, lift) > 0.0
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

// ★ 随包三份共用的那一判：照设置里的上限 · 软硬 · 启用延迟 · 回切，把分（scores）排成 order，约好叫醒。
// 在用的号还能用、又没有该切回的 ⇒ 它排第一（后端照 order 第一个用，排第一就是不换）。
// 回 #{order, why, hold_until?, wake: #{at?, when}, memo}。
fn decide_with(ctx, scores) {
  let s = ctx.settings;
  let ids = list(s, ctx);
  let now = ctx.now;
  // 一个号都用不了 ⇒ 放开软上限再看。
  let lift = true;
  for id in ids { if usable(s, ctx, id, false) { lift = false; break; } }
  let k = rank(ids, scores);
  // 启用延迟：排在它前面的号全都用不了满 D 分钟才放它（排第一的前面没有号 ⇒ 恒放）。起点记在 memo。
  let old = if ctx.memo != () && ctx.memo.since != () { ctx.memo.since } else { #{} };
  let since = #{};
  let allowed = #{};
  let all_bad = true;
  let i = 0;
  for id in k {
    let r = row(s, ctx, id);
    let d = if r != () && r.delay != () { r.delay * 60 } else { 0 };
    if i == 0 || d == 0 {
      allowed[id] = true;
    } else if all_bad {
      let st = if old[id] != () { old[id] } else { now };
      since[id] = st;
      allowed[id] = now - st >= d;
    } else {
      allowed[id] = false;
    }
    if usable(s, ctx, id, lift) { all_bad = false; }
    i += 1;
  }
  let c = ctx.current;
  let order = [];
  let why = #{};
  if ids.contains(c) && usable(s, ctx, c, lift) {
    // 回切：排在在用号前面、余量回到阈值以上的那个（只在没放开软上限时）。
    let back = ();
    if !lift {
      for id in k {
        if id == c { break; }
        let r = row(s, ctx, id);
        if r != () && r.back != () && (usable(s, ctx, id, lift) && allowed[id]) && left(s, ctx, id, false) >= r.back.to_float() { back = id; break; }
      }
    }
    if back != () { order.push(back); why[back] = "余量回到 " + row(s, ctx, back).back + "% 以上，切回"; }
    order.push(c);
    if back == () { why[c] = "在用，还能用就不换"; }
  }
  for id in k {
    if !order.contains(id) && (usable(s, ctx, id, lift) && allowed[id]) { order.push(id); }
  }
  for id in ids {
    if why[id] != () { continue; }
    let a = acct(ctx, id);
    why[id] = if a == () || !a.sendable { "此刻发不出去" }
      else if !usable(s, ctx, id, lift) { "到了用量上限" }
      else if !allowed[id] { "还在启用延迟里" }
      else { "可用" };
  }
  // 叫醒：分段边界 · 延迟到点 · 能用的号读数到上限。
  let at = ();
  let when = [];
  for id in ids {
    for w in ["5h", "7d"] {
      let e = next_edge(segs(s, ctx, id, w), ctx.clock);
      if e != () {
        let t = now - now % 60 + e * 60;
        if at == () || t < at { at = t; }
      }
      if usable(s, ctx, id, lift) && acct(ctx, id).kind != "api" {
        when.push(#{ acct: id, w: w, used_at_least: cap(s, ctx, id, w, lift) });
      }
    }
    if since[id] != () {
      let r = row(s, ctx, id);
      let t = since[id] + r.delay * 60;
      if t > now && (at == () || t < at) { at = t; }
    }
  }
  let out = #{ order: order, why: why, wake: #{ at: at, when: when }, memo: #{ since: since } };
  // 一个都用不了：最早恢复的那一刻（被拒到期 · 窗口重置 · 分段边界 · 延迟到点）。
  if order.len() == 0 {
    let best = ();
    for id in ids {
      let a = acct(ctx, id);
      let cands = [a.refusedUntil, a.resets.w5h, a.resets.w7d];
      if since[id] != () { cands.push(since[id] + row(s, ctx, id).delay * 60); }
      for t in cands { if t != () && t > now && (best == () || t < best) { best = t; } }
    }
    if at != () && (best == () || at < best) { best = at; }
    out.hold_until = best;
  }
  out
}

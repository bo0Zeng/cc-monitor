// 轮换策略的积木库（随后端带、只读；策略里 `lib::xxx(…)` 调）。只算、不决定：该用哪个号、什么时候换，是策略自己的代码。
//
// 这几个函数认的设置写法（策略想用就照它写，不用也行）：
//   #{ caps: #{ "5h": [#{from, to, v}], "7d": [...] },   // 总体默认上限，一天里的分钟分段，落空 ＝ 100
//      capKind: "soft" | "hard",                          // 总体默认上限类型
//      accounts: [ #{ start?: true, id?: "lab", caps?, capKind?, ... } ] }   // 号列表；先后就是同权时的先后

// 某组分段在某个钟点（一天里第几分钟）的上限；落空 ＝ 100。
fn cap_at(segs, clock) { cap_or(segs, clock, 100.0) }

// 同 cap_at，落空（没有分段 · 这个钟点不在哪一段里）⇒ dflt。
fn cap_or(segs, clock, dflt) {
  if segs == () { return dflt; }
  for s in segs {
    if s.from <= clock && clock < s.to { return s.v.to_float(); }
  }
  dflt
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

// ───────────── 智能切换用的几样（只算、不决定）─────────────
// 单位：额度是百分点；用量与代价折成「折合输入 token」（号的 tokensPerPct ＝ 1% 约多少，跨号比时乘它）；时刻是秒，用速按分。

// 一窗此刻的用量（w ＝ "w5h" | "w7d"）：读数缺 ⇒ 0；过了重置时刻 ⇒ 0。
fn used_of(a, w, now) {
  let u = a.used[w];
  let r = a.resets[w];
  if u == () || (r != () && r <= now) { 0.0 } else { u.to_float() }
}

// 一个号此刻那一份额度与它什么时候作废（c5 · c7 ＝ 两窗此刻的上限）：
//   amt ＝ 余量（有周上限 ⇒ 两窗「上限 − 用量」取小，没有 ⇒ 只看 5 小时窗）· l7 ＝ 周窗剩的（没有周上限 ⇒ ()）· open ＝ 5 小时窗开着；
//   wk ＝ 周上限卡着（周剩的不够它 5 小时窗一路用到周重置）⇒ 5 小时窗剩的不作废，作废看周重置；
//   exp ＝ 作废的时刻：周上限卡着 ⇒ 周重置；只有 5 小时卡着 ⇒ 5 小时重置；没开窗 ⇒ ()（用了才开，开了 5 小时后才作废）。
fn piece(a, c5, c7, now) {
  let l5 = c5 - used_of(a, "w5h", now);
  let r5 = a.resets.w5h;
  let open = r5 != () && r5 > now;
  let e5 = if open { r5 } else { () };
  if a.weekly != true { return #{ amt: l5, exp: e5, wk: false, open: open, l7: () }; }
  let l7 = c7 - used_of(a, "w7d", now);
  let r7 = a.resets.w7d;
  let on7 = r7 != () && r7 > now;
  let wins = if on7 { (r7 - now + 17999) / 18000 } else { 1 };
  let amt = if l7 < l5 { l7 } else { l5 };
  if l7 < c5 * wins.to_float() {
    return #{ amt: amt, exp: if on7 { r7 } else { now + 7 * 86400 }, wk: true, open: open, l7: l7 };
  }
  #{ amt: amt, exp: e5, wk: false, open: open, l7: l7 }
}

// 要换号时「先用最先作废的」那个先后的键（越小越先）：只有 5 小时卡着、扣掉别的会话到作废前会用掉的（others，点 / 分；说不出 ⇒ 0）
//   还有剩 ⇒ 离作废还有几秒；没开窗 ⇒ 第二档；周上限卡着 · 别的会话会用完的 ⇒ 最后。同一档的先后由调用方再照号牌排。
fn due_rank(p, others, now) {
  if p.exp == () { return 1000000000; }
  if p.wk { return 3000000000; }
  let o = if others != () { others } else { 0.0 };
  if p.amt - o * (p.exp - now).to_float() / 60.0 > 0.0 { p.exp - now } else { 3000000000 }
}

// 写缓存比读缓存多的倍数（本会话那一档）；说不出 ⇒ 1.9（1 小时档：写 2 倍、读 0.1 倍）。
fn write_extra(sess) {
  if sess.writeExtra != () { sess.writeExtra.to_float() } else { 1.9 }
}

// 换到号 id 当场多花的（折合输入 token）：本会话在它上面的缓存还在 ⇒ 只补写缓存之后新长的那段，否则整段上下文重写一遍。
fn switch_cost(sess, id, now) {
  let full = if sess.context != () { sess.context.to_float() } else { 0.0 };
  let c = if sess.cached != () { sess.cached[id] } else { () };
  if c != () && c["until"] != () && c["until"] > now && c.tokens != () {
    let miss = full - c.tokens.to_float();
    return write_extra(sess) * (if miss > 0.0 { miss } else { 0.0 });
  }
  write_extra(sess) * full
}

// 本会话在一份额度作废前用得掉多少（折合输入 token）：free ＝ 那份扣掉别人会用掉的还剩几点 · pace ＝ 本会话用速（token / 分）·
//   tpp ＝ 那个号 1% 约多少 · minutes ＝ 离作废还有几分钟。
fn usable(free, pace, tpp, minutes) {
  let can = pace / tpp * minutes;
  (if free < can { free } else { can }) * tpp
}

// 用速的几档（[倍数, 权]）：起伏（σ/μ）说不出或是 0 ⇒ 只一档；否则 μ、μ ± √3σ，权 2/3 · 1/6 · 1/6（正态的三点求积）。
fn pace_bands(spread) {
  let cv = if spread != () { spread.to_float() } else { 0.0 };
  if cv <= 0.0 { return [[1.0, 1.0]]; }
  let lo = 1.0 - 1.7320508 * cv;
  [[if lo > 0.0 { lo } else { 0.0 }, 1.0 / 6.0], [1.0, 2.0 / 3.0], [1.0 + 1.7320508 * cv, 1.0 / 6.0]]
}

// 视野里这个号每次 5 小时重置后再来的那几份（[[百分点, 起, 作废]]）：至多到 now + horizon；满上限（segs5 照那一刻的钟点，落空 ⇒ dflt），
//   有周上限的号几份加起来不超过周窗剩的（left7；没有周上限 ⇒ ()）；周上限卡着（wk）的那几份作废都在 exp（周重置）。
fn refills(r5, left7, wk, exp, segs5, dflt, clock, now, horizon) {
  let out = [];
  let r = r5;
  let left = left7;
  while r != () && r > now && r < now + horizon {
    let c5 = cap_or(segs5, (clock + (r - now) / 60) % 1440, dflt);
    let c = if left != () && left < c5 { left } else { c5 };
    if c > 0.0 { out.push([c, r, if wk { exp } else { r + 18000 }]); }
    if left != () { left -= c; }
    r += 18000;
  }
  out
}

// 往后推演（只算、不决定）：推到 o.end，回「卡住发不出的量 ＋ 用掉的周上限卡着的额度 ＋ 本会话一路换号重写缓存的量」（折合输入 token）。
//   前六个是按列放的每份额度：数量 · 起 · 作废 · 号 · 周上限卡着 · 延迟切入的号（按列放，号多时也不超沙箱的数组 / 表上限）。
//   守着的人（别的会话 · 本会话）一直用他那个号此刻那份，用完或作废再并进「池子」：别的会话守在哪 · 那份在第几格 · 用速多少在 oev
//   （#{acc: [号], q: [格], r: [token / 分]}；o.m ＝ 这一档用速的倍数）；本会话这一条：o.at ＝ 它要守的那份在第几格，o.rho ＝ 本会话的用速，
//   o.share ＝ 别的会话也守在这个号上的用速。池子照最先作废的那份用（延迟切入的最后）；本会话并进池子以后跟着池子走，池子每换一个号本会话就换一次：
//   换到缓存期内用过的号只补写新长的那段（o.grow：每分钟多出的重写量），否则整段重写（o.kfull）；重写的量也从那个号的额度里出。
//   o 另有：mine（本会话守的号）· lump（现在换过去当场重写的量，留着 ＝ 0）· cache（#{号: 缓存到几点}）· ttl · now · end。
//   推演本身在后端的原生内核里（lib::project_raw，一次调用一次操作），这里只把它交的几份原始量照这个口径加起来：
//   卡住的 ＋ 一路换号重写的 ＋ 每个号用掉的周上限卡着的额度。
fn project(amt, st, ex, who, wk, late, oev, o) {
  let r = lib::project_raw(amt, st, ex, who, wk, late, oev, o);
  let cost = r.stuck + r.rewrite;
  for x in r.weekly.values() { cost += x; }
  cost
}

// 往后推演一遍、交原始量（只算、不决定）：参数同 project；回 #{stuck, rewrite, weekly: #{号: tok}, left: [[号, tok, 作废时刻]]}
//   （卡住发不出的 · 一路换号重写缓存的 · 每个号用掉的周上限卡着的额度 · 推完还剩的每一份）。怎么加权由策略定。
//   原生积木：后端里那一份，不在这份文件里。

// 每号的用量上限、切入、切出照设置；智能切换关着时，用满或被拒才换号，几个号都能用时照号牌顺序。
// 切入：默认 ＝ 轮到它才用；强制切入 ＝ 有额度就切到它，几个强制切入的照号牌顺序、靠前的有额度就切回；延迟切入 ＝ 其余账号都用完才切到它，延迟时长内有账号恢复就先停发、到点用那个账号。
// 切出：默认 ＝ 到上限或被拒才换走；强制切出 ＝ 用着它时，其余账号余量合计达到切出条件（或一有额度）即换走，换走后排在其余账号后面。
// 智能切换：要换号时先用最先作废的那份额度（只有 5 小时上限的账号看 5 小时重置，周上限卡着的账号看周重置），没开窗的、周上限卡着的放后面；
// 在用的账号还能用时，往后推演「留着」与「现在换过去」两种，比卡住发不出的量、用掉的周额度与换号代价（重写一遍提示缓存），换了更少才换。量不到这个会话的用速时不提前换。
// 「现在用」选的账号与手动排的先后压过上面这些：在用的账号只在用不了时换。

/*--- rotation
{
  "format": 5,
  "settings": { "accounts": [], "smart": true },
  "form": [
    { "key": "caps", "kind": "daySegments", "scope": "each", "windows": ["5h", "7d"], "follow": true,
      "min": 0, "max": 100, "unit": "percent", "default": 90,
      "label": { "zh": "用量上限" }, "help": { "zh": "账号用量的上限，可按时段分段；0% 表示该时段停用；7 天未另设时同 5 小时" } },
    { "key": "in", "kind": "choice", "scope": "each", "default": "default",
      "choices": [
        { "v": "default", "label": { "zh": "默认" } },
        { "v": "force", "label": { "zh": "强制切入" }, "mark": { "glyph": "↺" } },
        { "v": "delay", "label": { "zh": "延迟切入" }, "mark": { "dashed": true } } ],
      "label": { "zh": "切入" },
      "help": { "zh": "默认：轮到它才用。强制切入：有额度就切到它；几个强制切入的账号都有额度时照号牌顺序，靠前的有额度就切回。延迟切入：其余账号都用完以后才切到它" } },
    { "key": "wait", "kind": "minutes", "scope": "each", "min": 0, "max": 300, "default": 0, "showIf": { "key": "in", "v": "delay" },
      "label": { "zh": "延迟" },
      "help": { "zh": "其余账号都用完时，这一时长内有账号恢复额度就先停发、到点用那个账号；没有就马上切入。0 表示用完就切" } },
    { "key": "out", "kind": "choice", "scope": "each", "default": "default",
      "choices": [
        { "v": "default", "label": { "zh": "默认" } },
        { "v": "force", "label": { "zh": "强制切出" }, "mark": { "glyph": "⇥" } } ],
      "label": { "zh": "切出" },
      "help": { "zh": "默认：用着它时，到上限或被拒才换走。强制切出：用着它时，满足切出条件即换走" } },
    { "key": "outTotal", "kind": "percent", "scope": "each", "min": 0, "max": 1000, "default": 400, "showIf": { "key": "out", "v": "force" },
      "nullable": { "zh": "一有额度" }, "lead": { "zh": "其余号余量合计达到" },
      "label": { "zh": "切出条件" },
      "help": { "zh": "其余账号余量之和，单位为百分点；余量 ＝ 上限 − 用量，两窗取小，此刻不可用的账号按 0 计" } },
    { "key": "smart", "kind": "toggle", "scope": "all", "default": true,
      "label": { "zh": "智能切换" }, "help": { "zh": "要换号时先用最先作废的那份额度；在用的账号还能用时，现在换过去能少卡住或省下周额度、且抵得过换号代价才换。额度宽裕时很少提前换" } }
  ]
}
---*/

// 一个号某一窗此刻的上限：那一窗没设 ⇒ 同 5 小时；都没设 ⇒ 90（声明里的默认）。
fn cap_of(r, w, clock) {
  let c = r.caps;
  if c != () && c[w] != () { return lib::cap_or(c[w], clock, 90.0); }
  if c != () && c["5h"] != () { return lib::cap_or(c["5h"], clock, 90.0); }
  90.0
}

// 用不了的号几点恢复（被拒到期 · 到上限那几窗的重置，取晚的；周窗只看有周上限的号）；说不出 ⇒ ()。
fn back_at(a, r, now, clock) {
  let t = now;
  if a.refusedUntil != () && a.refusedUntil > now { t = a.refusedUntil; }
  for w in [["5h", "w5h"], ["7d", "w7d"]] {
    if w[0] == "7d" && a.weekly != true { continue; }
    let c = cap_of(r, w[0], clock);
    if lib::used_of(a, w[1], now) < c { continue; }
    let at = a.resets[w[1]];
    if c <= 0.0 || at == () { return (); }
    if at > t { t = at; }
  }
  if t > now { t } else { () }
}

fn decide(ctx) {
  let s = ctx.settings;
  let now = ctx.now;
  let clock = ctx.clock;
  let cur = ctx.current;
  let memo = if ctx.memo != () { ctx.memo } else { #{} };
  let smart = s.smart != false;
  let sess = if ctx.session != () { ctx.session } else { #{} };

  // 号列表（号牌顺序）里此刻有的号：have ＝ 号 ⇒ 事实，rows ＝ 号 ⇒ 它那一行，pos ＝ 号 ⇒ 第几个。
  // 下面的数据都按「号 ⇒ 一个数」摊平放：沙箱的表上限按嵌套合计，号多时整张表放不下。
  let have = #{};
  for a in ctx.accounts { have[a.id] = a; }
  let ids = [];
  let rows = #{};
  let pos = #{};
  for r in s.accounts {
    let id = if r.start == true { ctx.start } else { r.id };
    if id == () || have[id] == () || rows[id] != () { continue; }
    pos[id] = ids.len();
    ids.push(id);
    rows[id] = r;
  }

  // 每号此刻那一份额度（余量 · 作废时刻 · 周上限卡着 · 开没开窗 · 周剩的）· 有没有额度 · 切入 · 1% 约多少 · 别的会话压在上面的用速。
  let p = #{}; let amt = #{}; let ok = #{}; let tier = #{}; let tpp = #{}; let oth = #{};
  for id in ids {
    let a = have[id];
    let r = rows[id];
    p[id] = lib::piece(a, cap_of(r, "5h", clock), cap_of(r, "7d", clock), now);
    amt[id] = p[id].amt;
    ok[id] = a.sendable == true && amt[id] > 0.0;
    tier[id] = if r["in"] != () { r["in"] } else { "default" };
    tpp[id] = if a.tokensPerPct != () && a.tokensPerPct.w5h != () { a.tokensPerPct.w5h.to_float() } else { () };
    oth[id] = if a.others != () && a.others.w5h != () { a.others.w5h.to_float() } else { () };
  }

  // 挑号的先后：智能切换开 ⇒ 最先作废的在前（lib::due_rank），同一档照号牌；关 ⇒ 照号牌。
  let rank = |list| {
    if !smart { return list; }
    let keys = [];
    for id in list { keys.push(lib::due_rank(p[id], oth[id], now) * 1024 + pos[id]); }
    keys.sort();
    let out = [];
    for k in keys { out.push(ids[k % 1024]); }
    out
  };

  // 强制切出：其余（不是延迟切入的）号余量合计达到切出条件；没设 ⇒ 400（声明里的默认），空 ⇒ 一有额度。
  let out_now = |id| {
    let r = rows[id];
    if r["out"] != "force" { return false; }
    let total = 0.0;
    let any = false;
    for x in ids { if x != id && ok[x] && tier[x] != "delay" { total += amt[x]; any = true; } }
    if !("outTotal" in r) { return total >= 400.0; }
    if r.outTotal == () { any } else { total >= r.outTotal.to_float() }
  };

  // 能用的号：强制切入的照号牌在前，默认的其次，此刻该切出的排到它们后面；延迟切入的另放。
  let force = []; let norm = []; let outs = []; let late = [];
  for id in ids {
    if !ok[id] { continue; }
    if tier[id] == "delay" { late.push(id); }
    else if out_now.call(id) { outs.push(id); }
    else if tier[id] == "force" { force.push(id); }
    else { norm.push(id); }
  }
  norm = rank.call(norm);
  late = rank.call(late);
  let front = force + norm + outs;

  // 在用的号留不留。「现在用」选的 · 手动排了先后 ⇒ 在用的号只在用不了时换。
  let why = #{};
  let keep = false;
  let pinned = ctx.pinned == true || (ctx.temp != () && ctx.temp.len() > 0);
  if rows[cur] != () && ok[cur] {
    if pinned {
      keep = true;
      why[cur] = "手动选的账号";
    } else if out_now.call(cur) {
      keep = false;
    } else if tier[cur] == "delay" {
      if front.len() == 0 { keep = true; why[cur] = "其余账号无可用额度"; }
      else if memo.late == cur { keep = true; why[cur] = if rows[cur]["out"] == "force" { "未达切出条件，继续使用" } else { "切出为默认：到上限才换" }; }
    } else if force.len() == 0 || force[0] == cur {
      keep = true;
      why[cur] = "未达用量上限，继续使用";
    }
  }

  let order = [];
  if keep { order.push(cur); }
  for id in front { if !order.contains(id) { order.push(id); } }
  let in_late = keep && tier[cur] == "delay";
  let wait_at = ();
  if front.len() == 0 && late.len() > 0 && !in_late {
    // 其余账号都用完：延迟时长内有号恢复 ⇒ 停到那一刻。
    let w = rows[late[0]].wait;
    if w == () { w = 0; }
    for id in ids {
      if w <= 0 || tier[id] == "delay" { continue; }
      let t = back_at(have[id], rows[id], now, clock);
      if t != () && t - now <= w * 60 && (wait_at == () || t < wait_at) { wait_at = t; }
    }
    if wait_at == () { order += late; }
  }
  if in_late { for id in late { if !order.contains(id) { order.push(id); } } }
  for id in ids {
    if why[id] != () { continue; }
    if order.contains(id) {
      why[id] = if tier[id] == "delay" { "其余账号无可用额度" } else if tier[id] == "force" { "强制切入" } else if smart { "最先作废" } else { "号牌顺序" };
    } else if ok[id] {
      why[id] = if tier[id] == "delay" { "其余账号用完才切入" } else { "满足切出条件" };
    } else if have[id].sendable == true {
      why[id] = "到用量上限";
    }
  }

  // 智能切换：在用的号还能用、用户没定住它、没有强制切入的号有额度 ⇒ 比「留着」与「现在换过去」。
  let cands_seen = false;
  let pace = if sess.pace != () { sess.pace.to_float() } else { () };
  if smart && keep && !pinned && pace != () && tpp[cur] != () && force.len() == 0 && norm.len() > 0 {
    // 便宜的估（O(号数)）：比在用的那份先作废、扣掉别的会话会用掉的还剩、本会话换过去用得掉的多过一次换号代价的，才算候选；
    //   候选照最先作废排，只对前 3 个往前推演。
    let pc = p[cur];
    let est = [];
    for id in norm {
      let q = p[id];
      if id == cur || tpp[id] == () || oth[id] == () || q.exp == () || q.wk { continue; }
      if !pc.wk && pc.exp != () && q.exp >= pc.exp { continue; }
      let m = (q.exp - now).to_float() / 60.0;
      let free = q.amt - oth[id] * m;
      if free <= 0.0 { continue; }
      if lib::usable(free, pace, tpp[id], m) <= lib::switch_cost(sess, id, now) { continue; }
      est.push((q.exp - now) * 1024 + pos[id]);
    }
    if est.len() > 0 {
      cands_seen = true;
      est.sort();
      let horizon = 2 * 18000;   // 往后看两个 5 小时窗：此刻开着的每一窗都在里面作废一次、再来一轮
      // 推演要的每一份额度，按列放（数量 · 起 · 作废 · 号 · 周上限卡着 · 延迟切入），连同视野里每次 5 小时重置后再来的那几份。
      let pa = []; let pst = []; let pex = []; let pid = []; let pwk = []; let plt = [];
      for id in ids {
        if tpp[id] == () { continue; }
        let a = have[id];
        let q = p[id];
        let start = now;
        if a.refusedUntil != () && a.refusedUntil > now { start = a.refusedUntil; }
        if a.sendable != true && start == now { continue; }
        let left = if q.amt > 0.0 { q.amt } else { 0.0 };
        let lt = tier[id] == "delay";
        let e = if q.exp == () { now + 2 * horizon } else { q.exp };
        let tk = tpp[id];
        pa.push(left * tk); pst.push(start); pex.push(e); pid.push(id); pwk.push(q.wk); plt.push(lt);
        let c5 = rows[id].caps;
        let segs5 = if c5 != () { c5["5h"] } else { () };
        for x in lib::refills(a.resets.w5h, if q.l7 != () { q.l7 - left } else { () }, q.wk, e, segs5, 90.0, clock, now, horizon) {
          pa.push(x[0] * tk); pst.push(x[1]); pex.push(x[2]); pid.push(id); pwk.push(q.wk); plt.push(lt);
        }
      }
      let bands = lib::pace_bands(sess.spread);
      let at = #{};   // 号 ⇒ 它此刻那份在第几格
      let k = 0;
      while k < pa.len() { if pst[k] <= now && at[pid[k]] == () { at[pid[k]] = k; } k += 1; }
      let full = lib::write_extra(sess) * (if sess.context != () { sess.context.to_float() } else { 0.0 });   // 整段重写一遍
      let grow = lib::write_extra(sess) * (if sess.grow != () { sess.grow.to_float() } else { 0.0 });
      let ttl = if sess.ttl != () { sess.ttl } else { 3600 };   // 提示缓存留多久（说不出 ⇒ 1 小时那一档）
      let cache = #{};                                          // 号 ⇒ 本会话在它上面的缓存到几点
      if sess.cached != () { for c in sess.cached.keys() { let u = sess.cached[c]["until"]; if u != () { cache[c] = u; } } }
      cache[cur] = now + ttl;
      // 别的会话守在哪个号、那份在第几格、用速多少（几种走法 · 几档用速共用）。
      let oev = #{ acc: [], q: [], r: [] };
      for id in ids {
        let o = oth[id];
        if o == () || o <= 0.0 || tpp[id] == () || id == cur { continue; }
        oev.acc.push(id); oev.q.push(at[id]); oev.r.push(o * tpp[id]);
      }
      // 一种走法的代价：几档用速各推一遍、按权平均；别的会话也守在本会话要换去的那个号上 ⇒ 并进本会话这一条一起守。
      let plan = |to, lump| {
        let j = 0.0;
        for b in bands {
          let share = ();
          if to != cur && oth[to] != () && oth[to] > 0.0 { share = oth[to] * tpp[to] * b[0]; }
          j += b[1] * lib::project(pa, pst, pex, pid, pwk, plt, oev,
                 #{ m: b[0], at: at[to], rho: pace * b[0], share: share, mine: to, lump: lump, kfull: full, grow: grow, cache: cache, ttl: ttl, now: now, end: now + horizon });
        }
        j
      };
      // 一判的操作数有上限：一遍推演约 600 × 份数次、前面的准备约 1500 × 号数次（取宽）。推得起几遍就比几个候选（至多 3 个）；
      //   一个都推不起 ⇒ 这一判不提前换（只剩用满换号时照最先作废挑）。
      let looks = (170000 - 1500 * ids.len()) / (600 * pa.len() * bands.len() + 1);
      let top = if looks - 1 < 3 { looks - 1 } else { 3 };
      let best = cur;
      let j_best = if top >= 1 { plan.call(cur, 0.0) } else { 0.0 };
      let i = 0;
      while i < est.len() && i < top {
        let id = ids[est[i] % 1024];
        let j = plan.call(id, lib::switch_cost(sess, id, now));
        if j < j_best - 1.0 { j_best = j; best = id; }
        i += 1;
      }
      if best != cur {
        why[best] = `智能切换：${(p[best].exp - now + 59) / 60} 分钟后作废，现在换过去能少卡住或省下周额度`;
        let o2 = [best];
        for id in order { if !o2.contains(id) { o2.push(id); } }
        order = o2;
      } else {
        why[cur] = "继续使用：现在换号省下的不抵换号代价";
      }
    }
  }

  // 记下延迟切入的号是不是其余都用完才切进来的（会话从它开、其余有额度 ⇒ 换走）。
  let first = if order.len() > 0 { order[0] } else { () };
  memo.late = if first != () && tier[first] == "delay" { first } else { () };

  // 一个都不能用：停到延迟里等的那一刻，或最早有号恢复。
  let hold = ();
  if order.len() == 0 {
    hold = wait_at;
    if hold == () {
      for id in ids {
        let t = back_at(have[id], rows[id], now, clock);
        if t != () && (hold == () || t < hold) { hold = t; }
      }
    }
  }

  // 几时再来问：要用的号到上限的读数 · 分时段的边界 · 延迟切入的号在用时其余号被拒到期 · 智能切换有候选时每分钟 ·
  //   要用的号还没开窗时一用就开（开了才算得出它几时作废）。
  let when = [];
  let at = ();
  for id in ids {
    let r = rows[id];
    if id == first {
      when.push(#{ acct: id, w: "5h", used_at_least: cap_of(r, "5h", clock) });
      if have[id].weekly == true { when.push(#{ acct: id, w: "7d", used_at_least: cap_of(r, "7d", clock) }); }
      if smart && pace != () && !p[id].open { when.push(#{ acct: id, w: "5h", used_at_least: lib::used_of(have[id], "w5h", now) + 0.01 }); }
    }
    let t = ();
    if first != () && tier[first] == "delay" && tier[id] != "delay" { t = have[id].refusedUntil; }
    if r.caps != () {
      for w in ["5h", "7d"] {
        let e = lib::next_edge(r.caps[w], clock);
        if e != () && (t == () || now + e * 60 < t) { t = now + e * 60; }
      }
    }
    if t != () && t > now && (at == () || t < at) { at = t; }
  }
  if cands_seen && (at == () || now + 60 < at) { at = now + 60; }

  #{ order: order, why: why, hold_until: hold, wake: #{ at: at, when: when }, memo: memo }
}

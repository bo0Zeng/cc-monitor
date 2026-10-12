// 内置策略 2：优先的 → 在计时、有余量的号（先到点的先用）→ 没开窗的（7d 用得少的先）→ 备用。
// 7d 快到线的、5h 只剩一两点的往后放；按量号排在订阅号之后。在用的号还能用就排在普通号上面（不换），除了：
//   它刚到点重置、别的号还有在计时的余量 ⇒ 不粘，让引擎换过去先用掉；
//   别的号快到点、余量还多、又比它先到点 ⇒ 那个号排到它上面；
//   它是备用号、别的号能用了 ⇒ 不粘。
// 输入的号不分先后；同分按号名（引擎的次序）。

fn meta() {
  #{
    format: 1,
    ordered: false,
    params: [
      #{ k: "grab_min",  type: "minutes", init: 30, min: 0, max: 120, label: "p_grab_min" },
      #{ k: "grab_left", type: "points",  init: 30, min: 0, max: 100, label: "p_grab_left" },
      #{ k: "skip",      type: "points",  init: 2,  min: 0, max: 50,  label: "p_skip" },
      #{ k: "tired_7d",  type: "points",  init: 10, min: 0, max: 50,  label: "p_tired_7d" },
      #{ k: "delay",     type: "minutes", init: 0,  min: 0, max: 120, label: "p_delay" },
      #{ k: "all_out",   type: "choice",  of: ["stop", "go"], init: "stop", label: "p_all_out" },
    ],
    acct_params: [
      #{ k: "line",  type: "line",   init: 90, label: "p_line" },
      #{ k: "slots", type: "slots",  init: [], label: "p_slots" },
      #{ k: "mode",   type: "choice", of: ["normal", "first", "standby"], init: "normal", label: "p_mode" },
    ],
  }
}

fn texts() {
  #{
    zh: #{
      d_soon: "按重置早晚先用",
      d_grab: "别的号 {m} 分内到点、余量 ≥ {n} 点就抢过去",
      d_reset: "在用的号到点就先用别的号的余量",
      d_first: "{acct} 优先",
      d_standby: "{acct} 备用",
      p_grab_min: "快到点（分内）",
      p_grab_left: "余量至少（点）",
      p_skip: "只剩几点以下不挑",
      p_tired_7d: "7d 离线几点内往后放",
      p_delay: "换号前延迟（分）",
      p_all_out: "都到上限后",
      p_all_out_stop: "停下",
      p_all_out_go: "接着用",
      p_line: "用到多少换",
      p_slots: "按时段另算",
      p_mode: "用法",
      p_mode_normal: "普通",
      p_mode_first: "优先",
      p_mode_first_h: "一有额度就用它",
      p_mode_standby: "备用",
      p_mode_standby_h: "别的号一有额度就不用它",
      w_first: "优先",
      w_soon: "{at} 到点，先用",
      w_fresh: "没开窗 · 7d 用了 {n}%",
      w_tired: "快到线，往后放",
      w_standby: "备用",
      w_metered: "按量号，订阅号之后",
      w_grab: "{at} 到点、还余 {n} 点，抢过去",
      w_keep: "在用，还能用就不换",
      w_reset: "刚到点重置，先用别的号的余量",
    },
    en: #{
      d_soon: "Soonest reset first",
      d_grab: "grab one resetting within {m} min with ≥ {n} pts left",
      d_reset: "use others' leftovers when the current one resets",
      d_first: "{acct} first",
      d_standby: "{acct} standby",
      p_grab_min: "Resetting soon (min)",
      p_grab_left: "At least (pts left)",
      p_skip: "Skip below (pts left)",
      p_tired_7d: "7d within (pts) goes last",
      p_delay: "Delay before switching (min)",
      p_all_out: "When all are at their limit",
      p_all_out_stop: "Stop",
      p_all_out_go: "Keep going",
      p_line: "Switch at",
      p_slots: "By time of day",
      p_mode: "Use",
      p_mode_normal: "Normal",
      p_mode_first: "First",
      p_mode_first_h: "use whenever it has quota",
      p_mode_standby: "Standby",
      p_mode_standby_h: "leave as soon as another has quota",
      w_first: "first",
      w_soon: "resets {at}, use it first",
      w_fresh: "window not started · 7d {n}% used",
      w_tired: "close to its line, later",
      w_standby: "standby",
      w_metered: "pay-as-you-go, after subscriptions",
      w_grab: "resets {at} with {n} pts left, grabbing",
      w_keep: "in use, stays while usable",
      w_reset: "just reset, using others' leftovers first",
    },
  }
}

fn describe(p, accounts) {
  let out = ["d_soon", #{ k: "d_grab", m: p.grab_min, n: p.grab_left }, "d_reset"];
  for a in accounts {
    if a.params.mode == "first" { out.push(#{ k: "d_first", acct: a.id }); }
    if a.params.mode == "standby" { out.push(#{ k: "d_standby", acct: a.id }); }
  }
  out + lib::describe_common(p, accounts)
}

// 同一档里 7d 用得少的先：0..99 ⇒ 99..0。
fn room(a) { if a.w7d.open { 99 - a.w7d.used.to_int() } else { 99 } }

fn priority(ctx) {
  let p = ctx.params;
  let ck = lib::checks(ctx);
  let out = #{};
  let cur = ();
  let counting = 0;
  let others_ok = false;
  for a in ctx.accounts {
    let u = ck[a.id];
    if a.id == ctx.current { cur = a; }
    if !u.ok { out[a.id] = #{ p: (), why: u.why, back: u.back }; continue; }
    if a.params.mode == "standby" {
      out[a.id] = #{ p: 100 + room(a), why: "w_standby" };
      continue;
    }
    if a.id != ctx.current { others_ok = true; }
    let ln = lib::line_now(a, ctx);
    if a.params.mode == "first" {
      out[a.id] = #{ p: 9000 + room(a), why: "w_first" };
    } else if a.kind == "api" {
      out[a.id] = #{ p: 500, why: "w_metered" };
    } else if a.w7d.open && ln.w7d - a.w7d.used < p.tired_7d || a.w5h.open && u.left < p.skip {
      out[a.id] = #{ p: 1000 + room(a), why: "w_tired" };
    } else if !a.w5h.open {
      out[a.id] = #{ p: 2000 + room(a), why: #{ k: "w_fresh", n: 99 - room(a) } };
    } else {
      // 先到点的先用：离到点越近，数越大（3000..3902；同一分钟里 7d 用得少的先）。
      let mins = ((a.w5h.resets - ctx.now) / 60).max(0).min(300);
      out[a.id] = #{ p: 3000 + (300 - mins) * 3 + room(a) / 40, why: #{ k: "w_soon", at: a.w5h.resets } };
      if a.id != ctx.current { counting += 1; }
    }
  }

  if cur != () && ck[cur.id].ok && cur.params.mode != "first" && !(cur.params.mode == "standby" && others_ok) {
    if !cur.w5h.open && counting > 0 {
      // 在用的号刚到点重置、别的号还有在计时的余量 ⇒ 不粘。
      out[cur.id] = #{ p: out[cur.id].p, why: "w_reset" };
    } else {
      out[cur.id] = #{ p: 8000, why: "w_keep" };
      // 别的号快到点、余量还多、又比在用的号先到点 ⇒ 排到在用的号上面。
      for a in ctx.accounts {
        if a.id == cur.id || !ck[a.id].ok || !a.w5h.open || a.params.mode != "normal" { continue; }
        let soon = a.w5h.resets - ctx.now <= p.grab_min * 60;
        let earlier = !cur.w5h.open || a.w5h.resets < cur.w5h.resets;
        if soon && ck[a.id].left >= p.grab_left && earlier {
          out[a.id] = #{ p: 8500 - (a.w5h.resets - ctx.now) / 60, why: #{ k: "w_grab", at: a.w5h.resets, n: ck[a.id].left } };
        }
      }
    }
  }

  out = lib::delay(ctx, out, p.delay);
  lib::all_out(ctx, out)
}

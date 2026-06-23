//! Native harness:baseline 參考 agent + run 驅動 + 確定性回放。
//!
//! 用途(B0 §B-2/§B-3):跑批次種子確認「每房至少一條解(agent 找得到)+ 不崩潰 + 同 op 序列
//! bit 一致」。harness 與未來客戶端跑**同一份 sim**(護欄 A);agent 是外圍工具,不進 sim crate。

use magicraid_sim::grid::{cheb, ent_index_at, in_bounds, walkable};
use magicraid_sim::movement::find_path;
use magicraid_sim::spells::validate;
use magicraid_sim::{
    apply_drop, apply_pick, config, gen_offers, init_room, step, Action, Entity, GameState, Kind,
    PickResult, RunState, Spell, Status, Target, Tile,
};

// ─────────────────────────── baseline agent ───────────────────────────

#[inline]
fn sign(n: i32) -> i32 {
    (n > 0) as i32 - (n < 0) as i32
}

/// path 第一步,若該格可走且無單位則回傳(否則 None,避免 MoveTo 撞單位被拒)。
fn step_toward(g: &GameState, sx: i32, sy: i32, tx: i32, ty: i32) -> Option<(i32, i32)> {
    let path = find_path(g, sx, sy, tx, ty)?;
    let &(nx, ny) = path.first()?;
    if walkable(g, nx, ny) && ent_index_at(g, nx, ny).is_none() {
        Some((nx, ny))
    } else {
        None
    }
}

/// 八方向(與 stepToward/igniteOil 同序)。
const DIRS8: [(i32, i32); 8] = [
    (-1, -1),
    (0, -1),
    (1, -1),
    (-1, 0),
    (1, 0),
    (-1, 1),
    (0, 1),
    (1, 1),
];

/// 推進危險格的「設局」:找一個敵人 E 與其相鄰的危險格 H(尖刺/火),
/// 站位 P = E 在 H 對側的相鄰格;站上 P 推 E → E 撞進 H 秒殺。
/// 已在 P → 立即推;否則走向最近的 P(路徑 ≤ 4 才值得繞,免得送頭)。
fn try_push_kill(g: &GameState, mx: i32, my: i32, foes: &[&Entity]) -> Option<Action> {
    let mut best_move: Option<((i32, i32), usize)> = None;
    for e in foes {
        if e.kind == Kind::Boss {
            continue; // 魔像推不動
        }
        for (dx, dy) in DIRS8 {
            let (hx, hy) = (e.x + dx, e.y + dy);
            if !in_bounds(g, hx, hy) {
                continue;
            }
            let is_hazard =
                g.tiles[hy as usize][hx as usize] == Tile::Spike || g.fire[hy as usize][hx as usize] > 0;
            if !is_hazard || !walkable(g, hx, hy) || ent_index_at(g, hx, hy).is_some() {
                continue; // H 必須是可落腳的空危險格
            }
            let (px, py) = (e.x - dx, e.y - dy); // 對側站位
            // 已站在 P 且推得到 → 立即推。
            if (mx, my) == (px, py) && validate(Spell::Push, g, Target::cell(e.x, e.y)).is_ok() {
                return Some(Action::Cast {
                    spell: Spell::Push,
                    target: Target::cell(e.x, e.y),
                });
            }
            // 否則評估走過去。P 必須可站(空、非危險、可走、可達)。
            if !in_bounds(g, px, py)
                || !walkable(g, px, py)
                || ent_index_at(g, px, py).is_some()
                || g.tiles[py as usize][px as usize] == Tile::Spike
                || g.fire[py as usize][px as usize] > 0
            {
                continue;
            }
            if let Some(path) = find_path(g, mx, my, px, py) {
                let l = path.len();
                if l <= 4 && best_move.is_none_or(|(_, bl)| l < bl) {
                    best_move = Some(((px, py), l));
                }
            }
        }
    }
    if let Some((p, _)) = best_move {
        if let Some(s) = step_toward(g, mx, my, p.0, p.1) {
            return Some(Action::MoveTo { x: s.0, y: s.1 });
        }
    }
    None
}

fn enemies(g: &GameState) -> Vec<&Entity> {
    g.entities
        .iter()
        .filter(|e| e.alive() && !e.kind.is_mage())
        .collect()
}

/// 選一個動作(貪婪 baseline)。優先序:逃砸擊 → 撿符文 → 推進危險格 → 過熱爆發 → 普攻 → 接近。
pub fn choose_action(g: &GameState, run: &RunState) -> Action {
    let mage = g.mage();
    let (mx, my) = (mage.x, mage.y);
    let owns = |s: Spell| s.baseline() || run.acquired.contains(&s);

    let foes = enemies(g);
    if foes.is_empty() {
        return Action::Wait;
    }

    // ── 魔像戰 ──
    // 關鍵節奏:**每手都打魔像**。普攻 3,過熱窗口雙倍 = 6;魔像 ~5 發內倒,期間只挨 1–2 下砸擊
    // (14 HP 撐得住)。逃跑/繞路撿符文反而打斷輸出節奏 → 不做。血真的快沒了才補。
    if let Some(boss) = foes.iter().copied().find(|e| e.kind == Kind::Boss) {
        let can_bolt = validate(Spell::Bolt, g, Target::cell(boss.x, boss.y)).is_ok();
        // 不在過熱窗口、血很低、且這手打不到 boss(沒輸出損失)→ 補血。
        if !boss.exhausted && mage.hp <= 4 && run.potions > 0 && !can_bolt {
            return Action::Potion;
        }
        // 過熱窗口火球(★★ 留火腳下 DoT)優先,否則魔法彈;在射程就打。
        if boss.exhausted
            && owns(Spell::Fire)
            && validate(Spell::Fire, g, Target::cell(boss.x, boss.y)).is_ok()
        {
            return Action::Cast {
                spell: Spell::Fire,
                target: Target::cell(boss.x, boss.y),
            };
        }
        if can_bolt {
            return Action::Cast {
                spell: Spell::Bolt,
                target: Target::cell(boss.x, boss.y),
            };
        }
        // 不在射程 → 接近(LoS/距離拉進就能持續輸出)。
        if let Some(s) = step_toward(g, mx, my, boss.x, boss.y) {
            return Action::MoveTo { x: s.0, y: s.1 };
        }
        return Action::Wait;
    }

    // ── 一般敵人 ──
    // 推進危險格(環境擊殺):立即可推就推,否則走到「設局位」把敵人卡在你與尖刺/火之間。
    if owns(Spell::Push) {
        if let Some(a) = try_push_kill(g, mx, my, &foes) {
            return a;
        }
    }
    // 勾索把遠敵拉進危險格:拉的第一步就是尖刺/火 → 直接秒。
    if owns(Spell::Hook) {
        for e in &foes {
            if e.kind == Kind::Boss {
                continue;
            }
            let (sx, sy) = (e.x + sign(mx - e.x), e.y + sign(my - e.y));
            let lands_hazard = in_bounds(g, sx, sy)
                && (g.tiles[sy as usize][sx as usize] == Tile::Spike
                    || g.fire[sy as usize][sx as usize] > 0);
            if lands_hazard && validate(Spell::Hook, g, Target::cell(e.x, e.y)).is_ok() {
                return Action::Cast {
                    spell: Spell::Hook,
                    target: Target::cell(e.x, e.y),
                };
            }
        }
    }

    // 魔法彈:射程 + 視線內。目標優先序:**近戰小鬼先於遠程符文眼**(先解致命威脅)→ 血低 → 近。
    let key = |e: &Entity| (e.kind == Kind::Eye, e.hp, cheb(mx, my, e.x, e.y), e.id);
    let best = foes
        .iter()
        .copied()
        .filter(|e| validate(Spell::Bolt, g, Target::cell(e.x, e.y)).is_ok())
        .min_by_key(|e| key(e));
    if let Some(e) = best {
        return Action::Cast {
            spell: Spell::Bolt,
            target: Target::cell(e.x, e.y),
        };
    }

    // 接近最近敵人。
    let near = foes
        .iter()
        .copied()
        .min_by_key(|e| (cheb(mx, my, e.x, e.y), e.id))
        .unwrap();
    if let Some(s) = step_toward(g, mx, my, near.x, near.y) {
        return Action::MoveTo { x: s.0, y: s.1 };
    }
    Action::Wait
}

/// 三選一偏好:火 > 推 > 勾 > 其餘(依出現序)。固定 → 確定性。
pub fn choose_pick(offers: &[Spell]) -> Option<Spell> {
    const PREF: [Spell; 3] = [Spell::Fire, Spell::Push, Spell::Hook];
    for p in PREF {
        if offers.contains(&p) {
            return Some(p);
        }
    }
    offers.first().copied()
}

// ─────────────────────────── run 驅動 + 回放 ───────────────────────────

/// 一場 run 的操作記錄(回放用)。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunOp {
    Battle(Action),
    Pick(Spell),
    Drop { take: Spell, drop: Spell },
    NextRoom,
}

/// 最終狀態快照(bit 比對用)。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub entities: Vec<(u32, i32, i32, i32, i64)>,
    pub tiles: Vec<Vec<Tile>>,
    pub fire: Vec<Vec<i32>>,
    pub burn_t: Vec<Vec<i32>>,
    pub acquired: Vec<Spell>,
    pub potions: u32,
    pub room_idx: usize,
}

fn snapshot(g: &GameState, run: &RunState) -> Snapshot {
    Snapshot {
        entities: g
            .entities
            .iter()
            .map(|e| (e.id, e.x, e.y, e.hp, e.time))
            .collect(),
        tiles: g.tiles.clone(),
        fire: g.fire.clone(),
        burn_t: g.burn_t.clone(),
        acquired: run.acquired.clone(),
        potions: run.potions,
        room_idx: run.room_idx,
    }
}

/// 一場 run 的結果。
pub struct Trace {
    pub outcome: Status,
    pub steps: usize,
    pub max_room: usize,
    pub ops: Vec<RunOp>,
    pub final_snapshot: Snapshot,
}

/// 用 baseline agent 跑完一場(種子外傳)。`budget` = 戰鬥手數上限(防卡死)。
pub fn play(seed: u32, budget: usize) -> Trace {
    let mut run = RunState::new(seed);
    let mut g = init_room(0);
    let mut ops = Vec::new();
    let mut status = Status::AwaitingInput;
    let mut steps = 0usize;
    let mut max_room = 0usize;

    loop {
        match status {
            Status::AwaitingInput | Status::AwaitingRelease => {
                let a = if status == Status::AwaitingRelease {
                    Action::Wait // 觸發釋放
                } else {
                    choose_action(&g, &run)
                };
                ops.push(RunOp::Battle(a));
                status = step(&mut g, &mut run, a).status;
                steps += 1;
                if steps >= budget {
                    break;
                }
            }
            Status::PickOffered => {
                let offers = gen_offers(&run);
                if let Some(pick) = choose_pick(&offers) {
                    match apply_pick(&mut run, pick) {
                        PickResult::Done => ops.push(RunOp::Pick(pick)),
                        PickResult::NeedDrop => {
                            let drop = run.acquired[0]; // 丟最舊的(確定性)
                            apply_drop(&mut run, pick, drop);
                            ops.push(RunOp::Drop { take: pick, drop });
                        }
                    }
                }
                run.room_idx += 1;
                g = init_room(run.room_idx);
                ops.push(RunOp::NextRoom);
                max_room = max_room.max(run.room_idx);
                status = Status::AwaitingInput;
            }
            Status::RunComplete | Status::Defeat => break,
        }
    }

    let final_snapshot = snapshot(&g, &run);
    Trace {
        outcome: status,
        steps,
        max_room,
        ops,
        final_snapshot,
    }
}

/// 依「同種子 + 同 op 序列」重放,回傳最終狀態(確定性驗證用)。不經 agent。
pub fn replay(seed: u32, ops: &[RunOp]) -> (Status, Snapshot) {
    let mut run = RunState::new(seed);
    let mut g = init_room(0);
    let mut status = Status::AwaitingInput;
    for op in ops {
        match op {
            RunOp::Battle(a) => status = step(&mut g, &mut run, *a).status,
            RunOp::Pick(s) => {
                apply_pick(&mut run, *s);
            }
            RunOp::Drop { take, drop } => apply_drop(&mut run, *take, *drop),
            RunOp::NextRoom => {
                run.room_idx += 1;
                g = init_room(run.room_idx);
                status = Status::AwaitingInput;
            }
        }
    }
    (status, snapshot(&g, &run))
}

/// 總房數(= ROOMS 長度)。
pub fn room_count() -> usize {
    config::ROOMS.len()
}

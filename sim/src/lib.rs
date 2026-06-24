//! 魔法基地突襲 — 確定性 sim 核(headless)。
//!
//! **規格 = `prototype/demo1.html` 的行為**(已驗證好玩),不是重寫玩法。
//! 一份實作同時餵客戶端(WASM)、harness(native)、未來伺服器(cgo)——見 CLAUDE.md 紀律。
//!
//! 目標契約(B0 §E,逐模組長到位後實作):
//! ```text
//! step(state, action, seed) -> { state, events, status }   // 純函式,無 DOM/計時器/外部隨機
//! ```
//!
//! ## 目前進度(階段 B 骨架)
//! - ✅ `config` / `state` / `events` / `time_chain`:時間鏈與確定性地基已可驗。
//! - ✅ `grid` / `damage` / `terrain`:格子謂詞、整數 LoS、傷害+event、火油木牆 CA(確定性已驗)。
//! - ✅ `movement`:推/震/勾/走位落地 + ZOC 煞車 + 整數成本 A\*(D-3)。
//! - ✅ `ai`:小鬼/符文眼/魔像三套確定性行為 + 過熱循環。
//! - ✅ `spells`:7 招 registry(validate/cast/initiate)+ 烈焰術 AoE + 連鎖閃電接口。
//! - ✅ `roguelite`:確定性 PRNG(種子外傳)+ 三選一撿取 + 房間載入 + run 狀態。
//! - ⏳ 待補:`lib::step` 全契約(時間鏈 + action + status)與 `project_chain` 完整前瞻。

pub mod ai;
pub mod config;
pub mod damage;
pub mod events;
pub mod grid;
pub mod movement;
pub mod roguelite;
pub mod spells;
pub mod state;
pub mod terrain;
pub mod time_chain;

pub use damage::{StepCtx, TierTable};
pub use events::{Cause, Event, Status};
pub use roguelite::{
    apply_drop, apply_pick, gen_offers, hash32, init_base, init_room, rng_for, Mulberry32, Op,
    PickResult, RunState,
};
pub use spells::{Element, Reject, Spell, Target, TargetKind};
pub use state::{Channel, Entity, GameState, Kind, Tile};

use config::time_step;
use grid::{ent_index_at, in_bounds, walkable};
use movement::{find_path, move_mage_to, walk_brake};

/// 玩家在自己回合可下的指令(對應 JS `selectAction`/`cellClick` 的分支)。
///
/// `Pick`/`Drop`/換房屬 roguelite 層(見 `roguelite::apply_*` + `init_room`),不在戰鬥 step 內。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// 走到目標格(A\* 算在 sim,回放只存目的地)。對應 cellClick move。
    MoveTo { x: i32, y: i32 },
    /// 施法(channel 法術會起手前搖,非 channel 立即結算)。
    Cast { spell: Spell, target: Target },
    /// 釋放蓄力中的法術(AwaitingRelease 時)。
    Release,
    /// 待機一手。
    Wait,
    /// 喝回血瓶。
    Potion,
}

/// `step` 的結果。`rejected = Some` 表示動作非法、**沒有時間流逝**(狀態維持原樣)。
#[derive(Clone, Debug)]
pub struct StepResult {
    pub events: Vec<Event>,
    pub status: Status,
    pub rejected: Option<Reject>,
}

/// 順序鏈投影的一格(對應 JS `projectChain` 輸出的 `{kind, tag}`)。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChainSlot {
    pub kind: Kind,
    pub id: u32,
    /// 這格是「釋放蓄力」的那一手(view 畫成釋放標記)。
    pub releasing: bool,
}

/// 推進法師的時間值(對應 JS `endMageAction` 的 time 部分)。
fn end_mage_turn(g: &mut GameState) {
    let eff = g.mage().eff_speed_halves();
    let m = g.mage_mut();
    m.time += time_step(eff);
    if m.haste_turns > 0 {
        m.haste_turns -= 1;
    }
}

/// 終局/換房判定(統一 JS `defeat` 與 `checkClear`)。優先序:法師死 → 通關 → 清房。
fn terminal_status(g: &GameState) -> Option<Status> {
    if g.mage().hp <= 0 {
        return Some(Status::Defeat);
    }
    // base-raid(有核心):踩到核心 _或_ 清光守軍 = 突襲成功(RunComplete);否則續打。野區無核心,跳過。
    if let Some((cx, cy)) = g.core {
        let m = g.mage();
        if (m.x, m.y) == (cx, cy) || g.alive_enemies() == 0 {
            return Some(Status::RunComplete);
        }
        return None;
    }
    if let Some(boss) = g.entities.iter().find(|e| e.kind == Kind::Boss) {
        if !boss.alive() {
            return Some(Status::RunComplete);
        }
    }
    if g.alive_enemies() == 0 {
        let has_boss = g.entities.iter().any(|e| e.kind == Kind::Boss);
        if !has_boss && g.room_idx + 1 < config::ROOMS.len() {
            return Some(Status::PickOffered);
        }
    }
    None
}

/// 釋放蓄力(目前僅烈焰術)。對應 JS `releaseHeavy` 的結算部分。
fn release_channel(g: &mut GameState, ctx: &mut StepCtx) {
    if let Some(ch) = g.mage().channel.clone() {
        g.mage_mut().channel = None;
        spells::resolve_heavy(g, ch.tx, ch.ty, ctx);
    }
    end_mage_turn(g);
}

/// 套用玩家動作(含推進法師時間)。非法回 `Err(Reject)`,**不推進時間**。
fn apply_action(
    g: &mut GameState,
    run: &mut RunState,
    action: Action,
    ctx: &mut StepCtx,
) -> Result<(), Reject> {
    match action {
        Action::Wait => {
            end_mage_turn(g);
            Ok(())
        }
        Action::Potion => {
            let (hp, maxhp) = {
                let m = g.mage();
                (m.hp, m.maxhp)
            };
            if run.potions == 0 || hp >= maxhp {
                return Err(Reject::CannotDrink);
            }
            run.potions -= 1;
            let healed = (hp + config::POTION_HEAL).min(maxhp) - hp;
            let id = {
                let m = g.mage_mut();
                m.hp += healed;
                m.id
            };
            ctx.events.push(Event::Healed { id, amt: healed });
            end_mage_turn(g);
            Ok(())
        }
        Action::Cast { spell, target } => {
            spells::validate(spell, g, target)?;
            if spell.is_channel() {
                spells::initiate(spell, g, target, ctx);
            } else {
                spells::cast(spell, g, target, ctx);
            }
            end_mage_turn(g);
            Ok(())
        }
        Action::MoveTo { x, y } => {
            let (mx, my) = {
                let m = g.mage();
                (m.x, m.y)
            };
            if !in_bounds(g, x, y)
                || !walkable(g, x, y)
                || ent_index_at(g, x, y).is_some()
                || (x == mx && y == my)
            {
                return Err(Reject::BlockedDestination);
            }
            let path = find_path(g, mx, my, x, y).ok_or(Reject::NoPath)?;
            let next = *path.first().ok_or(Reject::NoPath)?;
            ctx.mage_hurt = false;
            // 多格且第一步不進控制區 → 自動連走;否則只走一步。
            let auto = path.len() > 1 && !walk_brake(g, Some(next), ctx);
            g.mage_mut().path = if auto { Some(path[1..].to_vec()) } else { None };
            move_mage_to(g, next.0, next.1, ctx);
            end_mage_turn(g);
            Ok(())
        }
        Action::Release => Err(Reject::NothingToRelease),
    }
}

/// 跑時間鏈直到控制權回到玩家或進入終局(對應 JS `advance`)。
fn run_chain(g: &mut GameState, ctx: &mut StepCtx) -> Status {
    let mut guard = 0u32;
    loop {
        guard += 1;
        debug_assert!(guard < 100_000, "時間鏈未收斂(疑似死迴圈)");
        if guard >= 100_000 {
            return Status::AwaitingInput;
        }

        let ai = next_actor_index(g).expect("應永遠有存活者");
        if g.entities[ai].kind.is_mage() {
            terrain::fire_tick(g, ctx);
            if let Some(s) = terminal_status(g) {
                return s;
            }
            // 蓄力:被打斷則清掉、否則停在釋放手。
            if let Some(ch) = g.mage().channel.clone() {
                if ch.interrupted {
                    g.mage_mut().channel = None;
                } else {
                    g.mage_mut().channel.as_mut().unwrap().ready = true;
                    return Status::AwaitingRelease;
                }
            }
            // auto-walk:消耗一步直到 ZOC 煞車/抵達。
            if let Some(next) = g.mage().path.as_ref().and_then(|p| p.first().copied()) {
                let blocked = walk_brake(g, Some(next), ctx)
                    || !walkable(g, next.0, next.1)
                    || ent_index_at(g, next.0, next.1).is_some();
                if blocked {
                    g.mage_mut().path = None;
                } else {
                    g.mage_mut().path.as_mut().unwrap().remove(0);
                    move_mage_to(g, next.0, next.1, ctx);
                    end_mage_turn(g);
                    if let Some(s) = terminal_status(g) {
                        return s;
                    }
                    continue;
                }
            }
            return Status::AwaitingInput;
        } else {
            ai::enemy_act(g, ai, ctx);
            let eff = g.entities[ai].eff_speed_halves();
            g.entities[ai].time += time_step(eff);
            if g.mage().hp <= 0 {
                return Status::Defeat;
            }
        }
    }
}

/// **核心契約**:`step(state, action, …) → {events, status}`(B0 §E)。純函式、確定性。
///
/// 套用玩家一手 → 跑完時間鏈(敵人手 + auto-walk),直到控制權回玩家或終局。
/// `AwaitingRelease` 狀態下,**任何 action 都視為釋放**(對應 JS 任意點擊觸發 releaseHeavy)。
/// tier(★★)由 `run` 複製進 step-local ctx;戰鬥不寫回 run(撿取/升級走 roguelite 層)。
pub fn step(g: &mut GameState, run: &mut RunState, action: Action) -> StepResult {
    let mut ctx = StepCtx::new();
    ctx.tiers = run.tiers.clone();

    let ready = g.mage().channel.as_ref().is_some_and(|c| c.ready);
    if ready {
        release_channel(g, &mut ctx);
    } else if let Err(r) = apply_action(g, run, action, &mut ctx) {
        // 非法動作:不推進時間,維持當前等待狀態。
        return StepResult {
            events: ctx.events,
            status: Status::AwaitingInput,
            rejected: Some(r),
        };
    }

    let status = terminal_status(g).unwrap_or_else(|| run_chain(g, &mut ctx));
    StepResult {
        events: ctx.events,
        status,
        rejected: None,
    }
}

/// 順序鏈前瞻 `n` 手(唯讀 query,不改狀態)。對應 JS `projectChain`(行 612–624)。
///
/// 純確定性模擬:複製存活者的 (time, 速度, 加速, 是否蓄力),反覆取最小時間者推進。
pub fn project_chain(g: &GameState, n: usize) -> Vec<ChainSlot> {
    struct SimE {
        kind: Kind,
        id: u32,
        time: i64,
        speed_halves: u8,
        haste: u32,
        channel: bool,
    }
    let mut sim: Vec<SimE> = g
        .entities
        .iter()
        .filter(|e| e.alive())
        .map(|e| SimE {
            kind: e.kind,
            id: e.id,
            time: e.time,
            speed_halves: e.speed_halves,
            haste: if e.kind.is_mage() { e.haste_turns } else { 0 },
            channel: e.kind.is_mage() && e.channel.is_some(),
        })
        .collect();

    let mut out = Vec::with_capacity(n);
    let mut first_mage = true;
    for _ in 0..n {
        if sim.is_empty() {
            break;
        }
        // time 升 → mage 優先 → id 升(與 next_actor 同鍵)。
        sim.sort_by(|a, b| {
            a.time
                .cmp(&b.time)
                .then_with(|| (!a.kind.is_mage()).cmp(&(!b.kind.is_mage())))
                .then_with(|| a.id.cmp(&b.id))
        });
        let a = &mut sim[0];
        let releasing = a.kind.is_mage() && a.channel && first_mage;
        if releasing {
            first_mage = false;
        }
        out.push(ChainSlot {
            kind: a.kind,
            id: a.id,
            releasing,
        });
        let eff = if a.kind.is_mage() && a.haste > 0 {
            config::SPEED_HALVES_HASTE
        } else {
            a.speed_halves
        };
        a.time += time_step(eff);
        if a.kind.is_mage() && a.haste > 0 {
            a.haste -= 1;
        }
    }
    out
}

use time_chain::next_actor_index;

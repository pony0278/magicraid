//! 法術 registry —— 資料驅動,新增一招 = 加一個變體 + 補 match 一筆。
//!
//! 對應 `prototype/demo1.html` `SPELLS`(296–388)、`CHAIN_SCAN`/`chainCandidates`(401–407)、
//! `heavyArea`/`resolveHeavy`(506–521)。
//!
//! **只 port sim 半邊**(B0 §C-3):`element / baseline / channel / target / max_tier / validate /
//! cast | initiate`。view 半邊(`icon/name/cost/desc/up/preview/noTarget`)留在殼端。
//! validate 回**結構化 `Reject`**(非中文字串)—— 字串是 view 的在地化(B0 §G-5)。

use crate::config::*;
use crate::damage::{deal_damage, StepCtx};
use crate::events::Event;
use crate::grid::{cheb, ent_index_at, heavy_area, in_bounds, los};
use crate::movement::{do_pull, do_push, shove_dir};
use crate::state::{Channel, GameState, Kind, Tile};
use crate::terrain::ignite_oil;

/// 元素(§10 元素=機制)。Demo 1 首發只用到 fire/neutral/physical;其餘預留。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Element {
    Fire,
    Water,
    Electric,
    Earth,
    Neutral,
    Physical,
}

/// 瞄準型別(共用 schema:sim 定 action 形狀、view 定瞄準 UI)。對應 JS `target`。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetKind {
    Enemy,
    AdjEnemy,
    Cell,
    SelfCast,
}

/// 施法目標格。self 類忽略座標。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Target {
    pub x: i32,
    pub y: i32,
}

impl Target {
    pub fn cell(x: i32, y: i32) -> Self {
        Target { x, y }
    }
    pub fn none() -> Self {
        Target { x: 0, y: 0 }
    }
}

/// validate 拒絕原因(結構化;view 對應中文)。對應 JS validate 回的字串。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reject {
    NoEnemyThere,
    OutOfRange,
    NoLineOfSight,
    NotAdjacent,
    AlreadyAdjacent,
    TargetIsWall,
    NotFloor,
    OutOfBounds,
    // ── step 層動作的拒絕原因(非法術 validate,但共用此枚舉) ──
    /// 沒血瓶 / 血已滿,喝藥無效。
    CannotDrink,
    /// 移動目標是牆/有單位/原地。
    BlockedDestination,
    /// 找不到到目標的路。
    NoPath,
    /// 沒有可釋放的蓄力。
    NothingToRelease,
}

/// 法術 id。對應 JS `SPELLS` 的鍵(同時是 `tierOf` 的鍵)。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spell {
    Bolt,
    Push,
    Fire,
    Heavy,
    OilFlask,
    Hook,
    Haste,
}

impl Spell {
    /// 法術字串 id(SpellCast 事件 + tier 查詢)。
    pub fn id(self) -> &'static str {
        match self {
            Spell::Bolt => "bolt",
            Spell::Push => "push",
            Spell::Fire => "fire",
            Spell::Heavy => "heavy",
            Spell::OilFlask => "oilflask",
            Spell::Hook => "hook",
            Spell::Haste => "haste",
        }
    }

    pub fn element(self) -> Element {
        match self {
            Spell::Bolt => Element::Neutral,
            Spell::Push => Element::Physical,
            Spell::Fire => Element::Fire,
            Spell::Heavy => Element::Fire,
            Spell::OilFlask => Element::Neutral,
            Spell::Hook => Element::Physical,
            Spell::Haste => Element::Neutral,
        }
    }

    /// 基礎包(不算進法杖欄位上限,不進撿取池)。對齊 docs/01 §10 base kit:魔法彈 + 推。
    /// (§10 base kit 另含火球;Demo 0/1 先只把「推」設為基礎,讓房間 2 穩定教得出推進陷阱。)
    pub fn baseline(self) -> bool {
        matches!(self, Spell::Bolt | Spell::Push)
    }

    /// 是否前搖兩格制(channel)。對應 JS `channel`。
    pub fn is_channel(self) -> bool {
        matches!(self, Spell::Heavy)
    }

    pub fn target_kind(self) -> TargetKind {
        match self {
            Spell::Bolt | Spell::Hook => TargetKind::Enemy,
            Spell::Push => TargetKind::AdjEnemy,
            Spell::Fire | Spell::Heavy | Spell::OilFlask => TargetKind::Cell,
            Spell::Haste => TargetKind::SelfCast,
        }
    }

    /// 升級上限(基礎 1,可升到 2 = ★★)。對應 JS `maxTier`(無則 1)。
    pub fn max_tier(self) -> u8 {
        match self {
            Spell::Bolt => 1,
            _ => 2,
        }
    }
}

/// 所有法術,**依 JS `SPELLS` 物件鍵的插入序**(確定性關鍵:撿取池洗牌前的初始序)。
pub const SPELL_ORDER: [Spell; 7] = [
    Spell::Bolt,
    Spell::Push,
    Spell::Fire,
    Spell::Heavy,
    Spell::OilFlask,
    Spell::Hook,
    Spell::Haste,
];

/// 可撿取池(baseline == false),保持 `SPELL_ORDER` 順序。對應 JS `PICKABLE`。
pub fn pickable() -> Vec<Spell> {
    SPELL_ORDER.iter().copied().filter(|s| !s.baseline()).collect()
}

#[inline]
fn sign(n: i32) -> i32 {
    (n > 0) as i32 - (n < 0) as i32
}

fn mage_pos(g: &GameState) -> (i32, i32) {
    let m = g.mage();
    (m.x, m.y)
}

/// 該格上的「敵人」索引(排除法師)。對應 JS `t.ent` 且 `kind!=="mage"` 的用法。
fn enemy_at(g: &GameState, x: i32, y: i32) -> Option<usize> {
    ent_index_at(g, x, y).filter(|&i| !g.entities[i].kind.is_mage())
}

/// 合法性檢查(對應各 SPELL 的 `validate`)。合法回 `Ok`,否則回結構化 `Reject`。
pub fn validate(spell: Spell, g: &GameState, t: Target) -> Result<(), Reject> {
    let (mx, my) = mage_pos(g);
    match spell {
        Spell::Bolt => {
            let e = enemy_at(g, t.x, t.y).ok_or(Reject::NoEnemyThere)?;
            if cheb(mx, my, g.entities[e].x, g.entities[e].y) > BOLT_RANGE {
                return Err(Reject::OutOfRange);
            }
            if !los(g, mx, my, t.x, t.y) {
                return Err(Reject::NoLineOfSight);
            }
            Ok(())
        }
        Spell::Push => {
            let e = enemy_at(g, t.x, t.y).ok_or(Reject::NoEnemyThere)?;
            if cheb(mx, my, g.entities[e].x, g.entities[e].y) != 1 {
                return Err(Reject::NotAdjacent);
            }
            Ok(())
        }
        Spell::Hook => {
            let e = enemy_at(g, t.x, t.y).ok_or(Reject::NoEnemyThere)?;
            let d = cheb(mx, my, g.entities[e].x, g.entities[e].y);
            if d > HOOK_RANGE {
                return Err(Reject::OutOfRange);
            }
            if d == 1 {
                return Err(Reject::AlreadyAdjacent);
            }
            if !los(g, mx, my, g.entities[e].x, g.entities[e].y) {
                return Err(Reject::NoLineOfSight);
            }
            Ok(())
        }
        Spell::Fire => {
            if !in_bounds(g, t.x, t.y) || cheb(mx, my, t.x, t.y) > FIRE_RANGE {
                return Err(Reject::OutOfRange);
            }
            if !los(g, mx, my, t.x, t.y) {
                return Err(Reject::NoLineOfSight);
            }
            if crate::grid::blocks_move(g, t.x, t.y) {
                return Err(Reject::TargetIsWall);
            }
            Ok(())
        }
        Spell::Heavy => {
            if !in_bounds(g, t.x, t.y) || cheb(mx, my, t.x, t.y) > HEAVY_RANGE {
                return Err(Reject::OutOfRange);
            }
            if !los(g, mx, my, t.x, t.y) {
                return Err(Reject::NoLineOfSight);
            }
            if g.tiles[t.y as usize][t.x as usize] == Tile::Wall {
                return Err(Reject::TargetIsWall);
            }
            Ok(())
        }
        Spell::OilFlask => {
            if !in_bounds(g, t.x, t.y) || cheb(mx, my, t.x, t.y) > OIL_RANGE {
                return Err(Reject::OutOfRange);
            }
            if !los(g, mx, my, t.x, t.y) {
                return Err(Reject::NoLineOfSight);
            }
            if g.tiles[t.y as usize][t.x as usize] != Tile::Floor {
                return Err(Reject::NotFloor);
            }
            Ok(())
        }
        Spell::Haste => Ok(()),
    }
}

/// 立即結算的法術(非 channel)。對應各 SPELL 的 `cast`。先發 `SpellCast` 事件。
pub fn cast(spell: Spell, g: &mut GameState, t: Target, ctx: &mut StepCtx) {
    let mage_id = g.mage().id;
    ctx.events.push(Event::SpellCast {
        id: mage_id,
        spell: spell.id(),
    });
    match spell {
        Spell::Bolt => {
            // 投射物攔截:命中視線上**第一個**單位(擋在前面的敵人會替後方擋彈)。
            let (mx, my) = mage_pos(g);
            if let Some(victim) = crate::grid::first_unit_on_ray(g, mx, my, t.x, t.y) {
                if !g.entities[victim].kind.is_mage() {
                    deal_damage(g, victim, BOLT_DMG, ctx);
                }
            }
        }
        Spell::Push => {
            if let Some(e) = enemy_at(g, t.x, t.y) {
                do_push(g, e, ctx);
            }
        }
        Spell::Hook => {
            if let Some(e) = enemy_at(g, t.x, t.y) {
                do_pull(g, e, ctx);
            }
        }
        Spell::Fire => {
            // 先直擊敵人,再依目標格點油 / 留火(★★)。
            if let Some(e) = enemy_at(g, t.x, t.y) {
                deal_damage(g, e, FIRE_DMG, ctx);
            }
            let (tx, ty) = (t.x as usize, t.y as usize);
            if g.tiles[ty][tx] == Tile::Oil {
                ignite_oil(g, t.x, t.y, ctx);
            } else if ctx.tiers.of("fire") >= 2 && !crate::grid::blocks_move(g, t.x, t.y) {
                g.fire[ty][tx] = FIRE_DUR; // 火球★★:命中格留一格火
            }
        }
        Spell::OilFlask => {
            let (mx, my) = mage_pos(g);
            paint_oil(g, t.x, t.y);
            if ctx.tiers.of("oilflask") >= 2 {
                // 澆油★★:沿「遠離法師」方向再潑成 3 格油線。
                let (dx, dy) = (sign(t.x - mx), sign(t.y - my));
                paint_oil(g, t.x + dx, t.y + dy);
                paint_oil(g, t.x + 2 * dx, t.y + 2 * dy);
            }
        }
        Spell::Haste => {
            let mage = g.mage_mut();
            mage.haste_turns = HASTE_GRANT;
            let id = mage.id;
            ctx.events.push(Event::HasteGained { id });
        }
        Spell::Heavy => {
            // Heavy 是 channel,不該走 cast;保險起見直接結算(等同立即釋放)。
            resolve_heavy(g, t.x, t.y, ctx);
        }
    }
}

/// 起手 channel 法術(設前搖,不在此結算)。對應 SPELL 的 `initiate`。
pub fn initiate(spell: Spell, g: &mut GameState, t: Target, ctx: &mut StepCtx) {
    debug_assert!(spell.is_channel(), "initiate 只用於 channel 法術");
    let mage = g.mage_mut();
    mage.channel = Some(Channel {
        spell: spell.id(),
        tx: t.x,
        ty: t.y,
        ready: false,
        interrupted: false,
    });
    let id = mage.id;
    ctx.events.push(Event::SpellCast {
        id,
        spell: spell.id(),
    });
}

/// 只在 `floor` 上潑油(對應 JS oilflask 的 `paint`)。
fn paint_oil(g: &mut GameState, x: i32, y: i32) {
    if in_bounds(g, x, y) && g.tiles[y as usize][x as usize] == Tile::Floor {
        g.tiles[y as usize][x as usize] = Tile::Oil;
    }
}

/// 烈焰術釋放結算(加號 AoE:傷害 + 點油/燒木/留火;烈焰★★ 把命中敵人外震一格)。
/// 對應 JS `resolveHeavy`(507–521)。供 channel 釋放時呼叫。
pub fn resolve_heavy(g: &mut GameState, cx: i32, cy: i32, ctx: &mut StepCtx) {
    let mut hit: Vec<usize> = Vec::new();
    for (x, y) in heavy_area(g, cx, cy) {
        if let Some(e) = enemy_at(g, x, y) {
            deal_damage(g, e, HEAVY_DMG, ctx);
            if g.entities[e].alive() {
                hit.push(e);
            }
        }
        let (ux, uy) = (x as usize, y as usize);
        match g.tiles[uy][ux] {
            Tile::Oil => g.fire[uy][ux] = FIRE_DUR,
            Tile::Wood => {
                g.tiles[uy][ux] = Tile::WoodBurn;
                g.burn_t[uy][ux] = WOOD_BURN_TICKS;
            }
            Tile::Floor | Tile::Spike => g.fire[uy][ux] = FIRE_DUR,
            _ => {}
        }
    }
    if ctx.tiers.of("heavy") >= 2 {
        for e in hit {
            if g.entities[e].alive() && g.entities[e].kind != Kind::Boss {
                let (ex, ey) = (g.entities[e].x, g.entities[e].y);
                shove_dir(g, e, sign(ex - cx), sign(ey - cy), ctx);
            }
        }
    }
}

/// 連鎖閃電的確定性鏈跳候選(接口;Demo 1 首發尚無電系法術使用)。
///
/// 對應 JS `CHAIN_SCAN`/`chainCandidates`(401–407):固定方向掃描序蒐集,**再依 id 排序**,
/// 絕不靠迭代序。`hit` = 已命中的 entity id(防跳回)。回傳 entity 索引(id 升序)。
pub const CHAIN_SCAN: [(i32, i32); 8] = [
    (0, -1),
    (1, -1),
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
];

pub fn chain_candidates(g: &GameState, from_x: i32, from_y: i32, hit: &[u32]) -> Vec<usize> {
    let mut out: Vec<usize> = Vec::new();
    for (dx, dy) in CHAIN_SCAN {
        let (x, y) = (from_x + dx, from_y + dy);
        if !in_bounds(g, x, y) {
            continue;
        }
        if let Some(e) = enemy_at(g, x, y) {
            if !hit.contains(&g.entities[e].id) {
                out.push(e);
            }
        }
    }
    out.sort_by_key(|&i| g.entities[i].id);
    out
}

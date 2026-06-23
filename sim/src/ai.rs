//! 敵人 AI(確定性)。
//!
//! 對應 `prototype/demo1.html`:`stepToward`(585–592)、`enemyAct`(593–609)。
//! 三套行為:小鬼貼臉、符文眼吃視線射擊、魔像砸擊過熱循環。全程無 RNG。
//!
//! 確定性:鄰居展開序(dy,dx 各 −1..=1)寫死;`step_toward` 嚴格 `<` 比較 → 同距時保留**先掃到**的格。

use crate::config::{BOSS_SLAM, EYE_DMG, EYE_RANGE, IMP_DMG};
use crate::damage::{deal_damage, StepCtx};
use crate::events::Event;
use crate::grid::{cheb, ent_index_at, los, slam_area, walkable};
use crate::state::{GameState, Kind, Tile};

fn mage_index(g: &GameState) -> usize {
    g.entities
        .iter()
        .position(|e| e.kind.is_mage())
        .expect("法師應存在")
}

/// 朝法師走一格,避開牆/單位/尖刺/火;只走「更接近」的格(嚴格 `<`)。對應 JS `stepToward`。
fn step_toward(g: &mut GameState, idx: usize, ctx: &mut StepCtx) {
    let (mx, my) = {
        let m = g.mage();
        (m.x, m.y)
    };
    let (ex, ey) = (g.entities[idx].x, g.entities[idx].y);
    let mut best: Option<(i32, i32)> = None;
    let mut bd = cheb(ex, ey, mx, my);
    for dy in -1..=1 {
        for dx in -1..=1 {
            if dx == 0 && dy == 0 {
                continue;
            }
            let (x, y) = (ex + dx, ey + dy);
            if !walkable(g, x, y) || ent_index_at(g, x, y).is_some() {
                continue;
            }
            if g.tiles[y as usize][x as usize] == Tile::Spike || g.fire[y as usize][x as usize] > 0 {
                continue;
            }
            let d = cheb(x, y, mx, my);
            if d < bd {
                bd = d;
                best = Some((x, y));
            }
        }
    }
    if let Some((bx, by)) = best {
        let id = g.entities[idx].id;
        g.entities[idx].x = bx;
        g.entities[idx].y = by;
        ctx.events.push(Event::Moved {
            id,
            from: (ex, ey),
            to: (bx, by),
        });
    }
}

/// 單一敵人的一手行動。對應 JS `enemyAct`。被震暈則消耗一層暈、跳過此手。
pub fn enemy_act(g: &mut GameState, idx: usize, ctx: &mut StepCtx) {
    if g.entities[idx].stun > 0 {
        g.entities[idx].stun -= 1; // 被震暈,跳過一手
        return;
    }

    let (mx, my) = {
        let m = g.mage();
        (m.x, m.y)
    };
    let (ex, ey) = (g.entities[idx].x, g.entities[idx].y);

    match g.entities[idx].kind {
        Kind::Imp => {
            if cheb(ex, ey, mx, my) == 1 {
                let m = mage_index(g);
                deal_damage(g, m, IMP_DMG, ctx);
            } else {
                step_toward(g, idx, ctx);
            }
        }
        Kind::Eye => {
            if cheb(ex, ey, mx, my) <= EYE_RANGE && los(g, ex, ey, mx, my) {
                let m = mage_index(g);
                deal_damage(g, m, EYE_DMG, ctx);
            } else {
                step_toward(g, idx, ctx);
            }
        }
        Kind::Boss => {
            // 過熱(雙倍傷害窗口)只持續到 boss 下一手:此手一開始就清除。
            if g.entities[idx].exhausted {
                g.entities[idx].exhausted = false;
            }
            if g.entities[idx].pending_slam {
                // 砸下:法師站在任一預告格上才中。
                let hit = g.entities[idx]
                    .slam
                    .as_ref()
                    .map(|cells| cells.iter().any(|&(sx, sy)| sx == mx && sy == my))
                    .unwrap_or(false);
                if hit {
                    let m = mage_index(g);
                    deal_damage(g, m, BOSS_SLAM, ctx);
                }
                g.entities[idx].slam = None;
                g.entities[idx].pending_slam = false;
                g.entities[idx].exhausted = true; // 砸完過熱
            } else {
                // 預告:在法師當前位置布告砸擊範圍。
                let cells = slam_area(g, mx, my);
                g.entities[idx].slam = Some(cells);
                g.entities[idx].pending_slam = true;
            }
        }
        Kind::Mage => {} // 法師不走 enemy_act
    }
}

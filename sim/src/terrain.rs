//! 火 / 油 / 木牆地形 —— 已驗證的簡易 cellular automaton。
//!
//! 對應 `prototype/demo1.html`:`igniteOil`(271–276)、`fireTick`(278–289)。
//! 統一心智模型(速度規格 §8):**可燃 = 油 + 木**;火沿可燃物蔓延;石頭安全;牆擋視線。
//!
//! ⚠ 確定性高風險(B0 §D-4):
//! - `ignite_oil` 的 DFS **push/pop 順序**決定多目標受傷/死亡先後,而死亡會觸發
//!   「擊殺續加速」反過來改時間鏈 → **逐字複製** stack(`Vec::pop` = LIFO)與 dy,dx 展開序。
//! - `fire_tick` 的 **y,x 掃描序**同理照搬;蔓延前先快照 `active`(雙緩衝)= CA 正確性關鍵。
//! - `seen` 用 grid `bool`(不迭代,無 HashSet 迭代序問題)。

use crate::config::{FIRE_DOT, FIRE_DUR, WOOD_BURN_TICKS};
use crate::damage::{deal_damage, StepCtx};
use crate::grid::{ent_index_at, in_bounds};
use crate::state::{GameState, Tile};

/// 點燃 `(sx,sy)`:DFS flood 沿相連**油格**延燒,每個被點到的油格起火、其上敵人吃 DoT。
///
/// 對應 JS `igniteOil`。注意:**油點燃的瞬間不傷法師**(只傷敵人,JS 行 275 的 `kind!=="mage"`);
/// 法師受火傷只發生在 `fire_tick`(站在火格上)。
pub fn ignite_oil(g: &mut GameState, sx: i32, sy: i32, ctx: &mut StepCtx) {
    let w = g.w;
    // seen 只追蹤界內格;界外格在 JS 只是「標記後 continue」,對結果無觀察差異(見模組註)。
    let mut seen = vec![false; (g.w * g.h) as usize];
    let mut st: Vec<(i32, i32)> = vec![(sx, sy)];

    while let Some((cx, cy)) = st.pop() {
        if !in_bounds(g, cx, cy) {
            continue;
        }
        let k = (cy * w + cx) as usize;
        if seen[k] {
            continue;
        }
        seen[k] = true;

        if g.tiles[cy as usize][cx as usize] != Tile::Oil {
            continue;
        }
        g.fire[cy as usize][cx as usize] = FIRE_DUR;
        if let Some(idx) = ent_index_at(g, cx, cy) {
            if !g.entities[idx].kind.is_mage() {
                deal_damage(g, idx, FIRE_DOT, ctx);
            }
        }
        // 八方向鄰居入 stack(順序逐字對齊 JS:dy 外、dx 內,跳過 (0,0))。
        for dy in -1..=1 {
            for dx in -1..=1 {
                if dx != 0 || dy != 0 {
                    st.push((cx + dx, cy + dy));
                }
            }
        }
    }
}

/// 一個火 tick:木牆倒數崩塌 → 火格傷害其上單位 → 蔓延至鄰接油/木 → 火格倒數熄滅。
///
/// 對應 JS `fireTick`。四個階段的掃描序與快照時機**逐字保留**。
pub fn fire_tick(g: &mut GameState, ctx: &mut StepCtx) {
    let (w, h) = (g.w as usize, g.h as usize);

    // 1. 燃燒中木牆倒數;歸零 → 崩塌成地板並起火。
    for y in 0..h {
        for x in 0..w {
            if g.tiles[y][x] == Tile::WoodBurn {
                g.burn_t[y][x] -= 1;
                if g.burn_t[y][x] <= 0 {
                    g.tiles[y][x] = Tile::Floor;
                    g.fire[y][x] = FIRE_DUR;
                }
            }
        }
    }

    // 2. 快照當前所有火格(蔓延前先抓 = 本 tick 只蔓延一圈,CA 正確性關鍵)。
    let mut active: Vec<(i32, i32)> = Vec::new();
    for y in 0..h {
        for x in 0..w {
            if g.fire[y][x] > 0 {
                active.push((x as i32, y as i32));
            }
        }
    }

    // 3. 每個火格:傷害其上單位(含法師),並蔓延到鄰接油/木。
    for (fx, fy) in active {
        if let Some(idx) = ent_index_at(g, fx, fy) {
            deal_damage(g, idx, FIRE_DOT, ctx);
        }
        for dy in -1..=1 {
            for dx in -1..=1 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let (x, y) = (fx + dx, fy + dy);
                if !in_bounds(g, x, y) {
                    continue;
                }
                let (ux, uy) = (x as usize, y as usize);
                if g.tiles[uy][ux] == Tile::Oil && g.fire[uy][ux] == 0 {
                    g.fire[uy][ux] = FIRE_DUR;
                } else if g.tiles[uy][ux] == Tile::Wood {
                    g.tiles[uy][ux] = Tile::WoodBurn;
                    g.burn_t[uy][ux] = WOOD_BURN_TICKS;
                }
            }
        }
    }

    // 4. 火格倒數;熄滅後油格還原成地板。
    for y in 0..h {
        for x in 0..w {
            if g.fire[y][x] > 0 {
                g.fire[y][x] -= 1;
                if g.fire[y][x] == 0 && g.tiles[y][x] == Tile::Oil {
                    g.tiles[y][x] = Tile::Floor;
                }
            }
        }
    }
}

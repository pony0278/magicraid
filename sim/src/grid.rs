//! 格子謂詞 —— 純函式,無狀態變更。
//!
//! 對應 `prototype/demo1.html`:`inB`(238)、`cheb`(239)、`entAt`(240)、
//! `blocksMove`/`blocksSight`/`walkable`(241–243)。
//! 確定性:`ent_index_at` 走 entities 的 id 序(`position`),對應 JS `Array.find`(B0 §D-6)。

use crate::state::{GameState, Tile};

#[inline]
pub fn in_bounds(g: &GameState, x: i32, y: i32) -> bool {
    x >= 0 && y >= 0 && x < g.w && y < g.h
}

/// 切比雪夫距離(八方向步數)。對應 JS `cheb`。
#[inline]
pub fn cheb(ax: i32, ay: i32, bx: i32, by: i32) -> i32 {
    (ax - bx).abs().max((ay - by).abs())
}

/// 該格上第一個存活實體的索引。對應 JS `entAt`(entities.find,hp>0)。
///
/// entities 永遠 id 序保存 → `position` 回最低 id 的存活實體,與 JS 插入序一致。
pub fn ent_index_at(g: &GameState, x: i32, y: i32) -> Option<usize> {
    g.entities
        .iter()
        .position(|e| e.alive() && e.x == x && e.y == y)
}

/// 假設 `in_bounds` 已成立(呼叫端負責),讀該格 tile。
#[inline]
fn tile_at(g: &GameState, x: i32, y: i32) -> Tile {
    g.tiles[y as usize][x as usize]
}

/// 擋移動:石牆 / 木牆 / 燃燒中木牆。對應 JS `blocksMove`。
#[inline]
pub fn blocks_move(g: &GameState, x: i32, y: i32) -> bool {
    matches!(tile_at(g, x, y), Tile::Wall | Tile::Wood | Tile::WoodBurn)
}

/// 擋視線/射線:同 `blocks_move`(石/木/燃燒木皆為掩體)。對應 JS `blocksSight`。
#[inline]
pub fn blocks_sight(g: &GameState, x: i32, y: i32) -> bool {
    matches!(tile_at(g, x, y), Tile::Wall | Tile::Wood | Tile::WoodBurn)
}

/// 可走:在界內且不擋移動。對應 JS `walkable`。
#[inline]
pub fn walkable(g: &GameState, x: i32, y: i32) -> bool {
    in_bounds(g, x, y) && !blocks_move(g, x, y)
}

/// 烈焰術範圍:`(cx,cy)` 的加號(中心 + 四正交)中在界內的格。對應 JS `heavyArea`(行 506)。
/// 順序逐字對齊 JS:N, W, C, E, S。
pub fn heavy_area(g: &GameState, cx: i32, cy: i32) -> Vec<(i32, i32)> {
    let mut a = Vec::new();
    for dy in -1i32..=1 {
        for dx in -1i32..=1 {
            if dx.abs() + dy.abs() > 1 {
                continue;
            }
            let (x, y) = (cx + dx, cy + dy);
            if in_bounds(g, x, y) {
                a.push((x, y));
            }
        }
    }
    a
}

/// 砸擊範圍:`(cx,cy)` 周圍 3×3(含中心)中非牆的格。對應 JS `slamArea`(行 247)。
pub fn slam_area(g: &GameState, cx: i32, cy: i32) -> Vec<(i32, i32)> {
    let mut a = Vec::new();
    for dy in -1..=1 {
        for dx in -1..=1 {
            let (x, y) = (cx + dx, cy + dy);
            if in_bounds(g, x, y) && g.tiles[y as usize][x as usize] != Tile::Wall {
                a.push((x, y));
            }
        }
    }
    a
}

/// 半進位整數除法:`round(num/den)`,den>0、num≥0(線段內插點座標恆非負)。
///
/// 對應 JS `Math.round`(.5 向 +∞)。`round(n/d) = floor(n/d + 1/2) = (2n+d)/(2d)`。
#[inline]
fn round_div(num: i32, den: i32) -> i32 {
    (2 * num + den) / (2 * den)
}

/// 視線(LoS):`(ax,ay)`→`(bx,by)` 之間有無阻擋。對應 JS `los`(行 249–253)。
///
/// **整數化(B0 §D-2)**:JS 用 `Math.round` 取樣 `steps-1` 個內插點,float 在邊界會漂。
/// 改用整數半進位 `round_div` 取**完全相同**的取樣格 → 去 float 且與 JS 行為一致(對拍不漂)。
/// 內插點恆落在兩端點的包圍盒內,故必在界內,直接讀 tile 安全。
pub fn los(g: &GameState, ax: i32, ay: i32, bx: i32, by: i32) -> bool {
    let dx = bx - ax;
    let dy = by - ay;
    let steps = dx.abs().max(dy.abs());
    if steps == 0 {
        return true;
    }
    for i in 1..steps {
        let x = round_div(ax * steps + dx * i, steps);
        let y = round_div(ay * steps + dy * i, steps);
        if blocks_sight(g, x, y) {
            return false;
        }
    }
    true
}

/// 投射物攔截:`(ax,ay)`→`(bx,by)` 視線上的第一個單位(不含起點格、含終點)。
///
/// 取樣與 `los` 完全相同 → 攔截者必落在同一條 LoS 線上(行為一致、確定性)。
/// 用於魔法彈等直線投射:擋在前面的單位先中彈(身體當掩體)。遇阻擋牆則中止(回 `None`)。
pub fn first_unit_on_ray(g: &GameState, ax: i32, ay: i32, bx: i32, by: i32) -> Option<usize> {
    let dx = bx - ax;
    let dy = by - ay;
    let steps = dx.abs().max(dy.abs());
    if steps == 0 {
        return None;
    }
    for i in 1..=steps {
        let x = round_div(ax * steps + dx * i, steps);
        let y = round_div(ay * steps + dy * i, steps);
        if let Some(idx) = ent_index_at(g, x, y) {
            return Some(idx);
        }
        if blocks_sight(g, x, y) {
            return None;
        }
    }
    None
}

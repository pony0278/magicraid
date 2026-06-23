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

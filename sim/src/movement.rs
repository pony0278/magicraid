//! 位移招式與尋路。
//!
//! 對應 `prototype/demo1.html`:`doPush`(437–445)、`shoveDir`(447–454)、`doPull`(455–467)、
//! `moveMageTo`(470–475)、`walkBrake`(477–482)、`findPath`(484–505)。
//!
//! ⚠ 確定性(B0 §D-3):A\* 用**整數成本**(正交 10 / 對角 14,去掉 1e-9 epsilon)。
//! 整數成本恰為 JS 浮點成本(1.0/1.4)的 10 倍、啟發值(Chebyshev)亦 ×10 → f/g 比較完全等價,
//! 路徑選擇與 JS 一致;且整數無 `f===f` 的浮點誤判,這版才是確定性正本。
//! **務必保留**:鄰居展開序(dy,dx 各 −1..=1)、open 選取 tiebreak(f 小 → g 大 → 插入序)。

use crate::config::{FIRE_DOT, HASTE_GRANT, PUSH_CRASH, SPIKE_DMG};
use crate::damage::{deal_damage, StepCtx};
use crate::events::{Cause, Event};
use crate::grid::{ent_index_at, in_bounds, walkable};
use crate::state::{GameState, Kind, Tile};

/// `Math.sign` 的整數版:-1 / 0 / 1。
#[inline]
fn sign(n: i32) -> i32 {
    (n > 0) as i32 - (n < 0) as i32
}

/// 敵人被推/震/勾到落點後的危險格結算:**尖刺優先,否則火**(對應 JS 推/震/勾,單一 else-if)。
/// 回傳 `true` 表示落點是尖刺(供 `do_pull` 判斷是否中止連拉)。
fn hazard_on_pushed(g: &mut GameState, idx: usize, x: i32, y: i32, ctx: &mut StepCtx) -> bool {
    if g.tiles[y as usize][x as usize] == Tile::Spike {
        deal_damage(g, idx, SPIKE_DMG, Cause::HazardPush, ctx);
        true
    } else if g.fire[y as usize][x as usize] > 0 {
        deal_damage(g, idx, FIRE_DOT, Cause::HazardPush, ctx);
        false
    } else {
        false
    }
}

/// 推:把敵人往「遠離法師」方向推一格。對應 JS `doPush`。
///
/// 撞牆/撞單位 → 撞擊傷(`PUSH_CRASH`)+(推★★)撞暈一手;否則移動 + 落點危險格結算。
pub fn do_push(g: &mut GameState, idx: usize, ctx: &mut StepCtx) {
    if g.entities[idx].kind == Kind::Boss {
        return; // 魔像太重,推不動
    }
    let (mx, my) = {
        let m = g.mage();
        (m.x, m.y)
    };
    let (ex, ey) = (g.entities[idx].x, g.entities[idx].y);
    let (nx, ny) = (ex + sign(ex - mx), ey + sign(ey - my));

    if !walkable(g, nx, ny) || ent_index_at(g, nx, ny).is_some() {
        deal_damage(g, idx, PUSH_CRASH, Cause::Crash, ctx);
        if ctx.tiers.of("push") >= 2 && g.entities[idx].alive() {
            g.entities[idx].stun = 1; // 推★★:撞擊 → 暈一手
            ctx.events.push(Event::Stunned {
                id: g.entities[idx].id,
            });
        }
        return;
    }
    move_entity(g, idx, nx, ny, ctx);
    hazard_on_pushed(g, idx, nx, ny, ctx);
}

/// 把單位往指定方向震一格(撞阻擋則停)。對應 JS `shoveDir`。供烈焰術★★ 震退用。
pub fn shove_dir(g: &mut GameState, idx: usize, dx: i32, dy: i32, ctx: &mut StepCtx) {
    if dx == 0 && dy == 0 {
        return;
    }
    let (nx, ny) = (g.entities[idx].x + dx, g.entities[idx].y + dy);
    if !walkable(g, nx, ny) || ent_index_at(g, nx, ny).is_some() {
        return;
    }
    move_entity(g, idx, nx, ny, ctx);
    hazard_on_pushed(g, idx, nx, ny, ctx);
}

/// 勾索:把敵人往法師方向拉最多 2 格(撞阻擋/到法師前停;落點危險格則中止)。對應 JS `doPull`。
pub fn do_pull(g: &mut GameState, idx: usize, ctx: &mut StepCtx) {
    if g.entities[idx].kind == Kind::Boss {
        return; // 魔像太重,拉不動
    }
    for _ in 0..2 {
        let (mx, my) = {
            let m = g.mage();
            (m.x, m.y)
        };
        let (ex, ey) = (g.entities[idx].x, g.entities[idx].y);
        let (nx, ny) = (ex + sign(mx - ex), ey + sign(my - ey));
        if nx == mx && ny == my {
            break; // 不能拉到法師身上
        }
        if !walkable(g, nx, ny) || ent_index_at(g, nx, ny).is_some() {
            break; // 撞阻擋停住
        }
        move_entity(g, idx, nx, ny, ctx);
        // 落點是尖刺或火 → 受傷並中止連拉(JS 兩個 if 皆 break)。
        if g.tiles[ny as usize][nx as usize] == Tile::Spike {
            deal_damage(g, idx, SPIKE_DMG, Cause::HazardPush, ctx);
            break;
        }
        if g.fire[ny as usize][nx as usize] > 0 {
            deal_damage(g, idx, FIRE_DOT, Cause::HazardPush, ctx);
            break;
        }
    }
    if ctx.tiers.of("hook") >= 2 && g.entities[idx].alive() {
        g.entities[idx].stun = 1; // 勾索★★:落點定身一手
        ctx.events.push(Event::Stunned {
            id: g.entities[idx].id,
        });
    }
}

/// 移動實體到 `(x,y)` 並發 `Moved` 事件(內部共用)。不做危險格結算。
fn move_entity(g: &mut GameState, idx: usize, x: i32, y: i32, ctx: &mut StepCtx) {
    let id = g.entities[idx].id;
    let from = (g.entities[idx].x, g.entities[idx].y);
    g.entities[idx].x = x;
    g.entities[idx].y = y;
    ctx.events.push(Event::Moved {
        id,
        from,
        to: (x, y),
    });
}

/// 法師移動到 `(x,y)` 的單格落地效果。對應 JS `moveMageTo`。
///
/// 注意:法師踩到危險格是**尖刺與火各自結算**(兩個獨立 if,非 else-if),與敵人被推不同。
/// 踩到急速符文 → 清格 + 獲得加速。
pub fn move_mage_to(g: &mut GameState, x: i32, y: i32, ctx: &mut StepCtx) {
    let mage_idx = g
        .entities
        .iter()
        .position(|e| e.kind.is_mage())
        .expect("法師應存在");
    move_entity(g, mage_idx, x, y, ctx);
    if g.tiles[y as usize][x as usize] == Tile::Spike {
        deal_damage(g, mage_idx, SPIKE_DMG, Cause::Other, ctx); // 法師自傷,不計入擊殺歸因
    }
    if g.fire[y as usize][x as usize] > 0 {
        deal_damage(g, mage_idx, FIRE_DOT, Cause::Other, ctx);
    }
    if g.tiles[y as usize][x as usize] == Tile::Rune {
        g.tiles[y as usize][x as usize] = Tile::Floor;
        let mage = g.mage_mut();
        mage.haste_turns = HASTE_GRANT;
        ctx.events.push(Event::HasteGained { id: mage.id });
    }
}

/// ZOC 煞車:auto-walk 是否該停下、把控制權交回玩家。對應 JS `walkBrake`。
///
/// `next` = 即將踏入的格(`None` 表示只檢查當前)。任一條成立即停:
/// 這趟被打過、已被敵人貼住、下一步會踏進敵人控制區。
pub fn walk_brake(g: &GameState, next: Option<(i32, i32)>, ctx: &StepCtx) -> bool {
    if ctx.mage_hurt {
        return true;
    }
    let (mx, my) = {
        let m = g.mage();
        (m.x, m.y)
    };
    let adj_to = |x: i32, y: i32| {
        g.entities
            .iter()
            .any(|e| e.alive() && !e.kind.is_mage() && crate::grid::cheb(e.x, e.y, x, y) == 1)
    };
    if adj_to(mx, my) {
        return true;
    }
    if let Some((nx, ny)) = next {
        if adj_to(nx, ny) {
            return true;
        }
    }
    false
}

/// A\* 尋路節點(open 表元素)。
struct Node {
    x: i32,
    y: i32,
    g: i64,
    f: i64,
}

/// 8 向 A\*。對應 JS `findPath`,改**整數成本**(B0 §D-3)。
///
/// 回傳從起點下一格到終點的路徑(不含起點、含終點);起終同格回 `None`、無路回 `None`。
/// 繞開尖刺/火/敵人(終點除外)、穿不過牆。
pub fn find_path(g: &GameState, sx: i32, sy: i32, tx: i32, ty: i32) -> Option<Vec<(i32, i32)>> {
    if sx == tx && sy == ty {
        return None;
    }
    let w = g.w;
    let idx = |x: i32, y: i32| (y * w + x) as usize;
    let n = (g.w * g.h) as usize;
    // 啟發值 = Chebyshev × 10(對齊整數成本;admissible)。
    let h = |x: i32, y: i32| (x - tx).abs().max((y - ty).abs()) as i64 * 10;

    let mut open: Vec<Node> = vec![Node {
        x: sx,
        y: sy,
        g: 0,
        f: h(sx, sy),
    }];
    let mut gscore = vec![i64::MAX; n];
    let mut prev: Vec<Option<(i32, i32)>> = vec![None; n];
    let mut closed = vec![false; n];
    gscore[idx(sx, sy)] = 0;

    while !open.is_empty() {
        // 選 f 最小;同 f 取 g 大;再同取插入序最前(bi 從 0、僅嚴格更佳才更新)。
        let mut bi = 0;
        for i in 1..open.len() {
            let (o, b) = (&open[i], &open[bi]);
            if o.f < b.f || (o.f == b.f && o.g > b.g) {
                bi = i;
            }
        }
        let c = open.remove(bi);
        let ck = idx(c.x, c.y);
        if closed[ck] {
            continue;
        }
        closed[ck] = true;

        if c.x == tx && c.y == ty {
            let mut path = Vec::new();
            let mut k = (c.x, c.y);
            while k != (sx, sy) {
                path.push(k);
                k = prev[idx(k.0, k.1)].expect("prev 鏈應可回溯到起點");
            }
            path.reverse();
            return Some(path);
        }

        for dy in -1..=1 {
            for dx in -1..=1 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let (x, y) = (c.x + dx, c.y + dy);
                if !in_bounds(g, x, y) {
                    continue;
                }
                let kk = idx(x, y);
                if closed[kk] || !walkable(g, x, y) {
                    continue;
                }
                let is_dest = x == tx && y == ty;
                if ent_index_at(g, x, y).is_some() && !is_dest {
                    continue;
                }
                let blocked_hazard =
                    g.tiles[y as usize][x as usize] == Tile::Spike || g.fire[y as usize][x as usize] > 0;
                if blocked_hazard && !is_dest {
                    continue;
                }
                let step = if dx != 0 && dy != 0 { 14 } else { 10 };
                let ng = c.g + step;
                if ng < gscore[kk] {
                    gscore[kk] = ng;
                    prev[kk] = Some((c.x, c.y));
                    open.push(Node {
                        x,
                        y,
                        g: ng,
                        f: ng + h(x, y),
                    });
                }
            }
        }
    }
    None
}

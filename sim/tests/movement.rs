//! 位移招式 / 整數 A\* / 整數 LoS 的正確性 + 確定性。

use magicraid_sim::config::{IMP_HP, MAGE_HP, PUSH_CRASH, SPIKE_DMG};
use magicraid_sim::damage::StepCtx;
use magicraid_sim::grid::los;
use magicraid_sim::movement::{do_pull, do_push, find_path, move_mage_to, walk_brake};
use magicraid_sim::state::{Entity, GameState, Kind, Tile};

/// 字元地圖建狀態:`.`地板 `#`牆 `~`油 `W`木 `s`尖刺 `H`符文 `@`法師 `o`小鬼 `B`魔像。
fn make(map: &[&str]) -> GameState {
    let h = map.len() as i32;
    let w = map[0].len() as i32;
    let mut tiles = Vec::new();
    let mut entities = Vec::new();
    let mut next_id = 1u32;
    for (y, row) in map.iter().enumerate() {
        let mut trow = Vec::new();
        for (x, c) in row.chars().enumerate() {
            trow.push(match c {
                '#' => Tile::Wall,
                '~' => Tile::Oil,
                'W' => Tile::Wood,
                's' => Tile::Spike,
                'H' => Tile::Rune,
                _ => Tile::Floor,
            });
            let (xi, yi) = (x as i32, y as i32);
            match c {
                '@' => entities.push(Entity::new(0, Kind::Mage, xi, yi)),
                'o' => {
                    entities.push(Entity::new(next_id, Kind::Imp, xi, yi));
                    next_id += 1;
                }
                'B' => {
                    entities.push(Entity::new(next_id, Kind::Boss, xi, yi));
                    next_id += 1;
                }
                _ => {}
            }
        }
        tiles.push(trow);
    }
    entities.sort_by_key(|e| e.id);
    GameState {
        w,
        h,
        fire: vec![vec![0; w as usize]; h as usize],
        burn_t: vec![vec![0; w as usize]; h as usize],
        tiles,
        entities,
        room_idx: 0,
    }
}

fn imp_idx(g: &GameState) -> usize {
    g.entities.iter().position(|e| e.kind == Kind::Imp).unwrap()
}
fn ent(g: &GameState, idx: usize) -> &Entity {
    &g.entities[idx]
}

#[test]
fn push_into_spike_damages() {
    // 法師(0,0)、小鬼(1,0)、尖刺(2,0):推 → 小鬼撞上尖刺。
    let mut g = make(&["@os"]);
    let mut ctx = StepCtx::new();
    let i = imp_idx(&g);
    // IMP_HP(5) < SPIKE_DMG(6) → hp 夾到 0(被解除)。編譯期確認前提。
    const _: () = assert!(IMP_HP < SPIKE_DMG);
    do_push(&mut g, i, &mut ctx);
    assert_eq!(ent(&g, i).x, 2, "小鬼應被推到尖刺格");
    assert_eq!(ent(&g, i).hp, 0, "應吃尖刺傷並被解除");
}

#[test]
fn push_into_wall_crashes() {
    // 小鬼背靠牆:推不動 → 撞擊傷。地圖 @o#:小鬼(1,0)後面是牆(2,0)。
    let mut g = make(&["@o#"]);
    let mut ctx = StepCtx::new();
    let i = imp_idx(&g);
    do_push(&mut g, i, &mut ctx);
    assert_eq!(ent(&g, i).x, 1, "撞牆不位移");
    assert_eq!(ent(&g, i).hp, IMP_HP - PUSH_CRASH, "應吃撞擊傷");
}

#[test]
fn push_star_star_stuns() {
    // 推★★:撞牆撞擊後敵人暈一手(需 tier push≥2)。
    let mut g = make(&["@o#"]);
    let mut ctx = StepCtx::new();
    ctx.tiers.set("push", 2);
    let i = imp_idx(&g);
    do_push(&mut g, i, &mut ctx);
    assert_eq!(ent(&g, i).stun, 1, "推★★ 應使撞擊目標暈一手");
}

#[test]
fn boss_is_immovable() {
    // 魔像推不動。
    let mut g = make(&["@B."]);
    let mut ctx = StepCtx::new();
    let b = g.entities.iter().position(|e| e.kind == Kind::Boss).unwrap();
    let before = (ent(&g, b).x, ent(&g, b).hp);
    do_push(&mut g, b, &mut ctx);
    assert_eq!((ent(&g, b).x, ent(&g, b).hp), before, "魔像不應被推動或受傷");
}

#[test]
fn pull_drags_two_steps_toward_mage() {
    // 勾索:小鬼在 (4,0)、法師 (0,0),拉 2 格 → 小鬼到 (2,0)。
    let mut g = make(&["@...o"]);
    let mut ctx = StepCtx::new();
    let i = imp_idx(&g);
    do_pull(&mut g, i, &mut ctx);
    assert_eq!(ent(&g, i).x, 2, "應被拉近 2 格");
}

#[test]
fn mage_steps_on_rune_gains_haste() {
    // 法師踩急速符文 → 清格 + 獲得加速。
    let mut g = make(&["@H"]);
    let mut ctx = StepCtx::new();
    move_mage_to(&mut g, 1, 0, &mut ctx);
    assert_eq!(g.tiles[0][1], Tile::Floor, "符文應被消耗");
    assert!(g.mage().haste_turns > 0, "應獲得加速");
}

#[test]
fn mage_on_burning_spike_takes_both() {
    // 法師踩到「著火的尖刺」→ 尖刺與火各自結算(兩段傷,與敵人被推的 else-if 不同)。
    let mut g = make(&["@s"]);
    g.fire[0][1] = 2;
    let mut ctx = StepCtx::new();
    move_mage_to(&mut g, 1, 0, &mut ctx);
    let expected = MAGE_HP - SPIKE_DMG - magicraid_sim::config::FIRE_DOT;
    assert_eq!(g.mage().hp, expected, "法師應同時吃尖刺與火傷");
}

#[test]
fn walk_brake_stops_when_hurt_or_adjacent() {
    let g = make(&["@..o"]);
    let mut ctx = StepCtx::new();
    // 不貼身、沒受傷 → 不煞車。
    assert!(!walk_brake(&g, Some((1, 0)), &ctx));
    // 受傷 → 煞車。
    ctx.mage_hurt = true;
    assert!(walk_brake(&g, Some((1, 0)), &ctx));
    // 下一步會貼到敵人(敵人在 (3,0),next=(2,0) 與其相鄰)→ 煞車。
    ctx.mage_hurt = false;
    assert!(walk_brake(&g, Some((2, 0)), &ctx));
}

#[test]
fn los_blocked_by_wall() {
    // 中間一道牆擋住直線視線。
    let g = make(&["@#o"]);
    assert!(!los(&g, 0, 0, 2, 0), "牆應擋住視線");
    let g2 = make(&["@.o"]);
    assert!(los(&g2, 0, 0, 2, 0), "空地應有視線");
}

#[test]
fn find_path_avoids_spike_via_second_row() {
    // 兩列走廊,row0 中間有尖刺(非終點)→ 路徑應繞到 row1 避開尖刺格。
    let g = make(&["@.s.o", "....."]);
    let path = find_path(&g, 0, 0, 4, 0).expect("應有路(可繞 row1)");
    assert!(
        !path.contains(&(2, 0)),
        "路徑應繞開尖刺(2,0),實得 {path:?}"
    );
    assert_eq!(*path.last().unwrap(), (4, 0), "終點應為目標");
}

#[test]
fn find_path_none_when_same_cell() {
    let g = make(&["@.."]);
    assert!(find_path(&g, 0, 0, 0, 0).is_none());
}

#[test]
fn movement_replay_is_bit_identical() {
    // 同地圖、同一串操作 → 實體位置/hp 與 event 流 bit 一致。
    let map = &["@.o.s", ".....", "o..s.", "....o"];
    let run = || {
        let mut g = make(map);
        let mut ctx = StepCtx::new();
        ctx.tiers.set("push", 2);
        ctx.tiers.set("hook", 2);
        // 對每隻小鬼依 id 序各推一次、再拉一次。
        let imp_ids: Vec<u32> = g
            .entities
            .iter()
            .filter(|e| e.kind == Kind::Imp)
            .map(|e| e.id)
            .collect();
        for id in imp_ids {
            if let Some(i) = g.entities.iter().position(|e| e.id == id && e.alive()) {
                do_push(&mut g, i, &mut ctx);
            }
            if let Some(i) = g.entities.iter().position(|e| e.id == id && e.alive()) {
                do_pull(&mut g, i, &mut ctx);
            }
        }
        let pos: Vec<(i32, i32, i32)> = g.entities.iter().map(|e| (e.x, e.y, e.hp)).collect();
        (pos, ctx.events)
    };
    assert_eq!(run(), run(), "移動回放必須 bit 一致");
}

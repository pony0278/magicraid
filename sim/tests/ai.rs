//! 敵人 AI 正確性 + 確定性:小鬼貼臉/走位、符文眼視線射擊、魔像砸擊過熱循環。

use magicraid_sim::ai::enemy_act;
use magicraid_sim::config::{BOSS_SLAM, EYE_DMG, IMP_DMG, MAGE_HP};
use magicraid_sim::damage::StepCtx;
use magicraid_sim::grid::{cheb, slam_area};
use magicraid_sim::state::{Channel, Entity, GameState, Kind, Tile};

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
                'e' => {
                    entities.push(Entity::new(next_id, Kind::Eye, xi, yi));
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

fn idx_of(g: &GameState, kind: Kind) -> usize {
    g.entities.iter().position(|e| e.kind == kind).unwrap()
}

#[test]
fn imp_steps_toward_mage() {
    let mut g = make(&["@...o"]); // 法師(0,0)、小鬼(4,0)
    let mut ctx = StepCtx::new();
    let i = idx_of(&g, Kind::Imp);
    let before = cheb(g.entities[i].x, g.entities[i].y, 0, 0);
    enemy_act(&mut g, i, &mut ctx);
    let after = cheb(g.entities[i].x, g.entities[i].y, 0, 0);
    assert!(after < before, "小鬼應更靠近法師");
}

#[test]
fn imp_attacks_when_adjacent() {
    let mut g = make(&["@o"]); // 相鄰
    let mut ctx = StepCtx::new();
    let i = idx_of(&g, Kind::Imp);
    enemy_act(&mut g, i, &mut ctx);
    assert_eq!(g.mage().hp, MAGE_HP - IMP_DMG, "貼臉應攻擊法師");
}

#[test]
fn imp_avoids_spike_when_stepping() {
    // 小鬼在 (2,0),法師 (0,0),(1,0) 是尖刺;不能直接走尖刺,改繞 row1。
    let mut g = make(&["@so", "..."]);
    let mut ctx = StepCtx::new();
    let i = idx_of(&g, Kind::Imp);
    enemy_act(&mut g, i, &mut ctx);
    let (x, y) = (g.entities[i].x, g.entities[i].y);
    assert_ne!((x, y), (1, 0), "小鬼不應踏上尖刺格");
}

#[test]
fn eye_shoots_with_line_of_sight() {
    let mut g = make(&["@..e"]); // 法師與符文眼同列、無遮擋、距離 3 ≤ range 4
    let mut ctx = StepCtx::new();
    let i = idx_of(&g, Kind::Eye);
    enemy_act(&mut g, i, &mut ctx);
    assert_eq!(g.mage().hp, MAGE_HP - EYE_DMG, "有視線應射擊");
}

#[test]
fn eye_without_los_steps_instead() {
    // 牆擋住視線 → 不射擊,改走位靠近。
    let mut g = make(&["@#.e"]);
    let mut ctx = StepCtx::new();
    let i = idx_of(&g, Kind::Eye);
    let before = (g.entities[i].x, g.entities[i].y);
    enemy_act(&mut g, i, &mut ctx);
    assert_eq!(g.mage().hp, MAGE_HP, "無視線不應造成傷害");
    assert_ne!((g.entities[i].x, g.entities[i].y), before, "應改為走位");
}

#[test]
fn stun_skips_turn_and_decrements() {
    let mut g = make(&["@o"]);
    let mut ctx = StepCtx::new();
    let i = idx_of(&g, Kind::Imp);
    g.entities[i].stun = 1;
    enemy_act(&mut g, i, &mut ctx);
    assert_eq!(g.mage().hp, MAGE_HP, "被暈時不應攻擊");
    assert_eq!(g.entities[i].stun, 0, "暈層應遞減");
}

#[test]
fn boss_telegraph_then_slam_cycle() {
    // 魔像(2,0)、法師(0,0)。boss 初始 pending_slam=true 但 slam 未 arm(None)。
    let mut g = make(&["@.B", "..."]);
    let mut ctx = StepCtx::new();
    let b = idx_of(&g, Kind::Boss);

    // 第 1 手:pending 但 slam=None → 砸空、進入過熱、轉成待預告。
    enemy_act(&mut g, b, &mut ctx);
    assert_eq!(g.mage().hp, MAGE_HP, "未 arm 的砸擊應落空");
    assert!(g.entities[b].exhausted, "砸完應過熱");
    assert!(!g.entities[b].pending_slam);

    // 第 2 手:清過熱 + 在法師當前位置布告砸擊範圍。
    enemy_act(&mut g, b, &mut ctx);
    assert!(!g.entities[b].exhausted, "下一手應清除過熱");
    assert!(g.entities[b].pending_slam, "應布告新預告");
    let cells = g.entities[b].slam.clone().expect("應有預告格");
    assert_eq!(cells, slam_area(&g, 0, 0), "預告應在法師位置周圍");

    // 第 3 手:法師站在預告格內(法師在 (0,0),預告含 (0,0))→ 砸中。
    enemy_act(&mut g, b, &mut ctx);
    assert_eq!(g.mage().hp, MAGE_HP - BOSS_SLAM, "站在預告格應被砸中");
    assert!(g.entities[b].exhausted, "砸完再次過熱");
}

#[test]
fn boss_slam_misses_when_mage_steps_out() {
    // arm 一個只涵蓋 (0,0) 周邊的預告,法師移到預告外 → 砸空。
    let mut g = make(&["@....", "....."]);
    let mut ctx = StepCtx::new();
    let b_id = 9;
    g.entities.push(Entity::new(b_id, Kind::Boss, 4, 0));
    g.entities.sort_by_key(|e| e.id);
    let b = g.entities.iter().position(|e| e.id == b_id).unwrap();
    // 在 (0,0) 布告,然後把法師移到 (4,1)(預告外)。
    g.entities[b].slam = Some(slam_area(&g, 0, 0));
    g.entities[b].pending_slam = true;
    let m = g.entities.iter().position(|e| e.kind.is_mage()).unwrap();
    g.entities[m].x = 4;
    g.entities[m].y = 1;
    enemy_act(&mut g, b, &mut ctx);
    assert_eq!(g.mage().hp, MAGE_HP, "走出預告格應躲掉砸擊");
}

#[test]
fn imp_attack_interrupts_mage_channel() {
    // 法師蓄力中被小鬼貼臉打 → channel 標記打斷(deal_damage 內)。
    let mut g = make(&["@o"]);
    let m = g.entities.iter().position(|e| e.kind.is_mage()).unwrap();
    g.entities[m].channel = Some(Channel {
        spell: "heavy",
        ready: false,
        interrupted: false,
    });
    let mut ctx = StepCtx::new();
    let i = idx_of(&g, Kind::Imp);
    enemy_act(&mut g, i, &mut ctx);
    assert!(
        g.mage().channel.as_ref().unwrap().interrupted,
        "蓄力中受傷應被打斷"
    );
}

#[test]
fn ai_replay_is_bit_identical() {
    // 多敵混合,每隻依 id 序各行動 3 手,跑兩次比對位置/hp/event。
    let map = &["@....e", ".o....", "....o.", "..B..."];
    let run = || {
        let mut g = make(map);
        let mut ctx = StepCtx::new();
        for _ in 0..3 {
            let ids: Vec<u32> = g
                .entities
                .iter()
                .filter(|e| !e.kind.is_mage())
                .map(|e| e.id)
                .collect();
            for id in ids {
                if let Some(i) = g.entities.iter().position(|e| e.id == id && e.alive()) {
                    enemy_act(&mut g, i, &mut ctx);
                }
            }
        }
        let snap: Vec<(u32, i32, i32, i32)> =
            g.entities.iter().map(|e| (e.id, e.x, e.y, e.hp)).collect();
        (snap, ctx.events)
    };
    assert_eq!(run(), run(), "AI 回放必須 bit 一致");
}

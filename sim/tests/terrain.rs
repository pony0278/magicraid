//! 地形(火/油/木牆 CA)正確性 + 確定性驗證。
//!
//! 對應 `prototype/demo1.html` `igniteOil` / `fireTick` 的行為,並驗「同輸入兩次 bit 一致」。

use magicraid_sim::config::{FIRE_DOT, FIRE_DUR, WOOD_BURN_TICKS};
use magicraid_sim::damage::StepCtx;
use magicraid_sim::state::{Entity, GameState, Kind, Tile};
use magicraid_sim::terrain::{fire_tick, ignite_oil};

/// 用一張字元地圖建狀態。`.`=地板 `#`=牆 `~`=油 `W`=木 `o`=小鬼 `@`=法師。
fn make(map: &[&str]) -> GameState {
    let h = map.len() as i32;
    let w = map[0].len() as i32;
    let mut tiles = Vec::new();
    let mut entities = Vec::new();
    let mut next_id = 1u32;
    for (y, row) in map.iter().enumerate() {
        let mut trow = Vec::new();
        for (x, c) in row.chars().enumerate() {
            let t = match c {
                '#' => Tile::Wall,
                '~' => Tile::Oil,
                'W' => Tile::Wood,
                _ => Tile::Floor,
            };
            trow.push(t);
            match c {
                '@' => entities.push(Entity::new(0, Kind::Mage, x as i32, y as i32)),
                'o' => {
                    entities.push(Entity::new(next_id, Kind::Imp, x as i32, y as i32));
                    next_id += 1;
                }
                _ => {}
            }
        }
        tiles.push(trow);
    }
    // entities 維持 id 序:法師(id 0)若存在排最前。
    entities.sort_by_key(|e| e.id);
    GameState {
        w,
        h,
        fire: vec![vec![0; w as usize]; h as usize],
        burn_t: vec![vec![0; w as usize]; h as usize],
        tiles,
        entities,
        room_idx: 0,
        core: None,
    }
}

fn hp_of(g: &GameState, id: u32) -> i32 {
    g.entities.iter().find(|e| e.id == id).unwrap().hp
}

#[test]
fn fire_floods_along_connected_oil() {
    // 一排相連油,點一端 → 整排起火。
    let mut g = make(&["~~~~"]);
    let mut ctx = StepCtx::new();
    ignite_oil(&mut g, 0, 0, &mut ctx);
    for x in 0..4 {
        assert_eq!(g.fire[0][x], FIRE_DUR, "油格 {x} 應起火");
    }
}

#[test]
fn ignite_oil_burns_enemy_not_mage() {
    // 敵人站油上點燃 → 吃 DoT;法師站油上點燃 → 不受傷(JS 行 275 kind!=="mage")。
    // 三格全油,imp(id1)站左、mage(id0)站右,從左端點火。
    let mut g = make(&["~~~"]);
    g.entities.push(Entity::new(1, Kind::Imp, 0, 0));
    g.entities.push(Entity::new(0, Kind::Mage, 2, 0));
    g.entities.sort_by_key(|e| e.id);
    let mut ctx = StepCtx::new();
    ignite_oil(&mut g, 0, 0, &mut ctx);
    assert_eq!(hp_of(&g, 1), magicraid_sim::config::IMP_HP - FIRE_DOT, "小鬼應吃火傷");
    assert_eq!(hp_of(&g, 0), magicraid_sim::config::MAGE_HP, "法師點油瞬間不受傷");
}

#[test]
fn fire_spreads_to_adjacent_oil_over_a_tick() {
    // 火格旁的油,經一個 fire_tick 被點燃。
    let mut g = make(&[".~"]);
    g.fire[0][0] = FIRE_DUR; // (0,0) 起火
    let mut ctx = StepCtx::new();
    fire_tick(&mut g, &mut ctx);
    assert!(g.fire[0][1] > 0, "鄰接油格應被點燃");
}

#[test]
fn wood_ignites_then_collapses_to_fire() {
    // 火格旁木牆:tick1 → 燃燒中(woodburn);tick2 → 崩塌成地板 + 火格。
    let mut g = make(&[".W"]);
    g.fire[0][0] = FIRE_DUR;
    let mut ctx = StepCtx::new();

    fire_tick(&mut g, &mut ctx);
    assert_eq!(g.tiles[0][1], Tile::WoodBurn, "木牆應進入燃燒中");
    assert_eq!(g.burn_t[0][1], WOOD_BURN_TICKS);

    fire_tick(&mut g, &mut ctx);
    assert_eq!(g.tiles[0][1], Tile::Floor, "燃燒木牆應崩塌成地板");
    assert!(g.fire[0][1] > 0, "崩塌處應起火");
}

#[test]
fn fire_extinguishes_and_oil_reverts() {
    // 油格起火 FIRE_DUR=2:跑 dur 次 fire_tick 後熄滅、油還原成地板。
    let mut g = make(&["~"]);
    g.fire[0][0] = FIRE_DUR;
    let mut ctx = StepCtx::new();
    for _ in 0..FIRE_DUR {
        fire_tick(&mut g, &mut ctx);
    }
    assert_eq!(g.fire[0][0], 0, "火應熄滅");
    assert_eq!(g.tiles[0][0], Tile::Floor, "油格應還原成地板");
}

#[test]
fn terrain_replay_is_bit_identical() {
    // 同地圖、同操作序列(點火 + 連跑 fire_tick)→ 最終 grid 與實體 hp 完全相同。
    let scenario = |g: &mut GameState, ctx: &mut StepCtx| {
        ignite_oil(g, 1, 2, ctx);
        for _ in 0..6 {
            fire_tick(g, ctx);
        }
    };
    let map = &[
        "########",
        "#~~~.WW#",
        "#~o~.W.#",
        "#~~~..o#",
        "########",
    ];
    let mut a = make(map);
    let mut b = make(map);
    let mut ca = StepCtx::new();
    let mut cb = StepCtx::new();
    scenario(&mut a, &mut ca);
    scenario(&mut b, &mut cb);

    assert_eq!(a.tiles, b.tiles, "tiles 必須 bit 一致");
    assert_eq!(a.fire, b.fire, "fire grid 必須 bit 一致");
    assert_eq!(a.burn_t, b.burn_t, "burn_t grid 必須 bit 一致");
    let ha: Vec<i32> = a.entities.iter().map(|e| e.hp).collect();
    let hb: Vec<i32> = b.entities.iter().map(|e| e.hp).collect();
    assert_eq!(ha, hb, "實體 hp 必須 bit 一致");
    assert_eq!(ca.events, cb.events, "event 流必須 bit 一致");
}

//! 法術 registry 正確性 + 確定性:validate 結構化拒絕、cast 效果、烈焰術 AoE、鏈跳候選。

use magicraid_sim::config::*;
use magicraid_sim::damage::StepCtx;
use magicraid_sim::spells::{
    cast, chain_candidates, initiate, resolve_heavy, validate, Reject, Spell, Target,
};
use magicraid_sim::state::{Entity, GameState, Kind, Tile};

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
        core: None,
    }
}

fn imp(g: &GameState) -> usize {
    g.entities.iter().position(|e| e.kind == Kind::Imp).unwrap()
}

#[test]
fn bolt_validate_and_damage() {
    let mut g = make(&["@..o"]); // 距離 3 ≤ range，有視線
    let mut ctx = StepCtx::new();
    let i = imp(&g);
    assert_eq!(validate(Spell::Bolt, &g, Target::cell(3, 0)), Ok(()));
    cast(Spell::Bolt, &mut g, Target::cell(3, 0), &mut ctx);
    assert_eq!(g.entities[i].hp, IMP_HP - BOLT_DMG);
}

#[test]
fn bolt_is_intercepted_by_unit_in_front() {
    // A(id1) 在 B(id2) 前面,瞄準 B → A 攔截中彈,B 沒事(身體當掩體)。
    let mut g = make(&["@.oo"]); // 法師(0,0)、A(2,0)、B(3,0);B 在射程 3 內
    let mut ctx = StepCtx::new();
    assert_eq!(validate(Spell::Bolt, &g, Target::cell(3, 0)), Ok(()), "瞄準 B 合法");
    cast(Spell::Bolt, &mut g, Target::cell(3, 0), &mut ctx);
    let a = g.entities.iter().find(|e| e.id == 1).unwrap().hp;
    let b = g.entities.iter().find(|e| e.id == 2).unwrap().hp;
    assert_eq!(a, IMP_HP - BOLT_DMG, "前方的 A 應攔截中彈");
    assert_eq!(b, IMP_HP, "後方的 B 應沒事");
}

#[test]
fn bolt_rejects_no_los_and_empty() {
    let g = make(&["@#o"]); // 牆擋視線
    assert_eq!(
        validate(Spell::Bolt, &g, Target::cell(2, 0)),
        Err(Reject::NoLineOfSight)
    );
    assert_eq!(
        validate(Spell::Bolt, &g, Target::cell(1, 0)),
        Err(Reject::NoEnemyThere)
    );
}

#[test]
fn bolt_out_of_range() {
    let g = make(&["@......o"]); // 距離 7 > range 3
    assert_eq!(
        validate(Spell::Bolt, &g, Target::cell(7, 0)),
        Err(Reject::OutOfRange)
    );
}

#[test]
fn push_requires_adjacent() {
    let g = make(&["@.o"]);
    assert_eq!(
        validate(Spell::Push, &g, Target::cell(2, 0)),
        Err(Reject::NotAdjacent)
    );
    let g2 = make(&["@o"]);
    assert_eq!(validate(Spell::Push, &g2, Target::cell(1, 0)), Ok(()));
}

#[test]
fn hook_rejects_adjacent_and_pulls() {
    let mut g = make(&["@...o"]);
    assert_eq!(
        validate(Spell::Hook, &make(&["@o.."]), Target::cell(1, 0)),
        Err(Reject::AlreadyAdjacent)
    );
    let i = imp(&g);
    assert_eq!(validate(Spell::Hook, &g, Target::cell(4, 0)), Ok(()));
    let mut ctx = StepCtx::new();
    cast(Spell::Hook, &mut g, Target::cell(4, 0), &mut ctx);
    assert_eq!(g.entities[i].x, 2, "勾索應拉近兩格");
}

#[test]
fn fire_ignites_oil_chain() {
    // 火球丟在油上 → 沿油延燒(igniteOil)。
    let mut g = make(&["@.~~~"]);
    let mut ctx = StepCtx::new();
    assert_eq!(validate(Spell::Fire, &g, Target::cell(2, 0)), Ok(()));
    cast(Spell::Fire, &mut g, Target::cell(2, 0), &mut ctx);
    for x in 2..5 {
        assert!(g.fire[0][x] > 0, "油格 {x} 應起火");
    }
}

#[test]
fn fire_tier2_leaves_fire_on_bare_floor() {
    // 火球★★ 命中空地板留一格火;基礎則不留。
    let mut g = make(&["@.."]);
    let mut ctx = StepCtx::new();
    cast(Spell::Fire, &mut g, Target::cell(2, 0), &mut ctx);
    assert_eq!(g.fire[0][2], 0, "基礎火球不在空地留火");

    let mut g2 = make(&["@.."]);
    let mut ctx2 = StepCtx::new();
    ctx2.tiers.set("fire", 2);
    cast(Spell::Fire, &mut g2, Target::cell(2, 0), &mut ctx2);
    assert!(g2.fire[0][2] > 0, "火球★★ 應在空地留火");
}

#[test]
fn fire_rejects_wall() {
    let g = make(&["@.#"]);
    assert_eq!(
        validate(Spell::Fire, &g, Target::cell(2, 0)),
        Err(Reject::TargetIsWall)
    );
}

#[test]
fn oilflask_paints_floor_only() {
    let mut g = make(&["@.#"]);
    let mut ctx = StepCtx::new();
    // 牆上不可潑。
    assert_eq!(
        validate(Spell::OilFlask, &g, Target::cell(2, 0)),
        Err(Reject::NotFloor)
    );
    // 空地可潑。
    assert_eq!(validate(Spell::OilFlask, &g, Target::cell(1, 0)), Ok(()));
    cast(Spell::OilFlask, &mut g, Target::cell(1, 0), &mut ctx);
    assert_eq!(g.tiles[0][1], Tile::Oil);
}

#[test]
fn oilflask_tier2_paints_line() {
    // ★★ 沿遠離法師方向潑 3 格油線。
    let mut g = make(&["@....."]);
    let mut ctx = StepCtx::new();
    ctx.tiers.set("oilflask", 2);
    cast(Spell::OilFlask, &mut g, Target::cell(2, 0), &mut ctx);
    assert_eq!(g.tiles[0][2], Tile::Oil);
    assert_eq!(g.tiles[0][3], Tile::Oil);
    assert_eq!(g.tiles[0][4], Tile::Oil);
}

#[test]
fn haste_grants_haste() {
    let mut g = make(&["@.."]);
    let mut ctx = StepCtx::new();
    cast(Spell::Haste, &mut g, Target::none(), &mut ctx);
    assert_eq!(g.mage().haste_turns, HASTE_GRANT);
}

#[test]
fn heavy_initiate_sets_channel() {
    let mut g = make(&["@..o"]);
    let mut ctx = StepCtx::new();
    assert!(Spell::Heavy.is_channel());
    initiate(Spell::Heavy, &mut g, Target::cell(3, 0), &mut ctx);
    let ch = g.mage().channel.clone().expect("應設定 channel");
    assert_eq!((ch.tx, ch.ty), (3, 0));
    assert!(!ch.ready && !ch.interrupted);
}

#[test]
fn heavy_resolve_plus_aoe_and_oil() {
    // 加號 AoE:中心打敵人、油格起火。佈局:中心 (2,1) 放敵,周圍鋪油。
    let mut g = make(&["..~..", ".~o~.", "..~.."]);
    g.entities.push(Entity::new(0, Kind::Mage, 0, 0));
    g.entities.sort_by_key(|e| e.id);
    let i = imp(&g);
    let mut ctx = StepCtx::new();
    resolve_heavy(&mut g, 2, 1, &mut ctx);
    assert_eq!(g.entities[i].hp, IMP_HP - HEAVY_DMG, "中心敵人應吃 AoE 傷");
    // 加號四格的油應被點燃。
    assert!(g.fire[0][2] > 0 && g.fire[2][2] > 0 && g.fire[1][1] > 0 && g.fire[1][3] > 0);
}

#[test]
fn heavy_tier2_shoves_survivors() {
    // ★★ 把命中的存活敵人往外震一格。敵人血厚到不會被一發打死。
    let mut g = make(&[".....", ".....", "....."]);
    let mut e = Entity::new(1, Kind::Imp, 3, 1); // 中心 (2,1) 右邊一格
    e.hp = 50;
    e.maxhp = 50;
    g.entities.push(Entity::new(0, Kind::Mage, 0, 0));
    g.entities.push(e);
    g.entities.sort_by_key(|en| en.id);
    let i = g.entities.iter().position(|en| en.id == 1).unwrap();
    let mut ctx = StepCtx::new();
    ctx.tiers.set("heavy", 2);
    resolve_heavy(&mut g, 2, 1, &mut ctx);
    assert_eq!(g.entities[i].x, 4, "存活敵人應被往外(遠離中心)震一格");
}

#[test]
fn chain_candidates_sorted_by_id() {
    // (2,1) 周圍放兩隻敵人(id 亂序加入),候選應依 id 升序。
    let mut g = make(&[".....", ".....", "....."]);
    g.entities.push(Entity::new(0, Kind::Mage, 0, 0));
    g.entities.push(Entity::new(5, Kind::Imp, 1, 1));
    g.entities.push(Entity::new(2, Kind::Imp, 3, 1));
    g.entities.sort_by_key(|e| e.id);
    let cands = chain_candidates(&g, 2, 1, &[]);
    let ids: Vec<u32> = cands.iter().map(|&i| g.entities[i].id).collect();
    assert_eq!(ids, vec![2, 5], "候選應依 entity id 升序");
    // 已命中 id 2 → 排除。
    let cands2 = chain_candidates(&g, 2, 1, &[2]);
    let ids2: Vec<u32> = cands2.iter().map(|&i| g.entities[i].id).collect();
    assert_eq!(ids2, vec![5]);
}

#[test]
fn spells_replay_is_bit_identical() {
    let map = &["@.~~o", ".....", "..o.."];
    let run = || {
        let mut g = make(map);
        let mut ctx = StepCtx::new();
        ctx.tiers.set("fire", 2);
        cast(Spell::Fire, &mut g, Target::cell(2, 0), &mut ctx);
        cast(Spell::Haste, &mut g, Target::none(), &mut ctx);
        resolve_heavy(&mut g, 2, 2, &mut ctx);
        let snap: Vec<(u32, i32, i32, i32)> =
            g.entities.iter().map(|e| (e.id, e.x, e.y, e.hp)).collect();
        (snap, g.tiles.clone(), g.fire.clone(), ctx.events)
    };
    assert_eq!(run(), run(), "法術回放必須 bit 一致");
}

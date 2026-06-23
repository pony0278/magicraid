//! roguelite 層:PRNG 確定性、三選一撿取/升級/丟牌、房間載入(含魔像初始預告)。

use magicraid_sim::config::SPELL_CAP;
use magicraid_sim::roguelite::{
    apply_drop, apply_pick, gen_offers, hash32, init_room, rng_for, Mulberry32, Op, PickResult,
    RunState,
};
use magicraid_sim::spells::{pickable, Spell};
use magicraid_sim::state::Kind;

#[test]
fn hash32_is_deterministic_and_pure() {
    assert_eq!(hash32("123|picks|0"), hash32("123|picks|0"));
    assert_ne!(hash32("123|picks|0"), hash32("123|picks|1"));
}

#[test]
fn mulberry32_sequence_is_reproducible() {
    let mut a = Mulberry32::new(42);
    let mut b = Mulberry32::new(42);
    let sa: Vec<u32> = (0..10).map(|_| a.next_u32()).collect();
    let sb: Vec<u32> = (0..10).map(|_| b.next_u32()).collect();
    assert_eq!(sa, sb);
    // below(n) 永遠落在 [0, n)。
    let mut r = Mulberry32::new(7);
    for _ in 0..1000 {
        assert!(r.below(6) < 6);
    }
}

#[test]
fn rng_for_is_path_independent() {
    // 同 (seed, tag, idx) → 同序列,與「之前抽過幾次」無關。
    let mut r1 = rng_for(999, "picks", 2);
    let mut r2 = rng_for(999, "picks", 2);
    assert_eq!(r1.next_u32(), r2.next_u32());
    // 不同 idx → 不同序列(極大機率)。
    let mut r3 = rng_for(999, "picks", 3);
    assert_ne!(r1.next_u32(), r3.next_u32());
}

#[test]
fn gen_offers_same_seed_same_three() {
    let run_a = RunState::new(12345);
    let run_b = RunState::new(12345);
    assert_eq!(gen_offers(&run_a), gen_offers(&run_b), "同種子同進度 → 同三張");
    assert!(gen_offers(&run_a).len() <= 3);
}

#[test]
fn gen_offers_only_pickable_and_unique() {
    let run = RunState::new(555);
    let offers = gen_offers(&run);
    let pool = pickable();
    for s in &offers {
        assert!(pool.contains(s), "選項必須是可撿池內的法術");
        assert!(!s.baseline(), "baseline 不該出現在三選一");
    }
    // 無重複。
    let mut seen = offers.clone();
    seen.dedup();
    assert_eq!(seen.len(), offers.len(), "三張不應重複");
}

#[test]
fn apply_pick_adds_until_cap_then_needs_drop() {
    let mut run = RunState::new(1);
    assert_eq!(apply_pick(&mut run, Spell::OilFlask), PickResult::Done);
    assert_eq!(apply_pick(&mut run, Spell::Fire), PickResult::Done);
    assert_eq!(apply_pick(&mut run, Spell::Hook), PickResult::Done);
    assert_eq!(run.acquired.len(), SPELL_CAP);
    // 滿了、拿新的(未持有)→ 需丟牌。
    assert_eq!(apply_pick(&mut run, Spell::Haste), PickResult::NeedDrop);
}

#[test]
fn apply_pick_upgrades_without_slot() {
    let mut run = RunState::new(1);
    apply_pick(&mut run, Spell::OilFlask); // tier 1
    let before = run.acquired.len();
    let r = apply_pick(&mut run, Spell::OilFlask); // 再選同一張 → 升級
    assert_eq!(r, PickResult::Done);
    assert_eq!(run.acquired.len(), before, "升級不占新槽");
    assert_eq!(run.tiers.of("oilflask"), 2, "應升到 ★★");
    // 升級不超過 max_tier。
    apply_pick(&mut run, Spell::OilFlask);
    assert_eq!(run.tiers.of("oilflask"), Spell::OilFlask.max_tier());
    assert!(matches!(run.op_log.last(), Some(Op::Upgrade { id: Spell::OilFlask, .. })));
}

#[test]
fn apply_drop_replaces_and_clears_tier() {
    let mut run = RunState::new(1);
    apply_pick(&mut run, Spell::OilFlask);
    apply_pick(&mut run, Spell::Fire);
    apply_pick(&mut run, Spell::Hook);
    apply_pick(&mut run, Spell::OilFlask); // 把 push 升到 2
    assert_eq!(run.tiers.of("oilflask"), 2);
    // 丟掉 push 換 haste。
    apply_drop(&mut run, Spell::Haste, Spell::OilFlask);
    assert!(!run.acquired.contains(&Spell::OilFlask));
    assert!(run.acquired.contains(&Spell::Haste));
    assert_eq!(run.tiers.of("oilflask"), 1, "丟掉的法術等級應清除(回預設 1)");
    assert_eq!(run.tiers.of("haste"), 1, "換上的新法術為 ★");
    assert_eq!(run.acquired.len(), SPELL_CAP, "仍維持滿欄");
}

#[test]
fn init_room_parses_entities_and_tiles() {
    // 房間 0:法師 (2,4)、1 小鬼、1 符文眼、無 boss。
    let g = init_room(0);
    assert_eq!((g.mage().x, g.mage().y), (2, 4), "法師起始位置");
    assert_eq!(g.alive_enemies(), 2, "房間 0 應有 2 敵");
    assert_eq!(g.mage().id, 0, "法師 id 為 0");
    // 邊界是牆。
    assert_eq!(g.tiles[0][0], magicraid_sim::state::Tile::Wall);
}

#[test]
fn init_room_arms_boss_telegraph() {
    // 關主房:boss 應在載入時就 arm 初始預告(補 ai.rs 標註的缺口)。
    let boss_room = magicraid_sim::config::ROOMS.len() - 1;
    let g = init_room(boss_room);
    let boss = g.entities.iter().find(|e| e.kind == Kind::Boss).expect("應有 boss");
    assert!(boss.pending_slam, "boss 初始應 pending_slam");
    let slam = boss.slam.as_ref().expect("boss 初始預告應已 arm");
    // 預告應涵蓋法師起始格。
    assert!(slam.contains(&(g.mage().x, g.mage().y)));
}

#[test]
fn full_run_offers_replay_bit_identical() {
    // 模擬一整場:固定種子,逐房 gen_offers + 撿第一張,兩次跑出完全相同的 build + oplog。
    let play = || {
        let mut run = RunState::new(0xABCDEF);
        for room in 0..5usize {
            run.room_idx = room;
            let offers = gen_offers(&run);
            if let Some(&first) = offers.first() {
                if apply_pick(&mut run, first) == PickResult::NeedDrop {
                    // 滿欄就丟第一張手牌。
                    let drop = run.acquired[0];
                    apply_drop(&mut run, first, drop);
                }
            }
        }
        (run.acquired, run.op_log)
    };
    assert_eq!(play(), play(), "整場撿取回放必須 bit 一致");
}

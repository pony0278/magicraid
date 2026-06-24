//! Demo 2 base-raid:`init_base` 解析 `C` 核心 + 終局(抵達核心 _或_ 清場 = 突襲成功)。

use magicraid_sim::{init_base, step, Action, RunState, Spell, Status, Target};

#[test]
fn init_base_parses_core_and_spawn() {
    let g = init_base(&["#####", "#@.C#", "#####"]);
    assert_eq!(g.core, Some((3, 1)), "C 應設核心格");
    let m = g.mage();
    assert_eq!((m.x, m.y), (1, 1), "@ 應設突襲者出生點");
    assert_eq!(g.alive_enemies(), 0);
}

#[test]
fn reaching_core_wins_even_with_enemy_alive() {
    // 牆內死關一隻 imp(永遠碰不到法師),法師沿 row 1 直奔核心。
    let mut g = init_base(&[
        "#######",
        "#@...C#",
        "#####.#",
        "#o###.#",
        "#######",
    ]);
    let mut run = RunState::new(0);
    let r = step(&mut g, &mut run, Action::MoveTo { x: 5, y: 1 });
    assert_eq!(r.status, Status::RunComplete, "踩到核心 = 突襲成功");
    assert!(g.alive_enemies() >= 1, "守軍還活著 → 確定是『抵達核心』而非『清場』");
}

#[test]
fn clearing_defenders_wins() {
    // imp 擋在法師與核心之間;清掉它 = 突襲成功(清場路徑)。
    let mut g = init_base(&["#####", "#@oC#", "#####"]);
    let mut run = RunState::new(0);
    let mut last = Status::AwaitingInput;
    for _ in 0..4 {
        // 對著 imp 砸魔法彈(2 發致死);死後 alive_enemies()==0 → RunComplete。
        last = step(&mut g, &mut run, Action::Cast { spell: Spell::Bolt, target: Target::cell(2, 1) }).status;
        if last == Status::RunComplete {
            break;
        }
    }
    assert_eq!(last, Status::RunComplete, "清光守軍 = 突襲成功");
    assert_eq!(g.alive_enemies(), 0);
}

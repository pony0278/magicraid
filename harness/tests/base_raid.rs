use harness::bot_raid;
use magicraid_sim::{Spell, Status};

#[test]
fn bot_cracks_a_trivial_base() {
    // 一隻 imp 擋路 + 核心;bot 帶基礎包就該攻破(清場或到核心)。
    let res = bot_raid(&["#######", "#@..oC#", "#######"], &[], 500);
    assert_eq!(res.outcome, Status::RunComplete, "bot 應攻破簡單基地");
    assert!(res.steps > 0 && res.steps < 500, "幾手內破關,不超預算 (steps={})", res.steps);
}

#[test]
fn tougher_base_takes_more_steps() {
    // 多守軍 + 油 → 應該更花手數(或守得住)。只驗不崩、有回報。
    let res = bot_raid(
        &["##########", "#@..~~..oC#", "#..o..e..C#", "##########"],
        &[Spell::Fire],
        1000,
    );
    assert!(matches!(res.outcome, Status::RunComplete | Status::Defeat), "要嘛攻破要嘛守住,不卡死");
}

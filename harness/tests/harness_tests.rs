//! harness 閘門測試:確定性回放 bit 一致、不崩潰、可解性(房 0–4 由 baseline 攻克)。
//!
//! 備註:boss(最後一房)是設計上的技巧關,貪婪 baseline 尚未攻克 → 此處只斷言「能抵達 boss」
//! (= 房 0–4 皆可解),boss 本身可解性留待更強 agent / 手寫解法(docs/07 backlog)。

use harness::{play, replay, room_count};
use magicraid_sim::Status;

const SEEDS: u32 = 300;
const BUDGET: usize = 5000;

#[test]
fn replay_is_bit_identical_for_all_seeds() {
    // B-3 核心:同 (seed, op 序列) → 最終狀態 + 結果 bit 一致。
    for seed in 0..SEEDS {
        let t = play(seed, BUDGET);
        let (status, snap) = replay(seed, &t.ops);
        assert_eq!(status, t.outcome, "seed {seed}: 回放結果不一致");
        assert_eq!(snap, t.final_snapshot, "seed {seed}: 回放最終狀態不 bit 一致");
    }
}

#[test]
fn agent_is_deterministic_run_to_run() {
    // 同種子跑兩次 baseline → 完全相同的 op 序列與結果(agent + sim 皆確定性)。
    for seed in [0u32, 1, 7, 42, 123, 299] {
        let a = play(seed, BUDGET);
        let b = play(seed, BUDGET);
        assert_eq!(a.ops, b.ops, "seed {seed}: 兩次 op 序列不同");
        assert_eq!(a.outcome, b.outcome);
        assert_eq!(a.final_snapshot, b.final_snapshot);
    }
}

#[test]
fn no_crash_no_timeout_across_seeds() {
    // 跑滿不 panic(panic 會讓測試直接失敗),且沒有任何一場卡死到 budget。
    for seed in 0..SEEDS {
        let t = play(seed, BUDGET);
        assert!(
            t.steps < BUDGET,
            "seed {seed}: 跑到 budget 上限({BUDGET})疑似卡死"
        );
        // 一場必定停在終局(通關/陣亡),不會懸在等待。
        assert!(
            matches!(t.outcome, Status::RunComplete | Status::Defeat),
            "seed {seed}: 非終局結束 = {:?}",
            t.outcome
        );
    }
}

#[test]
fn rooms_0_to_2_solvable_every_seed() {
    // 每個種子都清掉房 0、1、2(進入房 3)→ 證明這三房可解。
    for seed in 0..SEEDS {
        let t = play(seed, BUDGET);
        assert!(
            t.max_room >= 3,
            "seed {seed}: 只到房 {} — 房 0–2 未必每場可解",
            t.max_room
        );
    }
}

#[test]
fn rooms_3_and_4_are_solvable() {
    // 至少有種子抵達最後一房(boss)= 房 3、4 被 baseline 攻克過 → 證明它們可解。
    let last = room_count() - 1;
    let any_reaches_boss = (0..SEEDS).any(|seed| play(seed, BUDGET).max_room >= last);
    assert!(any_reaches_boss, "沒有任何種子抵達 boss 房 → 房 3/4 可解性未證");
}

#[test]
fn boss_room_is_reached_known_gap() {
    // 文件化現況:boss 房可抵達,但 baseline 尚未通關(設計上的技巧關)。
    // 一旦未來更強 agent 能通關,把此測試升級成斷言 RunComplete。
    let last = room_count() - 1;
    let reached: u32 = (0..SEEDS)
        .filter(|&s| play(s, BUDGET).max_room >= last)
        .count() as u32;
    let cleared: u32 = (0..SEEDS)
        .filter(|&s| play(s, BUDGET).outcome == Status::RunComplete)
        .count() as u32;
    assert!(reached > 0, "應有種子抵達 boss 房");
    // 已知缺口:cleared 目前為 0。不硬性斷言通關,只記錄。
    println!("boss 房:抵達 {reached} 場,baseline 通關 {cleared} 場(已知缺口)");
}

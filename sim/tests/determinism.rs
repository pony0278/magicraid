//! 確定性地基驗證(階段 B 閘門的第一塊)。
//!
//! 兩件事:
//! 1. 時間鏈出手順序與《速度制戰鬥規格》§5 的手算 trace 一致(整數時間正確)。
//! 2. 同初始狀態跑兩次 → 時間值 bit 級相同(B0 §B-3 對拍精神的最小版)。

use magicraid_sim::config::{SPEED_HALVES_BASE, SPEED_HALVES_HASTE};
use magicraid_sim::state::{Entity, GameState, Kind, Tile};
use magicraid_sim::time_chain::{advance_time, next_actor_index};

/// 最小兩角色狀態:法師(id 0)+ 小鬼(id 1),擺在 1×2 地板上。
/// `mage_speed` 直接設法師速度(半步),用來測等速 vs 加速兩種 trace。
fn two_actor_state(mage_speed: u8) -> GameState {
    let mut mage = Entity::new(0, Kind::Mage, 0, 0);
    mage.speed_halves = mage_speed;
    let imp = Entity::new(1, Kind::Imp, 1, 0); // 基礎速度 1.0
    GameState {
        w: 2,
        h: 1,
        tiles: vec![vec![Tile::Floor, Tile::Floor]],
        fire: vec![vec![0, 0]],
        burn_t: vec![vec![0, 0]],
        entities: vec![mage, imp],
        room_idx: 0,
    }
}

/// 純時間鏈推進 n 手(行動 = 只推進該角色時間),回傳出手者種類序列。
fn run_chain(g: &mut GameState, n: usize) -> String {
    let mut seq = String::new();
    for _ in 0..n {
        let i = next_actor_index(g).expect("應永遠有存活者");
        seq.push(if g.entities[i].kind.is_mage() { 'M' } else { 'E' });
        advance_time(&mut g.entities[i]);
    }
    seq
}

#[test]
fn equal_speed_alternates() {
    // 速度規格 §5:法 1.0 / 怪 1.0 → 乾淨 1:1 交替。
    let mut g = two_actor_state(SPEED_HALVES_BASE);
    assert_eq!(run_chain(&mut g, 8), "MEMEMEME");
}

#[test]
fn haste_inserts_extra_turn() {
    // 速度規格 §5:法 1.5 / 怪 1.0 → 出現連續兩個「法」(插隊額外一手)。
    // 整數時間(1/6):法 step=4、怪 step=6。手算前 10 手 = MEMEMMEMEM。
    let mut g = two_actor_state(SPEED_HALVES_HASTE);
    assert_eq!(run_chain(&mut g, 10), "MEMEMMEMEM");
}

#[test]
fn tiebreak_mage_first() {
    // 時間值相等時法師優先(B0 §D-6 第一段 tiebreak)。
    let g = two_actor_state(SPEED_HALVES_BASE); // 兩者 time 都 0
    let i = next_actor_index(&g).unwrap();
    assert!(g.entities[i].kind.is_mage());
}

#[test]
fn replay_is_bit_identical() {
    // 同初始 + 同操作序列 → 最終時間值逐一相同(回放地基)。
    let mut a = two_actor_state(SPEED_HALVES_HASTE);
    let mut b = two_actor_state(SPEED_HALVES_HASTE);
    run_chain(&mut a, 200);
    run_chain(&mut b, 200);
    let ta: Vec<i64> = a.entities.iter().map(|e| e.time).collect();
    let tb: Vec<i64> = b.entities.iter().map(|e| e.time).collect();
    assert_eq!(ta, tb, "兩次跑的時間值必須 bit 級相同");
}

#[test]
fn time_is_always_integer_multiple_of_step() {
    // 全整數驗證:加速場景跑久了,法師時間值仍是其 step(4)的整數倍 → 無分數漂移。
    let mut g = two_actor_state(SPEED_HALVES_HASTE);
    run_chain(&mut g, 99);
    let mage_time = g.entities.iter().find(|e| e.kind.is_mage()).unwrap().time;
    assert_eq!(mage_time % 4, 0, "法師時間值應為 step=4 的整數倍");
}

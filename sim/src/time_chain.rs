//! 時間鏈推進 —— sim 核確定性的地基。
//!
//! 對應 `prototype/demo1.html`:`nextActor`(行 543–547)、`advance`(548–582)、
//! `endMageAction`(533–540)。模型見《速度制戰鬥規格》§1/§3。
//!
//! 確定性兩條鐵律(B0 §D-1 / §D-6):
//! 1. 時間值用 **1/6 整數**累加(config::time_step),絕不 float。
//! 2. nextActor tiebreak **寫死**:time 升 → mage 優先 → id 升。

use crate::config;
use crate::state::{Entity, GameState};

/// 推進一名行動者的時間值。對應 JS `time += 1/effSpeed`(+ 法師加速手數遞減)。
///
/// 全整數:`time += time_step(eff_speed)`,eff_speed ∈ {1,2,3,4}(半步)皆整除 12。
pub fn advance_time(e: &mut Entity) {
    e.time += config::time_step(e.eff_speed_halves());
    if e.haste_turns > 0 {
        e.haste_turns -= 1;
    }
}

/// tiebreak 排序鍵:mage 排在敵人前(0 < 1)。
#[inline]
fn mage_rank(e: &Entity) -> u8 {
    if e.kind.is_mage() {
        0
    } else {
        1
    }
}

/// 下一個行動者在 `entities` 中的索引(存活者中時間值最小)。
///
/// 三段 tiebreak 寫死:`time 升 → mage 優先 → id 升`。因 id 唯一,排序全序、無歧義 →
/// 同狀態必得同結果,回放 bit 一致。沒有存活者(理論上不會)時回 `None`。
pub fn next_actor_index(g: &GameState) -> Option<usize> {
    g.entities
        .iter()
        .enumerate()
        .filter(|(_, e)| e.alive())
        .min_by(|(_, a), (_, b)| {
            a.time
                .cmp(&b.time)
                .then_with(|| mage_rank(a).cmp(&mage_rank(b)))
                .then_with(|| a.id.cmp(&b.id))
        })
        .map(|(i, _)| i)
}

/// 把全體存活者依出手順序(時間鏈)排出索引序列,給 view 畫 XCOM 式順序條。
///
/// 對應 JS `projectChain`(唯讀 query,非 step())。此處為純骨架版:照當前時間值排,
/// 不模擬未來推進(完整前瞻待後續模組)。排序鍵與 `next_actor_index` 完全一致。
pub fn order_chain(g: &GameState) -> Vec<usize> {
    let mut idx: Vec<usize> = g
        .entities
        .iter()
        .enumerate()
        .filter(|(_, e)| e.alive())
        .map(|(i, _)| i)
        .collect();
    idx.sort_by(|&a, &b| {
        let (ea, eb) = (&g.entities[a], &g.entities[b]);
        ea.time
            .cmp(&eb.time)
            .then_with(|| mage_rank(ea).cmp(&mage_rank(eb)))
            .then_with(|| ea.id.cmp(&eb.id))
    });
    idx
}

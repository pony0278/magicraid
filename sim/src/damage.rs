//! 傷害 / 死亡結算 + 事件輸出。
//!
//! 對應 `prototype/demo1.html` `dealDamage`(行 257–268),但 **`pendingHits.push` 改成 emit
//! `Event`**(B0 §C-4)。含:boss 過熱雙倍、法師蓄力受傷打斷、擊殺續加速(★★)。

use crate::events::Event;
use crate::state::{GameState, Kind};
use std::collections::BTreeMap;

/// 已撿法術的等級表(對應 JS 全域 `tier` + `tierOf`,行 198–199)。
///
/// 1 = 基礎,2 = ★★。`roguelite.rs` 之後負責填;在那之前 `of` 一律回 1(★★ 效果休眠)。
/// 用 `BTreeMap`(只查詢、不靠迭代序),避免 HashMap 迭代序風險(B0 §D-6)。
#[derive(Clone, Debug, Default)]
pub struct TierTable {
    inner: BTreeMap<&'static str, u8>,
}

impl TierTable {
    /// 查某法術的等級;未撿/未升 → 1。對應 JS `tierOf`。
    pub fn of(&self, id: &str) -> u8 {
        self.inner.get(id).copied().unwrap_or(1)
    }
    /// 設定等級(roguelite 升級用;測試也用它注入 ★★)。
    pub fn set(&mut self, id: &'static str, lvl: u8) {
        self.inner.insert(id, lvl);
    }
}

/// 一次 `step()` 內的區域可變狀態(B0 §G-6:這些**不進持久 GameState**)。
///
/// 之後 `lib::step` 會建一個 `StepCtx`,穿給 damage / terrain / spells / ai 共用。
pub struct StepCtx {
    /// 依序累積的事件(view 照順序重放動畫/字串)。
    pub events: Vec<Event>,
    /// auto-walk 期間「被打過就煞車」旗標(對應 JS `mageHurt`,行 261)。
    pub mage_hurt: bool,
    /// 法術等級表(★★ 效果的閘門:擊殺續加速、撞暈、勾索定身、烈焰震退…)。
    pub tiers: TierTable,
}

impl StepCtx {
    pub fn new() -> Self {
        StepCtx {
            events: Vec::new(),
            mage_hurt: false,
            tiers: TierTable::default(),
        }
    }
}

impl Default for StepCtx {
    fn default() -> Self {
        Self::new()
    }
}

/// 對 `entities[idx]` 造成 `amt` 傷害並結算死亡。對應 JS `dealDamage`(逐行對齊)。
pub fn deal_damage(g: &mut GameState, idx: usize, mut amt: i32, ctx: &mut StepCtx) {
    let kind = g.entities[idx].kind;

    // boss 過熱(exhausted)受到雙倍傷害。
    if kind == Kind::Boss && g.entities[idx].exhausted {
        amt *= 2;
    }

    // 法師蓄力中受傷 → 標記打斷(idempotent:已打斷不重複發事件)。
    if kind.is_mage() {
        let id = g.entities[idx].id;
        if let Some(ch) = g.entities[idx].channel.as_mut() {
            if !ch.interrupted {
                ch.interrupted = true;
                ctx.events.push(Event::ChannelInterrupted { id });
            }
        }
        ctx.mage_hurt = true;
    }

    let id = g.entities[idx].id;
    g.entities[idx].hp -= amt;
    ctx.events.push(Event::Damaged { id, amt });

    if g.entities[idx].hp <= 0 {
        g.entities[idx].hp = 0;
        if kind.is_mage() {
            // 法師死亡交給 step 終局處理(對應 JS dealDamage 對 mage 直接 return)。
            return;
        }
        ctx.events.push(Event::Died { id });
        // 加速★★:擊殺當下若法師仍在加速,續一手(滾雪球)。需用擊殺當下的法師狀態。
        if ctx.tiers.of("haste") >= 2 {
            let mage = g.mage_mut();
            if mage.haste_turns > 0 {
                mage.haste_turns += 1;
                ctx.events.push(Event::HasteGained { id: mage.id });
            }
        }
    }
}

//! 傷害 / 死亡結算 + 事件輸出。
//!
//! 對應 `prototype/demo1.html` `dealDamage`(行 257–268),但 **`pendingHits.push` 改成 emit
//! `Event`**(B0 §C-4)。含:boss 過熱雙倍、法師蓄力受傷打斷、擊殺續加速(★★)。

use crate::events::Event;
use crate::state::{GameState, Kind};

/// 一次 `step()` 內的區域可變狀態(B0 §G-6:這些**不進持久 GameState**)。
///
/// 之後 `lib::step` 會建一個 `StepCtx`,穿給 damage / terrain / spells / ai 共用。
pub struct StepCtx {
    /// 依序累積的事件(view 照順序重放動畫/字串)。
    pub events: Vec<Event>,
    /// auto-walk 期間「被打過就煞車」旗標(對應 JS `mageHurt`,行 261)。
    pub mage_hurt: bool,
    /// roguelite `tierOf("haste")`。先佔位(預設 1),`roguelite.rs` 之後餵入;
    /// ≥2 時才有「擊殺續一手」滾雪球(對應 JS 行 266)。
    pub haste_tier: u8,
}

impl StepCtx {
    pub fn new() -> Self {
        StepCtx {
            events: Vec::new(),
            mage_hurt: false,
            haste_tier: 1,
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
        if ctx.haste_tier >= 2 {
            let mage = g.mage_mut();
            if mage.haste_turns > 0 {
                mage.haste_turns += 1;
                ctx.events.push(Event::HasteGained { id: mage.id });
            }
        }
    }
}

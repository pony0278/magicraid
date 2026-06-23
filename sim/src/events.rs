//! 結構化事件與回合狀態。
//!
//! 紀律(B0 §C-4):sim **不吐字串、不吐動畫幀**,只吐 events;view 端從 events 重建
//! 動畫與在地化字串。`Status` 收斂 JS 散落的 waiting/anim/wantOverlay 控制流(B0 §E)。
//! 骨架先列核心變體,terrain/spells 模組進來時再擴(FireSpread/WoodIgnited…)。

/// 一次 `step()` 內依序累積的事件(view 照順序重放)。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Damaged { id: u32, amt: i32 },
    Died { id: u32 },
    Moved { id: u32, from: (i32, i32), to: (i32, i32) },
    SpellCast { id: u32, spell: &'static str },
    ChannelInterrupted { id: u32 },
    Stunned { id: u32 },
    HasteGained { id: u32 },
}

/// `step()` 回傳的回合狀態(對應 B0 §E 對照表)。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// 輪到法師、無 auto-path、無待釋放通道 → 等玩家輸入。
    AwaitingInput,
    /// 前搖撐過、停在釋放手(JS mage.channel.ready)。
    AwaitingRelease,
    /// 非 boss 房清空且有下一房 → 給三選一。
    PickOffered,
    /// boss 死,整場通關。
    RunComplete,
    /// 法師 hp ≤ 0。
    Defeat,
}

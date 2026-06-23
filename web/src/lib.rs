//! WASM 客戶端綁定(免 wasm-bindgen)。
//!
//! 護欄 A:**跑同一份 sim**(`magicraid-sim`),這層只做 C-ABI 邊界與 JSON 序列化,不含任何玩法邏輯。
//! 邊界設計:sim 狀態 → JSON 字串寫進 wasm 線性記憶體;JS 讀 `ptr/len` 後 `TextDecoder`+`JSON.parse`。
//!
//! 玩法真相仍在 sim;view metadata(icon/name/文案)留在 JS 殼(B0 §C-3)。

use magicraid_sim::{
    apply_drop, apply_pick, gen_offers, init_room, project_chain, spells::pickable, step, Action,
    Event, GameState, Kind, PickResult, RunState, Spell, Status, Target, Tile,
};
use std::cell::RefCell;

/// 法術 code ↔ Spell(對齊 sim SPELL_ORDER;JS 端用同一張表)。
const SPELL_BY_CODE: [Spell; 7] = [
    Spell::Bolt,
    Spell::Push,
    Spell::Fire,
    Spell::Heavy,
    Spell::OilFlask,
    Spell::Hook,
    Spell::Haste,
];

fn spell_code(s: Spell) -> u32 {
    SPELL_BY_CODE.iter().position(|&x| x == s).unwrap() as u32
}

struct World {
    g: GameState,
    run: RunState,
    status: Status,
    rejected: bool,
    last_events: Vec<Event>,
}

thread_local! {
    static WORLD: RefCell<Option<World>> = const { RefCell::new(None) };
    static BUF: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

fn status_code(s: Status) -> u32 {
    match s {
        Status::AwaitingInput => 0,
        Status::AwaitingRelease => 1,
        Status::PickOffered => 2,
        Status::RunComplete => 3,
        Status::Defeat => 4,
    }
}

fn tile_code(t: Tile) -> u32 {
    match t {
        Tile::Floor => 0,
        Tile::Wall => 1,
        Tile::Wood => 2,
        Tile::WoodBurn => 3,
        Tile::Oil => 4,
        Tile::Spike => 5,
        Tile::Rune => 6,
    }
}

fn kind_str(k: Kind) -> &'static str {
    match k {
        Kind::Mage => "mage",
        Kind::Imp => "imp",
        Kind::Eye => "eye",
        Kind::Boss => "boss",
    }
}

/// 把字串放進 thread-local 緩衝,回傳其指標(長度由 `mr_buf_len` 取)。
fn publish(s: String) -> *const u8 {
    BUF.with(|b| {
        let mut b = b.borrow_mut();
        *b = s.into_bytes();
        b.as_ptr()
    })
}

// ─────────────────────────── 匯出 API ───────────────────────────

/// 開新一場(種子外傳)。
#[no_mangle]
pub extern "C" fn mr_new(seed: u32) {
    let run = RunState::new(seed);
    let g = init_room(0);
    WORLD.with(|w| {
        *w.borrow_mut() = Some(World {
            g,
            run,
            status: Status::AwaitingInput,
            rejected: false,
            last_events: Vec::new(),
        });
    });
}

/// 套用一手。act:0=Wait 1=Potion 2=MoveTo(x,y) 3=Cast(spell,x,y) 4=Release。回傳 status code。
#[no_mangle]
pub extern "C" fn mr_step(act: u32, x: i32, y: i32, spell: u32) -> u32 {
    WORLD.with(|w| {
        let mut w = w.borrow_mut();
        let world = match w.as_mut() {
            Some(v) => v,
            None => return 0,
        };
        let action = match act {
            1 => Action::Potion,
            2 => Action::MoveTo { x, y },
            3 => Action::Cast {
                spell: SPELL_BY_CODE[(spell as usize).min(6)],
                target: Target::cell(x, y),
            },
            4 => Action::Release,
            _ => Action::Wait,
        };
        let r = step(&mut world.g, &mut world.run, action);
        world.rejected = r.rejected.is_some();
        world.status = r.status;
        world.last_events = r.events;
        status_code(world.status)
    })
}

/// 上一手是否被拒(非法 action,無時間流逝)。
#[no_mangle]
pub extern "C" fn mr_rejected() -> u32 {
    WORLD.with(|w| w.borrow().as_ref().map_or(0, |x| x.rejected as u32))
}

/// 目前 status code。
#[no_mangle]
pub extern "C" fn mr_status() -> u32 {
    WORLD.with(|w| w.borrow().as_ref().map_or(0, |x| status_code(x.status)))
}

/// 三選一選一張(自動丟最舊的;v1 不做丟牌 UI)。需 status==PickOffered。
#[no_mangle]
pub extern "C" fn mr_pick(spell: u32) {
    WORLD.with(|w| {
        if let Some(world) = w.borrow_mut().as_mut() {
            let s = SPELL_BY_CODE[(spell as usize).min(6)];
            if let PickResult::NeedDrop = apply_pick(&mut world.run, s) {
                let drop = world.run.acquired[0];
                apply_drop(&mut world.run, s, drop);
            }
        }
    });
}

/// 前進到下一房(清關/選完後呼叫)。
#[no_mangle]
pub extern "C" fn mr_next_room() {
    WORLD.with(|w| {
        if let Some(world) = w.borrow_mut().as_mut() {
            world.run.room_idx += 1;
            world.g = init_room(world.run.room_idx);
            world.status = Status::AwaitingInput;
            world.rejected = false;
        }
    });
}

/// 緩衝長度(配合任何回傳 ptr 的呼叫)。
#[no_mangle]
pub extern "C" fn mr_buf_len() -> usize {
    BUF.with(|b| b.borrow().len())
}

/// 把目前完整可渲染狀態序列化成 JSON,回傳 ptr(長度用 mr_buf_len)。
#[no_mangle]
pub extern "C" fn mr_render() -> *const u8 {
    WORLD.with(|w| {
        let w = w.borrow();
        let world = match w.as_ref() {
            Some(v) => v,
            None => return publish("{}".into()),
        };
        publish(render_json(world))
    })
}

/// 目前三選一選項(spell code 陣列 JSON);非 PickOffered 或無選項時回 "[]"。
#[no_mangle]
pub extern "C" fn mr_offers() -> *const u8 {
    WORLD.with(|w| {
        let w = w.borrow();
        let world = match w.as_ref() {
            Some(v) => v,
            None => return publish("[]".into()),
        };
        let offers = gen_offers(&world.run);
        let codes: Vec<String> = offers.iter().map(|&s| spell_code(s).to_string()).collect();
        publish(format!("[{}]", codes.join(",")))
    })
}

/// 上一手累積的事件(JSON 陣列),供殼端重建動畫(命中閃光、移動、死亡…)。
#[no_mangle]
pub extern "C" fn mr_events() -> *const u8 {
    WORLD.with(|w| {
        let w = w.borrow();
        match w.as_ref() {
            Some(world) => publish(events_json(&world.last_events)),
            None => publish("[]".into()),
        }
    })
}

fn events_json(evs: &[Event]) -> String {
    let mut s = String::from("[");
    for (i, e) in evs.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        match e {
            Event::Damaged { id, amt } => {
                s.push_str(&format!("{{\"t\":\"dmg\",\"id\":{id},\"amt\":{amt}}}"))
            }
            Event::Died { id } => s.push_str(&format!("{{\"t\":\"die\",\"id\":{id}}}")),
            Event::Moved { id, from, to } => s.push_str(&format!(
                "{{\"t\":\"mv\",\"id\":{id},\"fx\":{},\"fy\":{},\"tx\":{},\"ty\":{}}}",
                from.0, from.1, to.0, to.1
            )),
            Event::SpellCast { id, spell } => {
                s.push_str(&format!("{{\"t\":\"cast\",\"id\":{id},\"s\":\"{spell}\"}}"))
            }
            Event::ChannelInterrupted { id } => {
                s.push_str(&format!("{{\"t\":\"intr\",\"id\":{id}}}"))
            }
            Event::Stunned { id } => s.push_str(&format!("{{\"t\":\"stun\",\"id\":{id}}}")),
            Event::HasteGained { id } => s.push_str(&format!("{{\"t\":\"haste\",\"id\":{id}}}")),
            Event::Healed { id, amt } => {
                s.push_str(&format!("{{\"t\":\"heal\",\"id\":{id},\"amt\":{amt}}}"))
            }
        }
    }
    s.push(']');
    s
}

fn render_json(world: &World) -> String {
    let g = &world.g;
    let mut s = String::with_capacity(2048);
    s.push('{');
    s.push_str(&format!("\"w\":{},\"h\":{},", g.w, g.h));
    s.push_str(&format!("\"status\":{},", status_code(world.status)));
    s.push_str(&format!("\"rejected\":{},", world.rejected));
    s.push_str(&format!("\"room\":{},", g.room_idx));
    s.push_str(&format!("\"potions\":{},", world.run.potions));

    // tiles
    s.push_str("\"tiles\":[");
    for (y, row) in g.tiles.iter().enumerate() {
        if y > 0 {
            s.push(',');
        }
        s.push('[');
        for (x, t) in row.iter().enumerate() {
            if x > 0 {
                s.push(',');
            }
            s.push_str(&tile_code(*t).to_string());
        }
        s.push(']');
    }
    s.push_str("],");

    // fire
    s.push_str("\"fire\":[");
    for (y, row) in g.fire.iter().enumerate() {
        if y > 0 {
            s.push(',');
        }
        s.push('[');
        for (x, v) in row.iter().enumerate() {
            if x > 0 {
                s.push(',');
            }
            s.push_str(&v.to_string());
        }
        s.push(']');
    }
    s.push_str("],");

    // entities
    s.push_str("\"ents\":[");
    for (i, e) in g.entities.iter().filter(|e| e.alive()).enumerate() {
        if i > 0 {
            s.push(',');
        }
        let ch = e.channel.is_some();
        let ready = e.channel.as_ref().is_some_and(|c| c.ready);
        s.push_str(&format!(
            "{{\"id\":{},\"k\":\"{}\",\"x\":{},\"y\":{},\"hp\":{},\"max\":{},\"haste\":{},\"ch\":{},\"ready\":{},\"slam\":{}}}",
            e.id, kind_str(e.kind), e.x, e.y, e.hp, e.maxhp, e.haste_turns, ch, ready,
            e.pending_slam
        ));
    }
    s.push_str("],");

    // boss 砸擊預告格(若有)
    s.push_str("\"slamCells\":[");
    if let Some(boss) = g.entities.iter().find(|e| e.kind == Kind::Boss && e.alive()) {
        if boss.pending_slam {
            if let Some(cells) = &boss.slam {
                for (i, (cx, cy)) in cells.iter().enumerate() {
                    if i > 0 {
                        s.push(',');
                    }
                    s.push_str(&format!("[{cx},{cy}]"));
                }
            }
        }
    }
    s.push_str("],");

    // acquired(spell codes)
    s.push_str("\"acquired\":[");
    for (i, sp) in world.run.acquired.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&spell_code(*sp).to_string());
    }
    s.push_str("],");

    // 法術等級(code:tier),供 ★ 顯示
    s.push_str("\"tiers\":{");
    for (i, sp) in world.run.acquired.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&format!("\"{}\":{}", spell_code(*sp), world.run.tiers.of(sp.id())));
    }
    s.push_str("},");

    // 順序鏈(前 8 手)
    s.push_str("\"chain\":[");
    for (i, slot) in project_chain(g, 8).iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&format!(
            "{{\"k\":\"{}\",\"rel\":{}}}",
            kind_str(slot.kind),
            slot.releasing
        ));
    }
    s.push(']');

    s.push('}');
    s
}

/// 可撿池大小(JS 啟動時可查,非必要)。
#[no_mangle]
pub extern "C" fn mr_pickable_count() -> u32 {
    pickable().len() as u32
}

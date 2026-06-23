//! 端到端 `step` 契約 + 確定性:用 init_room 載入真房間,透過 step 打、驗回放 bit 一致。

use magicraid_sim::{
    config, init_room, project_chain, step, Action, GameState, Kind, Reject, RunState, Spell,
    Status, Target,
};

fn alive_enemies(g: &GameState) -> usize {
    g.alive_enemies()
}

#[test]
fn room0_starts_awaiting_input_with_mage_first() {
    // 載入即輪到法師(time 全 0、mage 優先)。
    let g = init_room(0);
    let chain = project_chain(&g, 4);
    assert_eq!(chain[0].kind, Kind::Mage, "開局第一手應是法師");
}

#[test]
fn wait_advances_time_chain_and_enemies_move() {
    // 法師待機 → 時間鏈推進,敵人有機會行動(往法師靠近)。
    let mut g = init_room(0);
    let mut run = RunState::new(1);
    let imp_before = g
        .entities
        .iter()
        .find(|e| e.kind == Kind::Imp)
        .map(|e| (e.x, e.y))
        .unwrap();
    let r = step(&mut g, &mut run, Action::Wait);
    assert_eq!(r.rejected, None);
    assert_eq!(r.status, Status::AwaitingInput, "待機後仍回到法師輸入");
    let imp_after = g
        .entities
        .iter()
        .find(|e| e.kind == Kind::Imp)
        .map(|e| (e.x, e.y))
        .unwrap();
    assert_ne!(imp_before, imp_after, "敵人應在時間鏈中行動過");
}

#[test]
fn illegal_cast_is_rejected_without_time_passing() {
    let mut g = init_room(0);
    let mut run = RunState::new(1);
    let mage_time_before = g.mage().time;
    // 朝空地放魔法彈(沒有敵人)→ 拒絕。
    let r = step(
        &mut g,
        &mut run,
        Action::Cast {
            spell: Spell::Bolt,
            target: Target::cell(3, 3),
        },
    );
    assert_eq!(r.rejected, Some(Reject::NoEnemyThere));
    assert_eq!(g.mage().time, mage_time_before, "非法動作不應推進時間");
}

#[test]
fn move_to_walks_across_room() {
    let mut g = init_room(0);
    let mut run = RunState::new(1);
    let (sx, sy) = (g.mage().x, g.mage().y);
    // 往右走一格(空地)。
    let r = step(&mut g, &mut run, Action::MoveTo { x: sx + 1, y: sy });
    assert_eq!(r.rejected, None);
    assert_ne!((g.mage().x, g.mage().y), (sx, sy), "法師應移動");
}

#[test]
fn heavy_channel_then_release_resolves() {
    // 起手烈焰術(channel)→ 進入 AwaitingRelease;再 step 釋放 → 結算。
    let mut g = init_room(2); // 房間 3:有油,法師 (2,6)
    let mut run = RunState::new(1);
    let (mx, my) = (g.mage().x, g.mage().y);
    // 朝鄰近一格油施放(射程內、有視線)。目標選法師上方油區附近的格。
    let target = Target::cell(mx, my - 1);
    let r = step(
        &mut g,
        &mut run,
        Action::Cast {
            spell: Spell::Heavy,
            target,
        },
    );
    assert_eq!(r.rejected, None);
    // 蓄力後敵人可能插手,最終法師停在釋放手。
    assert_eq!(r.status, Status::AwaitingRelease, "撐過前搖應停在釋放手");
    assert!(g.mage().channel.is_some());
    // 釋放:任意 action 觸發。
    let r2 = step(&mut g, &mut run, Action::Wait);
    assert_eq!(r2.rejected, None);
    assert!(g.mage().channel.is_none(), "釋放後 channel 清空");
}

#[test]
fn potion_heals_and_consumes_charge() {
    let mut g = init_room(0);
    let mut run = RunState::new(1);
    // 先讓法師掉血。
    g.mage_mut().hp = 5;
    let before = run.potions;
    let r = step(&mut g, &mut run, Action::Potion);
    assert_eq!(r.rejected, None);
    assert_eq!(run.potions, before - 1, "應消耗一瓶");
    assert!(g.mage().hp > 5, "應回血");
    // 滿血時喝藥被拒。
    g.mage_mut().hp = g.mage().maxhp;
    let r2 = step(&mut g, &mut run, Action::Potion);
    assert_eq!(r2.rejected, Some(Reject::CannotDrink));
}

#[test]
fn clearing_room_yields_pick_offered() {
    // 把房間 0 的敵人全部移除 → 待機一手後應回報 PickOffered。
    let mut g = init_room(0);
    let mut run = RunState::new(1);
    for e in g.entities.iter_mut() {
        if !e.kind.is_mage() {
            e.hp = 0;
        }
    }
    let r = step(&mut g, &mut run, Action::Wait);
    assert_eq!(r.status, Status::PickOffered, "清房應給三選一");
}

#[test]
fn boss_death_yields_run_complete() {
    let boss_room = config::ROOMS.len() - 1;
    let mut g = init_room(boss_room);
    let mut run = RunState::new(1);
    let bi = g.entities.iter().position(|e| e.kind == Kind::Boss).unwrap();
    g.entities[bi].hp = 0;
    let r = step(&mut g, &mut run, Action::Wait);
    assert_eq!(r.status, Status::RunComplete, "魔像死 = 通關");
}

#[test]
fn mage_death_yields_defeat() {
    let mut g = init_room(0);
    let mut run = RunState::new(1);
    g.mage_mut().hp = 1;
    // 站到敵人旁讓它打死?簡化:直接設 1 血,讓敵人靠近多手後打到。
    // 用多手待機讓小鬼貼上來攻擊,最終擊倒。
    let mut status = Status::AwaitingInput;
    for _ in 0..40 {
        let r = step(&mut g, &mut run, Action::Wait);
        status = r.status;
        if status == Status::Defeat {
            break;
        }
    }
    assert_eq!(status, Status::Defeat, "法師被擊倒應 Defeat");
}

#[test]
fn full_step_replay_is_bit_identical() {
    // 固定種子 + 固定動作序列,跑兩份,逐欄位比對最終狀態與 event 流。
    let scenario = || {
        let mut g = init_room(2);
        let mut run = RunState::new(0xC0FFEE);
        let actions = [
            Action::MoveTo {
                x: g.mage().x,
                y: g.mage().y - 1,
            },
            Action::Wait,
            Action::Cast {
                spell: Spell::Bolt,
                target: Target::cell(7, 2), // 房間 3 的符文眼在 (7,2)
            },
            Action::Wait,
            Action::Wait,
        ];
        let mut all_events = Vec::new();
        for a in actions {
            let r = step(&mut g, &mut run, a);
            all_events.push((r.status, r.rejected, r.events));
        }
        let snap: Vec<(u32, i32, i32, i32, i64)> = g
            .entities
            .iter()
            .map(|e| (e.id, e.x, e.y, e.hp, e.time))
            .collect();
        (snap, g.tiles.clone(), g.fire.clone(), all_events, run.potions)
    };
    assert_eq!(scenario(), scenario(), "整段 step 回放必須 bit 一致");
}

#[test]
fn project_chain_reflects_haste_insertion() {
    // 加速 → 同樣 8 手內,法師出手次數應比等速更多(插隊)。
    let count_mage = |g: &GameState| {
        project_chain(g, 8)
            .iter()
            .filter(|s| s.kind == Kind::Mage)
            .count()
    };
    let base = count_mage(&init_room(0));
    let mut g = init_room(0);
    g.mage_mut().haste_turns = 5;
    let hasted = count_mage(&g);
    assert!(
        hasted > base,
        "加速應讓法師在鏈上更頻繁:base={base} hasted={hasted}"
    );
    let _ = alive_enemies(&g);
}

//! harness CLI:`harness [N] [budget]` —— 跑 seed 0..N,回報結果分佈、抵達房數、
//! 確定性回放是否 bit 一致、有無 panic。任一硬性檢查失敗 → 非零退出(可當 CI 閘門)。

use harness::{play, replay, room_count};
use magicraid_sim::Status;
use std::panic::{catch_unwind, AssertUnwindSafe};

fn main() {
    let mut args = std::env::args().skip(1);
    let n: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(500);
    let budget: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(5000);

    let rooms = room_count();
    let mut complete = 0u32;
    let mut defeat = 0u32;
    let mut timeout = 0u32;
    let mut panics = 0u32;
    let mut nondet = 0u32;
    let mut reach = vec![0u32; rooms + 1]; // reach[i] = 抵達(進入)房 i 的場數
    let mut total_steps = 0u64;

    for seed in 0..n {
        let res = catch_unwind(AssertUnwindSafe(|| {
            let t = play(seed, budget);
            let (rs, rsnap) = replay(seed, &t.ops);
            let det = rs == t.outcome && rsnap == t.final_snapshot;
            (t.outcome, t.max_room, t.steps, det)
        }));
        match res {
            Err(_) => panics += 1,
            Ok((outcome, max_room, steps, det)) => {
                total_steps += steps as u64;
                if !det {
                    nondet += 1;
                }
                for r in 0..=max_room.min(rooms) {
                    reach[r] += 1;
                }
                match outcome {
                    Status::RunComplete => complete += 1,
                    Status::Defeat => defeat += 1,
                    _ => timeout += 1,
                }
            }
        }
    }

    println!("=== magicraid harness ===  seeds=0..{n}  budget={budget}");
    println!("結果分佈:");
    println!("  ✅ RunComplete(通關)   {complete:>5}  ({:.1}%)", pct(complete, n));
    println!("  💀 Defeat(陣亡)        {defeat:>5}  ({:.1}%)", pct(defeat, n));
    println!("  ⏳ Timeout(超預算)     {timeout:>5}  ({:.1}%)", pct(timeout, n));
    println!("  🔥 Panic(崩潰)         {panics:>5}", );
    println!("  🎲 非確定性回放         {nondet:>5}");
    println!("平均步數/場: {:.1}", total_steps as f64 / n.max(1) as f64);
    println!("抵達各房場數(進入該房即計):");
    for (r, c) in reach.iter().enumerate().take(rooms) {
        println!("  房 {r}: {c:>5}");
    }

    // 硬性閘門(真 bug):無 panic、無非確定性回放、無卡死(timeout)。這是 harness 的核心職責。
    let mut ok = true;
    if panics > 0 {
        eprintln!("✗ 有 {panics} 場 panic");
        ok = false;
    }
    if nondet > 0 {
        eprintln!("✗ 有 {nondet} 場回放不一致(非確定性)");
        ok = false;
    }
    if timeout > 0 {
        eprintln!("✗ 有 {timeout} 場超出預算(疑似卡死)");
        ok = false;
    }

    // 可解性報告(每房至少一條解):房 r 被「進入」過代表前一房被解開。
    println!("\n可解性(baseline agent):");
    for r in 1..rooms {
        let mark = if reach[r] > 0 { "✓" } else { "✗" };
        println!("  房 {} 可解 {mark}(後續房被進入 {} 次)", r - 1, reach[r]);
    }
    let boss_cleared = complete > 0;
    println!(
        "  房 {}(boss)可解 {}",
        rooms - 1,
        if boss_cleared { "✓" } else { "✗(baseline 未攻克;設計上的技巧關 — 見備註)" }
    );

    if ok {
        println!("\n✓ 核心閘門全過:無崩潰、無非確定性、無卡死。");
        println!("  備註:boss 房是設計上的技巧關(急速閃砸 + 過熱爆發),貪婪 baseline 尚未攻克;");
        println!("        需更強 agent 或手寫解法序列才能證明 boss 可解(見 docs/07 backlog)。");
    } else {
        std::process::exit(1);
    }
}

fn pct(x: u32, n: u32) -> f64 {
    if n == 0 {
        0.0
    } else {
        100.0 * x as f64 / n as f64
    }
}

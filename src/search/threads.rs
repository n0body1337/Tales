// ============================================================================
// Tales - UCI chess engine written in Rust
// Copyright (C) 2025-2026 Andre MARTINS
//
// Tales is free software: you can redistribute it and/or modify it under
// the terms of the GNU General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.
//
// Tales is distributed in the hope that it will be useful, but WITHOUT ANY
// WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
// FOR A PARTICULAR PURPOSE. See the GNU General Public License for more
// details.
//
// You should have received a copy of the GNU General Public License along
// with this program. If not, see <https://www.gnu.org/licenses/>.
// ============================================================================

// Lazy SMP threading — multi-threaded search with shared transposition table.
// Each worker thread gets its own Searcher + eval_hash + Position copy.
// All threads share the same TransTable (benign data races, standard in chess).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering};
use std::thread;

use crate::board::moves::Move;
use crate::board::position::Position;
use crate::eval;
use crate::search::alphabeta;
use crate::search::ordering::Searcher;
use crate::tt::TransTable;

/// Shared state between threads (atomics + TT pointer)
pub struct SharedState {
    pub abort: AtomicBool,
    pub depth_reached: AtomicI32,
    pub total_nodes: AtomicU64,
}

/// Run Lazy SMP search with N threads.
/// Thread 0 is the "main" thread whose PV is used for output.
/// Odd-numbered threads search at depth+1 for diversity.
pub fn lazy_smp_search(
    pos: &mut Position,
    tt: &mut TransTable,
    par: &eval::params::EvalParams,
    eval_hash: &mut Vec<eval::EvalHashEntry>,
    pawn_tt: &mut eval::pawn_hash::PawnHash,
    num_threads: usize,
    max_depth: i32,
    time_limit_ms: u64,
    move_overhead_ms: u64,
    game_key: u64,
    nodes_limit: u64,
    nps_limit: i32,
    multi_pv: usize,
) {
    if num_threads <= 1 {
        // Single-threaded
        let mut searcher = Searcher::new();
        searcher.time_limit_ms = time_limit_ms;
        searcher.move_overhead_ms = move_overhead_ms;
        searcher.abort_search = false;
        searcher.nodes = 0;
        searcher.dp_completed = 0;
        searcher.pv_eng = [Move::NONE; 2];
        searcher.game_key = game_key;
        searcher.nodes_limit = nodes_limit;
        searcher.nps_limit = nps_limit;
        searcher.multi_pv = multi_pv;
        tt.new_search();

        if multi_pv > 1 {
            alphabeta::multi_pv(
                pos,
                &mut searcher,
                tt,
                par,
                eval_hash,
                pawn_tt,
                max_depth,
                multi_pv,
            );
        } else {
            alphabeta::iterate(pos, &mut searcher, tt, par, eval_hash, pawn_tt, max_depth);
        }
        return;
    }

    // Multi-threaded Lazy SMP
    let shared = Arc::new(SharedState {
        abort: AtomicBool::new(false),
        depth_reached: AtomicI32::new(0),
        total_nodes: AtomicU64::new(0),
    });

    // Use raw pointer for TT sharing (we've marked it Send+Sync)
    let tt_ptr = tt as *mut TransTable;
    tt.new_search();

    // Clone position and params for each thread
    let mut handles = Vec::with_capacity(num_threads);
    let mut results: Vec<Option<(i32, [Move; 2])>> = vec![None; num_threads];

    // Create results storage accessible from threads
    let results_ptr = results.as_mut_ptr();

    for thread_id in 0..num_threads {
        let mut thread_pos = pos.clone();
        let thread_par = par.clone();
        let mut thread_eval_hash = eval::new_eval_hash();
        let mut thread_pawn_tt = eval::pawn_hash::PawnHash::new();
        let shared_clone = Arc::clone(&shared);

        // SAFETY: tt_ptr points to a valid TransTable that outlives all threads.
        // Results ptr is valid because we join all threads before reading.
        let tt_raw = tt_ptr as usize; // send as usize (pointers aren't Send)
        let results_raw = results_ptr as usize;

        let handle = thread::spawn(move || {
            let tt_ref = unsafe { &mut *(tt_raw as *mut TransTable) };
            let results_slot =
                unsafe { &mut *(results_raw as *mut Option<(i32, [Move; 2])>).add(thread_id) };

            let mut searcher = Searcher::new();
            searcher.time_limit_ms = time_limit_ms;
            searcher.move_overhead_ms = move_overhead_ms;
            searcher.abort_search = false;
            searcher.nodes = 0;
            searcher.dp_completed = 0;
            searcher.pv_eng = [Move::NONE; 2];
            searcher.game_key = game_key;
            searcher.nodes_limit = nodes_limit;
            searcher.nps_limit = nps_limit;
            searcher.multi_pv = multi_pv;

            // Depth offset: odd threads search +1 deeper for diversity
            let depth_offset = (thread_id & 1) as i32;
            let effective_max_depth = max_depth + depth_offset;

            // Only thread 0 should print info lines
            let shut_up = thread_id != 0;

            // Run search
            if shut_up {
                iterate_silent(
                    &mut thread_pos,
                    &mut searcher,
                    tt_ref,
                    &thread_par,
                    &mut thread_eval_hash,
                    &mut thread_pawn_tt,
                    effective_max_depth,
                    &shared_clone,
                );
            } else {
                iterate_main(
                    &mut thread_pos,
                    &mut searcher,
                    tt_ref,
                    &thread_par,
                    &mut thread_eval_hash,
                    &mut thread_pawn_tt,
                    effective_max_depth,
                    &shared_clone,
                );
            }

            // Store results
            shared_clone
                .total_nodes
                .fetch_add(searcher.nodes, Ordering::Relaxed);
            *results_slot = Some((searcher.dp_completed, searcher.pv_eng));
        });

        handles.push(handle);
    }

    // Wait for all threads
    for handle in handles {
        let _ = handle.join();
    }

    // Pick best result (highest depth completed, thread 0 preferred)
    let mut best_depth = -1;
    let mut best_pv = [Move::NONE; 2];

    for (i, result) in results.iter().enumerate().take(num_threads) {
        if let Some((dp, pv)) = result
            && (*dp > best_depth || (*dp == best_depth && i == 0))
        {
            best_depth = *dp;
            best_pv = *pv;
        }
    }

    // Output best move
    if best_pv[0].is_none() {
        println!("bestmove 0000");
    } else if !best_pv[1].is_none() {
        println!(
            "bestmove {} ponder {}",
            best_pv[0].to_uci_string(),
            best_pv[1].to_uci_string()
        );
    } else {
        println!("bestmove {}", best_pv[0].to_uci_string());
    }
}

/// Main thread iterate — prints UCI info, uses Widen-style aspiration
fn iterate_main(
    pos: &mut Position,
    searcher: &mut Searcher,
    tt: &mut TransTable,
    par: &eval::params::EvalParams,
    eval_hash: &mut Vec<eval::EvalHashEntry>,
    pawn_tt: &mut eval::pawn_hash::PawnHash,
    max_depth: i32,
    shared: &Arc<SharedState>,
) {
    use crate::tt::{MATE, MAX_EVAL, MAX_PLY};

    let lmr = alphabeta::lmr_table();
    searcher.start_time = std::time::Instant::now();
    searcher.fl_root_choice = false;
    searcher.age_hist();

    let mut pv = [Move::NONE; MAX_PLY];
    let mut last_score = 0i32;

    for depth in 1..=max_depth {
        // Check if another thread triggered abort
        if shared.abort.load(Ordering::Relaxed) {
            searcher.abort_search = true;
            break;
        }

        // Skip if lagging behind
        let leader = shared.depth_reached.load(Ordering::Relaxed);
        if leader > searcher.dp_completed + 1 {
            searcher.dp_completed += 1;
            continue;
        }

        searcher.root_depth = depth;
        searcher.seldepth = 0;

        // Widen-style aspiration
        let cur_val = widen_shared(
            pos, searcher, tt, &lmr, par, eval_hash, pawn_tt, depth, &mut pv, last_score, shared,
        );

        if searcher.abort_search || shared.abort.load(Ordering::Relaxed) {
            searcher.abort_search = true;
            break;
        }

        last_score = cur_val;
        searcher.dp_completed = depth;
        shared.depth_reached.fetch_max(depth, Ordering::Relaxed);

        searcher.pv_eng[0] = pv[0];
        if !pv[1].is_none() {
            searcher.pv_eng[1] = pv[1];
        }

        // Print UCI info (only from main thread)
        let elapsed_ms = searcher.start_time.elapsed().as_millis().max(1) as u64;
        let total_nodes = shared.total_nodes.load(Ordering::Relaxed) + searcher.nodes;
        let nps = total_nodes * 1000 / elapsed_ms;
        let hf = tt.hashfull();

        let score_str = if cur_val > MAX_EVAL {
            format!("score mate {}", (MATE - cur_val + 1) / 2)
        } else if cur_val < -MAX_EVAL {
            format!("score mate -{}", (MATE + cur_val + 1) / 2)
        } else {
            format!("score cp {cur_val}")
        };

        let max_pv = 32;
        print!(
            "info depth {} seldepth {} {} nodes {} nps {} hashfull {} time {} pv",
            depth, searcher.seldepth, score_str, total_nodes, nps, hf, elapsed_ms
        );

        let mut i = 0;
        while i < max_pv && !pv[i].is_none() {
            print!(" {}", pv[i].to_uci_string());
            i += 1;
        }
        println!();

        // Shorten search if only one root move
        if depth >= 8 && !searcher.fl_root_choice {
            break;
        }

        // Abort on mate score
        if !(-MAX_EVAL..=MAX_EVAL).contains(&cur_val) {
            let mut max_mate_depth = (MATE - cur_val.abs() + 1) + 1;
            max_mate_depth = max_mate_depth * 4 / 3;
            if max_mate_depth <= depth {
                break;
            }
        }
    }

    // Signal other threads to stop
    shared.abort.store(true, Ordering::Relaxed);
}

/// Silent worker thread — searches without printing, uses Widen-style aspiration
fn iterate_silent(
    pos: &mut Position,
    searcher: &mut Searcher,
    tt: &mut TransTable,
    par: &eval::params::EvalParams,
    eval_hash: &mut Vec<eval::EvalHashEntry>,
    pawn_tt: &mut eval::pawn_hash::PawnHash,
    max_depth: i32,
    shared: &Arc<SharedState>,
) {
    use crate::tt::{MATE, MAX_EVAL, MAX_PLY};

    let lmr = alphabeta::lmr_table();
    searcher.start_time = std::time::Instant::now();
    searcher.fl_root_choice = false;
    searcher.age_hist();

    let mut pv = [Move::NONE; MAX_PLY];
    let mut last_score = 0i32;

    for depth in 1..=max_depth {
        if shared.abort.load(Ordering::Relaxed) || searcher.abort_search {
            break;
        }

        // Skip if lagging
        let leader = shared.depth_reached.load(Ordering::Relaxed);
        if leader > searcher.dp_completed + 1 {
            searcher.dp_completed += 1;
            continue;
        }

        searcher.root_depth = depth;
        searcher.seldepth = 0;

        // Widen-style aspiration
        let cur_val = widen_shared(
            pos, searcher, tt, &lmr, par, eval_hash, pawn_tt, depth, &mut pv, last_score, shared,
        );

        if searcher.abort_search || shared.abort.load(Ordering::Relaxed) {
            searcher.abort_search = true;
            break;
        }

        last_score = cur_val;
        searcher.dp_completed = depth;
        shared.depth_reached.fetch_max(depth, Ordering::Relaxed);

        searcher.pv_eng[0] = pv[0];
        if !pv[1].is_none() {
            searcher.pv_eng[1] = pv[1];
        }

        // Shorten search if only one root move
        if depth >= 8 && !searcher.fl_root_choice {
            break;
        }

        // Abort on mate score
        if !(-MAX_EVAL..=MAX_EVAL).contains(&cur_val) {
            let mut max_mate_depth = (MATE - cur_val.abs() + 1) + 1;
            max_mate_depth = max_mate_depth * 4 / 3;
            if max_mate_depth <= depth {
                break;
            }
        }
    }
}

/// Widen-style aspiration search for threaded iterate — checks shared abort
fn widen_shared(
    pos: &mut Position,
    searcher: &mut Searcher,
    tt: &mut TransTable,
    lmr: &alphabeta::LmrTable,
    par: &eval::params::EvalParams,
    eval_hash: &mut Vec<eval::EvalHashEntry>,
    pawn_tt: &mut eval::pawn_hash::PawnHash,
    depth: i32,
    pv: &mut [Move],
    last_score: i32,
    shared: &Arc<SharedState>,
) -> i32 {
    use crate::tt::{INF, MAX_EVAL};

    if depth > 6 && last_score.abs() < MAX_EVAL {
        let mut margin = 8;
        while margin < 500 {
            let alpha = last_score - margin;
            let beta = last_score + margin;
            let cur_val = alphabeta::search_root(
                pos, searcher, tt, lmr, par, eval_hash, pawn_tt, 0, alpha, beta, depth, pv,
            );
            if searcher.abort_search || shared.abort.load(Ordering::Relaxed) {
                searcher.abort_search = true;
                return cur_val;
            }
            if cur_val > alpha && cur_val < beta {
                return cur_val;
            }
            if cur_val > MAX_EVAL {
                break;
            }
            margin *= 2;
        }
    }

    // Full window search (fallback or depths <= 6)
    alphabeta::search_root(
        pos, searcher, tt, lmr, par, eval_hash, pawn_tt, 0, -INF, INF, depth, pv,
    )
}

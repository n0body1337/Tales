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

//! Lazy SMP threading — multi-threaded search with a shared transposition table.
//!
//! Each worker thread gets its own [`Searcher`], eval hash,
//! and [`Position`] copy. All threads share the same
//! [`TransTable`] (benign data races, standard in chess engines).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering};
use std::thread;

use crate::board::moves::Move;
use crate::board::position::Position;
use crate::eval;
use crate::search::alphabeta;
use crate::search::alphabeta::lmr_table;
use crate::search::ordering::{SearchCtx, Searcher};
use crate::search::uci_info;
use crate::tt::TransTable;

/// Shared state between threads (atomics + TT pointer).
pub struct SharedState {
    pub abort: AtomicBool,
    pub depth_reached: AtomicI32,
    pub total_nodes: AtomicU64,
}

/// SMP launch configuration — parameters for `lazy_smp_search` that are
/// fixed for the entire search and don't belong in the per-thread context.
pub struct SmpConfig {
    /// Number of search threads.
    pub num_threads: usize,
    /// Maximum search depth.
    pub max_depth: i32,
    /// Time limit in milliseconds.
    pub time_limit_ms: u64,
    /// Move overhead safety buffer in milliseconds.
    pub move_overhead_ms: u64,
    /// Per-game random key for eval blur.
    pub game_key: u64,
    /// Node count limit (0 = unlimited).
    pub nodes_limit: u64,
    /// NPS limit for strength clamping.
    pub nps_limit: i32,
    /// Number of PV lines to search.
    pub multi_pv: usize,
    /// Whether the engine is searching in ponder mode.
    pub is_pondering: bool,
    /// Real time limit to apply when ponderhit transitions search to normal.
    pub ponder_time_ms: u64,
    /// Whether the UCI Ponder option is enabled (controls bestmove ponder output).
    pub ponder_enabled: bool,
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
    cfg: &SmpConfig,
) {
    if cfg.num_threads <= 1 {
        // Single-threaded
        let mut searcher = Searcher::new();
        searcher.time_limit_ms = cfg.time_limit_ms;
        searcher.move_overhead_ms = cfg.move_overhead_ms;
        searcher.abort_search = false;
        searcher.nodes = 0;
        searcher.dp_completed = 0;
        searcher.pv_eng = [Move::NONE; 2];
        searcher.game_key = cfg.game_key;
        searcher.nodes_limit = cfg.nodes_limit;
        searcher.nps_limit = cfg.nps_limit;
        searcher.multi_pv = cfg.multi_pv;
        searcher.is_pondering = cfg.is_pondering;
        searcher.ponder_time_ms = cfg.ponder_time_ms;
        searcher.ponder_enabled = cfg.ponder_enabled;
        tt.new_search();
        let lmr = lmr_table();
        let mut ctx = SearchCtx {
            searcher: &mut searcher,
            tt,
            par,
            eval_hash,
            pawn_tt,
            lmr,
        };

        if cfg.multi_pv > 1 {
            alphabeta::multi_pv(&mut ctx, pos, cfg.max_depth, cfg.multi_pv);
        } else {
            alphabeta::iterate(&mut ctx, pos, cfg.max_depth);
        }
        alphabeta::print_bestmove(&ctx);
        return;
    }

    // Multi-threaded Lazy SMP
    let shared = Arc::new(SharedState {
        abort: AtomicBool::new(false),
        depth_reached: AtomicI32::new(0),
        total_nodes: AtomicU64::new(0),
    });

    // Share TT and results across threads via usize cast (standard SMP pattern).
    // SAFETY: tt outlives all threads (we join before returning).
    //         results[] slots are disjoint per thread_id; we join before reading.
    let tt_raw = tt as *mut TransTable as usize;
    tt.new_search();

    let mut handles = Vec::with_capacity(cfg.num_threads);
    let mut results: Vec<Option<(i32, [Move; 2])>> = vec![None; cfg.num_threads];
    let results_raw = results.as_mut_ptr() as usize;

    let time_limit_ms = cfg.time_limit_ms;
    let move_overhead_ms = cfg.move_overhead_ms;
    let game_key = cfg.game_key;
    let nodes_limit = cfg.nodes_limit;
    let nps_limit = cfg.nps_limit;
    let multi_pv = cfg.multi_pv;
    let max_depth = cfg.max_depth;
    let is_pondering = cfg.is_pondering;
    let ponder_time_ms = cfg.ponder_time_ms;
    let ponder_enabled = cfg.ponder_enabled;

    for thread_id in 0..cfg.num_threads {
        let mut thread_pos = pos.clone();
        let thread_par = par.clone();
        let mut thread_eval_hash = eval::new_eval_hash();
        let mut thread_pawn_tt = eval::pawn_hash::PawnHash::new();
        let shared_clone = Arc::clone(&shared);

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
            searcher.is_pondering = is_pondering;
            searcher.ponder_time_ms = ponder_time_ms;
            searcher.ponder_enabled = ponder_enabled;

            // Depth offset: odd threads search +1 deeper for diversity
            let depth_offset = (thread_id & 1) as i32;
            let effective_max_depth = max_depth + depth_offset;
            let is_main = thread_id == 0;

            let lmr = lmr_table();
            let mut ctx = SearchCtx {
                searcher: &mut searcher,
                tt: tt_ref,
                par: &thread_par,
                eval_hash: &mut thread_eval_hash,
                pawn_tt: &mut thread_pawn_tt,
                lmr,
            };

            iterate_threaded(
                &mut ctx,
                &mut thread_pos,
                effective_max_depth,
                &shared_clone,
                is_main,
            );

            // Store results
            shared_clone
                .total_nodes
                .fetch_add(ctx.searcher.nodes, Ordering::Relaxed);
            *results_slot = Some((ctx.searcher.dp_completed, ctx.searcher.pv_eng));
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

    for (i, result) in results.iter().enumerate().take(cfg.num_threads) {
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
    } else if best_pv[1].is_some() {
        println!(
            "bestmove {} ponder {}",
            best_pv[0].to_uci_string(),
            best_pv[1].to_uci_string()
        );
    } else {
        println!("bestmove {}", best_pv[0].to_uci_string());
    }
}

/// Threaded iterate — iterative deepening with aspiration windows and shared abort.
/// When `is_main` is true, prints UCI info lines and signals abort on completion.
fn iterate_threaded(
    ctx: &mut SearchCtx,
    pos: &mut Position,
    max_depth: i32,
    shared: &Arc<SharedState>,
    is_main: bool,
) {
    use crate::board::types::{MATE, MAX_EVAL, MAX_PLY};

    ctx.searcher.start_time = std::time::Instant::now();
    ctx.searcher.has_root_choice = false;
    ctx.searcher.age_hist();

    let mut pv = [Move::NONE; MAX_PLY];
    let mut last_score = 0i32;

    for depth in 1..=max_depth {
        if shared.abort.load(Ordering::Relaxed) || ctx.searcher.abort_search {
            ctx.searcher.abort_search = true;
            break;
        }

        // Skip if lagging behind the leader thread
        let leader = shared.depth_reached.load(Ordering::Relaxed);
        if leader > ctx.searcher.dp_completed + 1 {
            ctx.searcher.dp_completed += 1;
            continue;
        }

        ctx.searcher.root_depth = depth;
        ctx.searcher.seldepth = 0;

        // Aspiration search
        let cur_val = widen_shared(ctx, pos, depth, &mut pv, last_score, shared);

        if ctx.searcher.abort_search || shared.abort.load(Ordering::Relaxed) {
            ctx.searcher.abort_search = true;
            break;
        }

        last_score = cur_val;
        ctx.searcher.dp_completed = depth;
        shared.depth_reached.fetch_max(depth, Ordering::Relaxed);

        ctx.searcher.pv_eng[0] = pv[0];
        if pv[1].is_some() {
            ctx.searcher.pv_eng[1] = pv[1];
        }

        // Print UCI info (only from main thread)
        if is_main {
            let elapsed_ms = ctx.searcher.start_time.elapsed().as_millis().max(1) as u64;
            let total_nodes = shared.total_nodes.load(Ordering::Relaxed) + ctx.searcher.nodes;
            let nps = total_nodes * 1000 / elapsed_ms;
            let hf = ctx.tt.hashfull();

            let score_str = uci_info::format_score(cur_val);

            print!(
                "info depth {} seldepth {} {} nodes {} nps {} hashfull {} time {} pv",
                depth, ctx.searcher.seldepth, score_str, total_nodes, nps, hf, elapsed_ms
            );

            uci_info::print_pv(&pv, 32);
        }

        // Shorten search if only one root move
        if depth >= 8 && !ctx.searcher.has_root_choice {
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

    // Main thread signals other threads to stop
    if is_main {
        shared.abort.store(true, Ordering::Relaxed);
    }
}

/// Widen-style aspiration search for threaded iterate — checks shared abort
fn widen_shared(
    ctx: &mut SearchCtx,
    pos: &mut Position,
    depth: i32,
    pv: &mut [Move],
    last_score: i32,
    shared: &Arc<SharedState>,
) -> i32 {
    use crate::board::types::{INF, MAX_EVAL};

    if depth > 6 && last_score.abs() < MAX_EVAL {
        let mut margin = 8;
        while margin < 500 {
            let alpha = last_score - margin;
            let beta = last_score + margin;
            let cur_val = alphabeta::search_root(ctx, pos, 0, alpha, beta, depth, pv);
            if ctx.searcher.abort_search || shared.abort.load(Ordering::Relaxed) {
                ctx.searcher.abort_search = true;
                return cur_val;
            }
            if cur_val > alpha && cur_val < beta {
                return cur_val;
            }
            if !(-MAX_EVAL..=MAX_EVAL).contains(&cur_val) {
                break;
            }
            margin *= 2;
        }
    }

    // Full window search (fallback or depths <= 6)
    alphabeta::search_root(ctx, pos, 0, -INF, INF, depth, pv)
}

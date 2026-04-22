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

// Alpha-beta search.
// SearchRoot (root node) + Search (interior nodes).
// Features: NMP, LMR, futility, razoring, singular extension, LMP, PVS, Sherwin flag.

use crate::board::bitboard::{Bitboard, RANK_2_BB, RANK_7_BB};
use crate::board::moves::*;
use crate::board::position::{Position, Undo};
use crate::board::types::*;
use crate::eval;
use crate::search::ordering::*;
use crate::search::quiesce;
use crate::search::uci_info;
use crate::tt;

// ============================================================================
// LMR reduction table — initialized at startup
// Two tables: lmr[0] = zero-window, lmr[1] = PV (r-1)
// ============================================================================

/// LMR move dimension — clamped to 64 to keep table in L1 cache (32KB vs 128KB).
const LMR_MAX_MOVES: usize = 64;

pub struct LmrTable {
    pub table: [[[i32; LMR_MAX_MOVES]; MAX_PLY]; 2],
}

/// Get a reference to the global LMR table (computed once, reused forever).
pub fn lmr_table() -> &'static LmrTable {
    use std::sync::OnceLock;
    static LMR: OnceLock<Box<LmrTable>> = OnceLock::new();
    LMR.get_or_init(|| {
        let mut t = [[[0i32; LMR_MAX_MOVES]; MAX_PLY]; 2];
        #[allow(clippy::needless_range_loop)] // dp/mv used as both indices and math values
        for dp in 0..MAX_PLY {
            for mv in 0..LMR_MAX_MOVES {
                let mut r = 0i32;
                if dp != 0 && mv != 0 {
                    r = ((dp as f64).ln() * (mv as f64).ln() / 2.0) as i32;
                }
                t[0][dp][mv] = r; // zero-window node
                t[1][dp][mv] = (r - 1).max(0); // PV node (never negative)

                // reduction cannot exceed actual depth
                if t[0][dp][mv] > (dp as i32 - 1) {
                    t[0][dp][mv] = dp as i32 - 1;
                }
                if t[1][dp][mv] > (dp as i32 - 1) {
                    t[1][dp][mv] = dp as i32 - 1;
                }
            }
        }
        Box::new(LmrTable { table: t })
    })
}

// ============================================================================
// Search constants
// ============================================================================

const SNP_DEPTH: i32 = 3; // Static Null Move Pruning max depth
const RAZOR_DEPTH: i32 = 4; // Razoring max depth
const FUT_DEPTH: i32 = 6; // Futility max depth
const SELECTIVE_DEPTH: i32 = 6; // Max(SNP_DEPTH, RAZOR_DEPTH, FUT_DEPTH)

const RAZOR_MARGIN: [i32; 5] = [0, 300, 360, 420, 480];
const FUTILITY_MARGIN: [i32; 7] = [0, 100, 160, 220, 280, 340, 400];

/// Per-recursion state for interior-node search — arguments that change at each call.
pub struct SearchFrame {
    /// Whether the previous move was a null move.
    pub was_null: bool,
    /// The last move played (for refutation / countermove heuristics).
    pub last_move: Move,
    /// Square of the last capture (for recapture extensions).
    pub last_capt_sq: i32,
}

// ============================================================================
// SearchRoot — root-level search
// NO pruning at root (no SNP, NMP, razoring, futility).
// Has: TT probe, singular ext, IID, LMR, PVS, currmove.
// ============================================================================

/// Root-level search — iterates over legal moves with full-window alpha/beta at depth > 0.
pub fn search_root(
    ctx: &mut SearchCtx,
    pos: &mut Position,
    ply: usize,
    mut alpha: i32,
    beta: i32,
    depth: i32,
    pv: &mut [Move],
) -> i32 {
    // SAFETY: std::mem::zeroed() produces [Move(0); N] == [Move::NONE; N].
    // Move is repr(transparent) over u16 and Move::NONE == Move(0), so all-zeros is valid.
    let mut new_pv: [Move; MAX_PLY] = unsafe { std::mem::zeroed() };
    let mut best = -INF;
    let mut mv_tried = 0usize;
    // Quiet moves tried so far — used for history penalty on cutoff.
    let mut mv_quiet: [Move; MAX_MOVES] = unsafe { std::mem::zeroed() };
    let mut quiet_tried = 0usize;

    let is_pv = alpha != beta - 1;

    // Singular extension data
    let mut sing_move = Move::NONE;
    let mut sing_score = -INF;
    let mut can_sing = false;

    // EARLY EXIT
    ctx.tt.prefetch(pos.hash_key);
    ctx.searcher.nodes += 1;
    ctx.searcher.check_timeout();
    if ctx.searcher.abort_search && ctx.searcher.root_depth > 1 {
        return 0;
    }
    if ply > 0 {
        pv[0] = Move::NONE;
    }
    if pos.is_draw() && ply > 0 {
        return pos.draw_score(ctx.par.draw_score, ctx.par.prog_side);
    }

    // TT PROBE
    let mut tt_move = Move::NONE;
    if let Some(hit) = ctx
        .tt
        .retrieve(pos.hash_key, alpha, beta, depth, ply as i32)
    {
        tt_move = hit.best_move;
        if hit.cutoff {
            if hit.score >= beta {
                ctx.searcher
                    .update_history(pos, Move::NONE, tt_move, depth, ply);
            }
            if !is_pv {
                return hit.score;
            }
        }
    }

    // PREPARE FOR SINGULAR EXTENSION, SENPAI-STYLE
    if is_pv
        && depth > 5
        && let Some(hit) = ctx
            .tt
            .retrieve(pos.hash_key, alpha, beta, depth - 4, ply as i32)
        && hit.cutoff
        && hit.flag & tt::LOWER != 0
    {
        sing_move = hit.best_move;
        sing_score = hit.score;
        can_sing = true;
    }

    // MAX PLY GUARD
    if ply >= MAX_PLY - 1 {
        return eval::evaluate(
            pos,
            ctx.par,
            ctx.eval_hash,
            ctx.pawn_tt,
            ctx.searcher.game_key,
        );
    }

    let in_check = pos.in_check();

    // INTERNAL ITERATIVE DEEPENING
    if is_pv && !in_check && tt_move.is_none() && depth > 6 {
        let frame = SearchFrame {
            was_null: false,
            last_move: Move::NONE,
            last_capt_sq: -1,
        };
        search(ctx, pos, ply, alpha, beta, depth - 2, &frame, &mut new_pv);
        tt_move = ctx.tt.retrieve_move(pos.hash_key);
    }

    // PREPARE MOVE LOOP
    let ref_move = ctx.searcher.get_refutation(tt_move);
    let mut picker = MovePicker::new(
        tt_move,
        ref_move,
        -1, // no ref_sq at root
        ctx.searcher.killer[ply][0],
        ctx.searcher.killer[ply][1],
    );

    // MAIN MOVE LOOP
    loop {
        let (mv, mv_type) = picker.next_move(pos, &ctx.searcher.history);
        if mv.is_none() {
            break;
        }

        // MAKE MOVE
        let mv_hist_score = {
            let pc_idx = pos.pc[mv.from_sq() as usize].index();
            ctx.searcher.history[pc_idx][mv.to_sq() as usize]
        };
        let last_capt = if pos.pc[mv.to_sq() as usize] != NO_PC {
            mv.to_sq()
        } else {
            -1
        };

        let mut u = Undo::new();
        pos.do_move(mv, &mut u);
        if pos.illegal() {
            pos.undo_move(mv, &u);
            continue;
        }

        // MultiPV: skip moves in avoid list
        if ctx.searcher.is_avoid_move(mv) {
            pos.undo_move(mv, &u);
            continue;
        }

        // GATHER INFO
        let mut fl_extended = false;
        mv_tried += 1;
        if ply == 0 && mv_tried > 1 {
            ctx.searcher.has_root_choice = true;
        }

        // currmove output
        if ply == 0 && depth > 16 && !ctx.searcher.silent {
            println!(
                "info currmove {} currmovenumber {}",
                mv.to_uci_string(),
                mv_tried
            );
        }

        // SET NEW DEPTH
        let mut new_depth = depth - 1;

        // EXTENSIONS

        // Cache child in_check status (computed once instead of 6+ times)
        let child_in_check = pos.in_check();

        // 1. check extension, applied in PV nodes or at low depth
        if (is_pv || depth < 8) && child_in_check {
            new_depth += 1;
            fl_extended = true;
        }

        // 2. pawn to 7th rank extension
        if is_pv && depth < 6 {
            let piece_moved = pos.tp_on_sq(mv.to_sq());
            if piece_moved == P {
                let sq_bb = Bitboard(1u64 << mv.to_sq());
                if (sq_bb & (RANK_2_BB | RANK_7_BB)).is_not_empty() {
                    new_depth += 1;
                    fl_extended = true;
                }
            }
        }

        // 3. singular extension, Senpai-style
        if is_pv && depth > 5 && mv == sing_move && can_sing && !fl_extended {
            let new_alpha_s = -sing_score - 50;
            let mut mock_pv = [Move::NONE; 1];
            let frame = SearchFrame {
                was_null: false,
                last_move: Move::NONE,
                last_capt_sq: -1,
            };
            let sc = search(
                ctx,
                pos,
                ply + 1,
                new_alpha_s - 1,
                new_alpha_s,
                depth - 4,
                &frame,
                &mut mock_pv,
            );
            if sc <= new_alpha_s {
                new_depth += 1;
            }
        }

        // Track quiet moves for history penalty
        if mv_type == MoveKind::Normal || mv_type == MoveKind::Refutation {
            mv_quiet[quiet_tried] = mv;
            quiet_tried += 1;
        }

        // LMR (NORMAL MOVES)
        let mut reduction = 0;
        if depth > 2
            && mv_tried > 3
            && !in_check
            && !child_in_check
            && ctx.lmr.table[is_pv as usize][depth.min(63) as usize]
                [mv_tried.min(LMR_MAX_MOVES - 1)]
                > 0
            && mv_type == MoveKind::Normal
            && mv_hist_score < ctx.par.hist_limit
            && mv.move_type() != CASTLE
        {
            reduction = ctx.lmr.table[is_pv as usize][depth.min(63) as usize]
                [mv_tried.min(LMR_MAX_MOVES - 1)];

            // increase reduction on bad history score
            if mv_hist_score < 0 && new_depth - reduction >= 2 {
                reduction += 1;
            }

            new_depth -= reduction;
        }

        // PVS
        let mut score;
        loop {
            let frame = SearchFrame {
                was_null: false,
                last_move: mv,
                last_capt_sq: last_capt,
            };
            if best == -INF {
                score = -search(
                    ctx,
                    pos,
                    ply + 1,
                    -beta,
                    -alpha,
                    new_depth,
                    &frame,
                    &mut new_pv,
                );
            } else {
                score = -search(
                    ctx,
                    pos,
                    ply + 1,
                    -alpha - 1,
                    -alpha,
                    new_depth,
                    &frame,
                    &mut new_pv,
                );
                if !ctx.searcher.abort_search && score > alpha && score < beta {
                    score = -search(
                        ctx,
                        pos,
                        ply + 1,
                        -beta,
                        -alpha,
                        new_depth,
                        &frame,
                        &mut new_pv,
                    );
                }
            }

            // DON'T REDUCE A MOVE THAT SCORED ABOVE ALPHA
            if score > alpha && reduction > 0 {
                new_depth += reduction;
                reduction = 0;
                continue; // re-search at full depth
            }
            break;
        }

        // UNDO MOVE
        pos.undo_move(mv, &u);
        if ctx.searcher.abort_search && ctx.searcher.root_depth > 1 {
            return 0;
        }

        // BETA CUTOFF
        if score >= beta {
            if !in_check {
                ctx.searcher.update_history(pos, Move::NONE, mv, depth, ply);
                // Penalize all quiet moves tried before the cutoff move.
                for &mv_p in &mv_quiet[..quiet_tried.saturating_sub(1)] {
                    ctx.searcher.decrease_history(pos, mv_p, depth);
                }
            }
            ctx.tt
                .store(pos.hash_key, mv, score, tt::LOWER, depth, ply as i32);

            // At root, build and display PV
            if ply == 0 {
                quiesce::build_pv(pv, &new_pv, mv);
                display_pv(ctx, depth, score, pv);
            }

            return score;
        }

        // NEW BEST MOVE
        if score > best {
            best = score;
            if score > alpha {
                alpha = score;

                quiesce::build_pv(pv, &new_pv, mv);
                if ply == 0 {
                    display_pv(ctx, depth, score, pv);
                }
            }
        }
    }

    // CHECKMATE / STALEMATE
    if best == -INF {
        if in_check {
            return -MATE + ply as i32;
        }
        return pos.draw_score(ctx.par.draw_score, ctx.par.prog_side);
    }

    // STORE TO TT
    if pv[0] != Move::NONE {
        if !in_check {
            ctx.searcher
                .update_history(pos, Move::NONE, pv[0], depth, ply);
            // Penalize all quiet moves tried except the best move.
            for &mv_p in &mv_quiet[..quiet_tried] {
                if mv_p != pv[0] {
                    ctx.searcher.decrease_history(pos, mv_p, depth);
                }
            }
        }
        ctx.tt
            .store(pos.hash_key, pv[0], best, tt::EXACT, depth, ply as i32);
    } else {
        ctx.tt
            .store(pos.hash_key, Move::NONE, best, tt::UPPER, depth, ply as i32);
    }

    best
}

// ============================================================================
// Search — main interior-node search (alpha-beta with pruning and extensions)
// ============================================================================

/// Recursive alpha-beta search with null-move, LMR, futility, and check extensions.
///
/// The 8 parameters are the irreducible set for recursive alpha-beta: context,
/// position, ply, alpha, beta, depth, per-call frame, and PV output buffer.
/// `SearchCtx` already bundles all shared mutable state; `SearchFrame` bundles
/// per-call parent info.
#[allow(clippy::too_many_arguments)]
pub fn search(
    ctx: &mut SearchCtx,
    pos: &mut Position,
    ply: usize,
    mut alpha: i32,
    mut beta: i32,
    depth: i32,
    frame: &SearchFrame,
    pv: &mut [Move],
) -> i32 {
    let was_null = frame.was_null;
    let last_move = frame.last_move;
    let last_capt_sq = frame.last_capt_sq;
    // SAFETY: std::mem::zeroed() produces [Move(0); N] == [Move::NONE; N].
    // Move is repr(transparent) over u16 and Move::NONE == Move(0), so all-zeros is valid.
    let mut new_pv: [Move; MAX_PLY] = unsafe { std::mem::zeroed() };
    let mut mv_tried = 0usize;
    // Quiet moves tried so far — used for history penalty on cutoff.
    let mut mv_quiet: [Move; MAX_MOVES] = unsafe { std::mem::zeroed() };
    let mut quiet_tried = 0usize;
    let mut ref_sq: i32 = -1;

    let is_pv = alpha != beta - 1;

    // Singular extension data
    let mut sing_move = Move::NONE;
    let mut sing_score = -INF;
    let mut can_sing = false;
    let mut did_null = false;

    // QUIESCENCE SEARCH ENTRY POINT
    if depth <= 0 {
        return quiesce::quiesce_checks(ctx, pos, ply, alpha, beta, &mut new_pv);
    }

    // EARLY EXIT
    ctx.tt.prefetch(pos.hash_key);
    ctx.searcher.nodes += 1;
    if ply > ctx.searcher.seldepth {
        ctx.searcher.seldepth = ply;
    }
    ctx.searcher.check_timeout();
    if ctx.searcher.abort_search && ctx.searcher.root_depth > 1 {
        return 0;
    }
    pv[0] = Move::NONE;
    if pos.is_draw() {
        return pos.draw_score(ctx.par.draw_score, ctx.par.prog_side);
    }

    // MATE DISTANCE PRUNING
    let checkmating_score = MATE - ply as i32;
    if checkmating_score < beta {
        beta = checkmating_score;
        if alpha >= checkmating_score {
            return alpha;
        }
    }
    let checkmated_score = -MATE + ply as i32;
    if checkmated_score > alpha {
        alpha = checkmated_score;
        if beta <= checkmated_score {
            return beta;
        }
    }

    // TT PROBE
    let mut tt_move = Move::NONE;
    if let Some(hit) = ctx
        .tt
        .retrieve(pos.hash_key, alpha, beta, depth, ply as i32)
    {
        tt_move = hit.best_move;
        if hit.cutoff {
            if hit.score >= beta {
                ctx.searcher
                    .update_history(pos, last_move, tt_move, depth, ply);
            }
            if !is_pv {
                return hit.score;
            }
        }
    }

    // PREPARE FOR SINGULAR EXTENSION, SENPAI-STYLE
    if is_pv
        && depth > 5
        && let Some(hit) = ctx
            .tt
            .retrieve(pos.hash_key, alpha, beta, depth - 4, ply as i32)
        && hit.cutoff
        && hit.flag & tt::LOWER != 0
    {
        sing_move = hit.best_move;
        sing_score = hit.score;
        can_sing = true;
    }

    // MAX PLY GUARD
    if ply >= MAX_PLY - 1 {
        return eval::evaluate(
            pos,
            ctx.par,
            ctx.eval_hash,
            ctx.pawn_tt,
            ctx.searcher.game_key,
        );
    }

    let in_check = pos.in_check();

    // Can we apply forward-pruning heuristics at this node?
    let can_prune = !in_check && !is_pv && alpha > -MAX_EVAL && beta < MAX_EVAL;

    // GET EVAL FOR PRUNING
    let eval_score = if can_prune && (!was_null || depth <= SELECTIVE_DEPTH) {
        eval::evaluate(
            pos,
            ctx.par,
            ctx.eval_hash,
            ctx.pawn_tt,
            ctx.searcher.game_key,
        )
    } else {
        0
    };

    // STATIC NULL MOVE PRUNING / BETA PRUNING
    if can_prune && depth <= SNP_DEPTH && !was_null {
        let sc = eval_score - 120 * depth;
        if sc > beta {
            return sc;
        }
    }

    // NULL MOVE PRUNING
    if depth > 1 && !was_null && can_prune && pos.may_null() && eval_score >= beta {
        did_null = true;

        // Null move depth reduction — modified Stockfish formula
        let new_depth = depth - ((823 + 67 * depth) / 256) - ((eval_score - beta) / 200).min(3);

        // Omit null move search if normal search to the same depth wouldn't exceed beta
        // (sometimes free via hash table)
        if let Some(hit) = ctx
            .tt
            .retrieve(pos.hash_key, alpha, beta, new_depth, ply as i32)
            && hit.cutoff
            && hit.score < beta
        {
            // skip null move — equivalent of goto avoid_null
            did_null = false;
        }

        if did_null {
            let mut u = Undo::new();
            pos.do_null(&mut u);
            let frame = SearchFrame {
                was_null: true,
                last_move: Move::NONE,
                last_capt_sq: -1,
            };
            let score = if new_depth <= 0 {
                -quiesce::quiesce_checks(ctx, pos, ply + 1, -beta, -beta + 1, &mut new_pv)
            } else {
                -search(
                    ctx,
                    pos,
                    ply + 1,
                    -beta,
                    -beta + 1,
                    new_depth,
                    &frame,
                    &mut new_pv,
                )
            };

            // Get null-refutation square from TT
            if let Some(hit) = ctx
                .tt
                .retrieve(pos.hash_key, alpha, beta, depth, ply as i32)
                && hit.best_move.is_some()
            {
                ref_sq = hit.best_move.to_sq();
            }

            pos.undo_null(&u);
            if ctx.searcher.abort_search && ctx.searcher.root_depth > 1 {
                return 0;
            }

            // Do not return unproved mate scores, Stockfish-style
            let score = if score >= MAX_EVAL { beta } else { score };

            if score >= beta {
                // Verification search
                if new_depth > 6 {
                    let frame = SearchFrame {
                        was_null: true,
                        last_move,
                        last_capt_sq,
                    };
                    let v_score = search(ctx, pos, ply, alpha, beta, new_depth - 5, &frame, pv);
                    if ctx.searcher.abort_search && ctx.searcher.root_depth > 1 {
                        return 0;
                    }
                    if v_score >= beta {
                        return v_score;
                    }
                } else {
                    return score;
                }
            }
        }
    }

    // RAZORING (based on Toga II 3.0)
    if can_prune
        && tt_move.is_none()
        && !was_null
        && (pos.pawns(pos.side) & if pos.side == WC { RANK_7_BB } else { RANK_2_BB }).is_empty()
        && depth <= RAZOR_DEPTH
    {
        let threshold = beta - RAZOR_MARGIN[depth as usize];
        if eval_score < threshold {
            let score = quiesce::quiesce_checks(ctx, pos, ply, alpha, beta, &mut new_pv);
            if score < threshold {
                return score;
            }
        }
    }

    // INTERNAL ITERATIVE DEEPENING
    if is_pv && !in_check && tt_move.is_none() && depth > 6 {
        let frame = SearchFrame {
            was_null: false,
            last_move: Move::NONE,
            last_capt_sq,
        };
        search(ctx, pos, ply, alpha, beta, depth - 2, &frame, &mut new_pv);
        tt_move = ctx.tt.retrieve_move(pos.hash_key);
    }

    // PREPARE MOVE LOOP
    // Use Refutation(hash_move) — continuation heuristic for move ordering.
    let ref_move = ctx.searcher.get_refutation(tt_move);

    let mut picker = MovePicker::new(
        tt_move,
        ref_move,
        ref_sq,
        ctx.searcher.killer[ply][0],
        ctx.searcher.killer[ply][1],
    );

    let mut best = -INF;
    let mut hash_flag = tt::UPPER;
    let mut do_futility = false;

    // MAIN MOVE LOOP
    loop {
        let (mv, mv_type) = picker.next_move(pos, &ctx.searcher.history);
        if mv.is_none() {
            break;
        }

        // SET FUTILITY PRUNING FLAG (before first applicable quiet move)
        if mv_type == MoveKind::Normal
            && quiet_tried == 0
            && can_prune
            && depth <= FUT_DEPTH
            && eval_score + FUTILITY_MARGIN[depth as usize] < beta
        {
            do_futility = true;
        }

        // MAKE MOVE
        let mv_hist_score = {
            let pc_idx = pos.pc[mv.from_sq() as usize].index();
            ctx.searcher.history[pc_idx][mv.to_sq() as usize]
        };
        let last_capt = if pos.pc[mv.to_sq() as usize] != NO_PC {
            mv.to_sq()
        } else {
            -1
        };

        let mut u = Undo::new();
        pos.do_move(mv, &mut u);
        if pos.illegal() {
            pos.undo_move(mv, &u);
            continue;
        }

        // GATHER INFO
        let mut fl_extended = false;
        mv_tried += 1;
        if ply == 0 && mv_tried > 1 {
            ctx.searcher.has_root_choice = true;
        }

        // SET NEW DEPTH
        let mut new_depth = depth - 1;

        // EXTENSIONS

        // Cache child in_check status (computed once instead of 6+ times)
        let child_in_check = pos.in_check();

        // 1. check extension, applied in PV nodes or at low depth
        if (is_pv || depth < 8) && child_in_check {
            new_depth += 1;
            fl_extended = true;
        }

        // 2. recapture extension in PV nodes
        if is_pv && mv.to_sq() == last_capt_sq {
            new_depth += 1;
            fl_extended = true;
        }

        // 3. pawn to 7th rank extension at tips of PV line
        if is_pv && depth < 6 {
            let piece_moved = pos.tp_on_sq(mv.to_sq());
            if piece_moved == P {
                let sq_bb = Bitboard(1u64 << mv.to_sq());
                if (sq_bb & (RANK_2_BB | RANK_7_BB)).is_not_empty() {
                    new_depth += 1;
                    fl_extended = true;
                }
            }
        }

        // 4. singular extension, Senpai-style
        if is_pv && depth > 5 && mv == sing_move && can_sing && !fl_extended {
            let new_alpha_s = -sing_score - 50;
            let mut mock_pv = [Move::NONE; 1];
            let frame = SearchFrame {
                was_null: false,
                last_move: Move::NONE,
                last_capt_sq: -1,
            };
            let sc = search(
                ctx,
                pos,
                ply + 1,
                new_alpha_s - 1,
                new_alpha_s,
                depth - 4,
                &frame,
                &mut mock_pv,
            );
            if sc <= new_alpha_s {
                new_depth += 1;
            }
        }

        // FUTILITY PRUNING
        if do_futility
            && !child_in_check
            && mv_hist_score < ctx.par.hist_limit
            && mv_type == MoveKind::Normal
            && mv_tried > 1
        {
            pos.undo_move(mv, &u);
            continue;
        }

        // LATE MOVE PRUNING
        if can_prune
            && depth <= 3
            && quiet_tried > (3 * depth) as usize
            && !child_in_check
            && mv_hist_score < ctx.par.hist_limit
            && mv_type == MoveKind::Normal
        {
            pos.undo_move(mv, &u);
            continue;
        }

        // Track quiet moves for history penalty (AFTER pruning to avoid gaps)
        if mv_type == MoveKind::Normal || mv_type == MoveKind::Refutation {
            mv_quiet[quiet_tried] = mv;
            quiet_tried += 1;
        }

        // SHERWIN FLAG — set flag responsible for increasing reduction
        let mut sherwin_flag = false;
        if did_null && depth > 2 && !child_in_check {
            let q_score = quiesce::quiesce_checks(ctx, pos, ply, -beta, -beta + 1, pv);
            if q_score >= beta {
                sherwin_flag = true;
            }
        }

        // LMR 1: NORMAL MOVES
        let mut reduction = 0;
        if depth > 2
            && mv_tried > 3
            && !in_check
            && !child_in_check
            && ctx.lmr.table[is_pv as usize][depth.min(63) as usize]
                [mv_tried.min(LMR_MAX_MOVES - 1)]
                > 0
            && mv_type == MoveKind::Normal
            && mv_hist_score < ctx.par.hist_limit
            && mv.move_type() != CASTLE
        {
            reduction = ctx.lmr.table[is_pv as usize][depth.min(63) as usize]
                [mv_tried.min(LMR_MAX_MOVES - 1)];

            // increase reduction when Sherwin flag is set
            if sherwin_flag && new_depth - reduction >= 2 {
                reduction += 1;
            }

            // increase reduction on bad history score
            if mv_hist_score < 0 && new_depth - reduction >= 2 {
                reduction += 1;
            }

            // decrease reduction on good history score (but never fully cancel LMR)
            if mv_hist_score > ctx.par.hist_limit && reduction >= 2 {
                reduction -= 1;
            }

            new_depth -= reduction;
        }

        // LMR 2: MARGINAL REDUCTION OF BAD CAPTURES
        if depth > 2
            && mv_tried > 6
            && alpha > -MAX_EVAL
            && beta < MAX_EVAL
            && !in_check
            && !child_in_check
            && mv_type == MoveKind::BadCapt
            && !is_pv
        {
            reduction = 1;
            new_depth -= reduction;
        }

        // PVS
        let mut score;
        loop {
            let frame = SearchFrame {
                was_null: false,
                last_move: mv,
                last_capt_sq: last_capt,
            };
            if best == -INF {
                score = -search(
                    ctx,
                    pos,
                    ply + 1,
                    -beta,
                    -alpha,
                    new_depth,
                    &frame,
                    &mut new_pv,
                );
            } else {
                score = -search(
                    ctx,
                    pos,
                    ply + 1,
                    -alpha - 1,
                    -alpha,
                    new_depth,
                    &frame,
                    &mut new_pv,
                );
                if !ctx.searcher.abort_search && score > alpha && score < beta {
                    score = -search(
                        ctx,
                        pos,
                        ply + 1,
                        -beta,
                        -alpha,
                        new_depth,
                        &frame,
                        &mut new_pv,
                    );
                }
            }

            // DON'T REDUCE A MOVE THAT SCORED ABOVE ALPHA
            if score > alpha && reduction > 0 {
                new_depth += reduction;
                reduction = 0;
                continue; // re-search
            }
            break;
        }

        // UNDO MOVE
        pos.undo_move(mv, &u);
        if ctx.searcher.abort_search && ctx.searcher.root_depth > 1 {
            return 0;
        }

        // BETA CUTOFF
        if score >= beta {
            if !in_check {
                ctx.searcher.update_history(pos, last_move, mv, depth, ply);
                // Penalize all quiet moves tried before the cutoff move.
                for &mv_p in &mv_quiet[..quiet_tried.saturating_sub(1)] {
                    ctx.searcher.decrease_history(pos, mv_p, depth);
                }
            }
            ctx.tt
                .store(pos.hash_key, mv, score, tt::LOWER, depth, ply as i32);
            return score;
        }

        // NEW BEST
        if score > best {
            best = score;
            if score > alpha {
                alpha = score;
                hash_flag = tt::EXACT;
                quiesce::build_pv(pv, &new_pv, mv);
            }
        }
    }

    // CHECKMATE / STALEMATE
    if mv_tried == 0 {
        if in_check {
            return -MATE + ply as i32;
        }
        return pos.draw_score(ctx.par.draw_score, ctx.par.prog_side);
    }

    // STORE TO TT
    if hash_flag == tt::EXACT {
        if !in_check {
            ctx.searcher
                .update_history(pos, last_move, pv[0], depth, ply);
            // Penalize all quiet moves tried except the best move.
            for &mv_p in &mv_quiet[..quiet_tried] {
                if mv_p != pv[0] {
                    ctx.searcher.decrease_history(pos, mv_p, depth);
                }
            }
        }
        ctx.tt
            .store(pos.hash_key, pv[0], best, tt::EXACT, depth, ply as i32);
    } else {
        ctx.tt
            .store(pos.hash_key, Move::NONE, best, tt::UPPER, depth, ply as i32);
    }

    best
}

// ============================================================================
// Iterate — iterative deepening with aspiration windows.
// Uses aspiration search at depth > 6 with margin 8.
// ============================================================================

/// Iterative deepening loop — drives depth progression with aspiration windows.
pub fn iterate(ctx: &mut SearchCtx, pos: &mut Position, max_depth: i32) {
    ctx.searcher.nodes = 0;
    ctx.searcher.abort_search = false;
    ctx.searcher.start_time = std::time::Instant::now();
    ctx.searcher.has_root_choice = false;

    let mut pv = [Move::NONE; MAX_PLY];
    let mut last_score = 0i32;

    // tt.new_search() is called by lazy_smp_search() before iterate(),
    // matching the approach where tt_date is incremented once in the go handler.
    ctx.searcher.age_hist();

    for depth in 1..=max_depth {
        ctx.searcher.root_depth = depth;
        ctx.searcher.seldepth = 0;

        // Aspiration search
        let cur_val = widen(ctx, pos, depth, &mut pv, last_score);

        if ctx.searcher.abort_search {
            break;
        }

        last_score = cur_val;
        ctx.searcher.dp_completed = depth;

        // Save engine's best/ponder moves
        ctx.searcher.pv_eng[0] = pv[0];
        if pv[1].is_some() {
            ctx.searcher.pv_eng[1] = pv[1];
        }

        // Shorten search if there is only one root move available
        if depth >= 8 && !ctx.searcher.has_root_choice {
            break;
        }

        // Abort search on finding checkmate score
        if !(-MAX_EVAL..=MAX_EVAL).contains(&cur_val) {
            let mut max_mate_depth = (MATE - cur_val.abs() + 1) + 1;
            max_mate_depth = max_mate_depth * 4 / 3;
            if max_mate_depth <= depth {
                break;
            }
        }
    }
}

/// Print the UCI `bestmove` line from the engine's saved PV.
/// Only includes `ponder <move>` when the Ponder UCI option is enabled.
pub fn print_bestmove(ctx: &SearchCtx) {
    if ctx.searcher.pv_eng[0].is_none() {
        println!("bestmove 0000");
    } else if ctx.searcher.ponder_enabled && ctx.searcher.pv_eng[1].is_some() {
        println!(
            "bestmove {} ponder {}",
            ctx.searcher.pv_eng[0].to_uci_string(),
            ctx.searcher.pv_eng[1].to_uci_string()
        );
    } else {
        println!("bestmove {}", ctx.searcher.pv_eng[0].to_uci_string());
    }
}

/// Aspiration search — widens window on fail-high/fail-low
fn widen(
    ctx: &mut SearchCtx,
    pos: &mut Position,
    depth: i32,
    pv: &mut [Move],
    last_score: i32,
) -> i32 {
    if depth > 6 && last_score.abs() < MAX_EVAL {
        let mut margin = 8;
        while margin < 500 {
            let alpha = last_score - margin;
            let beta = last_score + margin;
            let cur_val = search_root(ctx, pos, 0, alpha, beta, depth, pv);
            if ctx.searcher.abort_search {
                return cur_val;
            }
            if cur_val > alpha && cur_val < beta {
                return cur_val;
            }
            if !(-MAX_EVAL..=MAX_EVAL).contains(&cur_val) {
                break;
            } // verify mate with infinite bounds
            margin *= 2;
        }
    }

    // Full window search (fallback or depths <= 6)
    search_root(ctx, pos, 0, -INF, INF, depth, pv)
}

// ============================================================================
// MultiPV — multi-principal-variation search
// ============================================================================

/// Multi-PV driver — runs `search_root` once per PV line with excluded-move masking.
pub fn multi_pv(ctx: &mut SearchCtx, pos: &mut Position, max_depth: i32, num_pvs: usize) {
    ctx.searcher.nodes = 0;
    ctx.searcher.abort_search = false;
    ctx.searcher.start_time = std::time::Instant::now();
    ctx.searcher.has_root_choice = false;
    // tt.new_search() is called by lazy_smp_search() before multi_pv(),
    // matching the approach where tt_date is incremented once in the go handler.
    ctx.searcher.age_hist();

    const MAX_MPV: usize = 64;
    let num_pvs = num_pvs.min(MAX_MPV);

    let mut pv_lines: Vec<[Move; MAX_PLY]> = vec![[Move::NONE; MAX_PLY]; num_pvs + 1];
    let mut pv_scores = vec![0i32; num_pvs + 1];
    let mut best_pv_idx = 0usize;

    for depth in 1..=max_depth {
        ctx.searcher.clear_avoid_list();
        let mut best_score = -INF;
        best_pv_idx = 0;

        for pv_idx in 0..num_pvs {
            ctx.searcher.root_depth = depth;
            ctx.searcher.seldepth = 0;

            let score = widen(ctx, pos, depth, &mut pv_lines[pv_idx], pv_scores[pv_idx]);

            if ctx.searcher.abort_search {
                break;
            }

            pv_scores[pv_idx] = score;
            if score > best_score {
                best_score = score;
                best_pv_idx = pv_idx;
            }

            // Add this PV's best move to avoid list
            if pv_lines[pv_idx][0].is_some() {
                ctx.searcher.set_avoid_move(pv_lines[pv_idx][0]);
            }
        }

        if ctx.searcher.abort_search {
            break;
        }

        ctx.searcher.dp_completed = depth;

        // Print all PV lines for this depth (reverse order for conventional display)
        for pv_idx in (0..num_pvs).rev() {
            if pv_lines[pv_idx][0].is_none() {
                continue;
            }

            let elapsed_ms = ctx.searcher.start_time.elapsed().as_millis().max(1) as u64;
            let nps = ctx.searcher.nodes * 1000 / elapsed_ms;
            let hf = ctx.tt.hashfull();
            let score = pv_scores[pv_idx];

            let score_str = uci_info::format_score(score);

            print!(
                "info depth {} seldepth {} multipv {} {} nodes {} nps {} hashfull {} time {} pv",
                depth,
                ctx.searcher.seldepth,
                pv_idx + 1,
                score_str,
                ctx.searcher.nodes,
                nps,
                hf,
                elapsed_ms
            );

            let mut i = 0;
            while i < MAX_PV && pv_lines[pv_idx][i].is_some() {
                print!(" {}", pv_lines[pv_idx][i].to_uci_string());
                i += 1;
            }
            println!();
        }
    }

    // Use the best PV's first move as bestmove
    let best_mv = pv_lines[best_pv_idx][0];
    let ponder_mv = pv_lines[best_pv_idx][1];

    ctx.searcher.pv_eng[0] = best_mv;
    ctx.searcher.pv_eng[1] = ponder_mv;
}

// ============================================================================
// Display helpers
// ============================================================================

fn display_pv(ctx: &SearchCtx, depth: i32, score: i32, pv: &[Move]) {
    if ctx.searcher.silent {
        return;
    }
    if ctx.searcher.multi_pv > 1 {
        return;
    } // MultiPV prints its own info lines

    let elapsed_ms = ctx.searcher.start_time.elapsed().as_millis().max(1) as u64;
    let nps = ctx.searcher.nodes * 1000 / elapsed_ms;
    let hf = ctx.tt.hashfull();

    let score_str = uci_info::format_score(score);

    print!(
        "info depth {} seldepth {} {} nodes {} nps {} hashfull {} time {} pv",
        depth, ctx.searcher.seldepth, score_str, ctx.searcher.nodes, nps, hf, elapsed_ms
    );

    let mut i = 0;
    while i < MAX_PV && pv[i].is_some() {
        print!(" {}", pv[i].to_uci_string());
        i += 1;
    }
    println!();
}

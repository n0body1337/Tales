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

//! Quiescence search — three layers: captures+checks, evasions, and pure captures.

use crate::board::moves::*;
use crate::board::position::{Position, Undo};
use crate::board::types::*;
use crate::eval;
use crate::search::ordering::*;
use crate::tt;

/// Quiescence layer 0 — resolves check evasions with captures, checks, and killers.
pub fn quiesce_checks(
    ctx: &mut SearchCtx,
    pos: &mut Position,
    ply: usize,
    mut alpha: i32,
    beta: i32,
    pv: &mut [Move],
) -> i32 {
    if pos.in_check() {
        return quiesce_flee(ctx, pos, ply, alpha, beta, pv);
    }

    // EARLY EXIT
    ctx.searcher.nodes += 1;
    ctx.searcher.check_timeout();
    if ctx.searcher.abort_search && ctx.searcher.root_depth > 1 {
        return 0;
    }
    pv[0] = Move::NONE;
    if pos.is_draw() && ply > 0 {
        return pos.draw_score(ctx.par.draw_score, ctx.par.prog_side);
    }

    let mut mv = Move::NONE;
    let is_pv = alpha != beta - 1;

    // STAND PAT
    let mut best = eval::evaluate(
        pos,
        ctx.par,
        ctx.eval_hash,
        ctx.pawn_tt,
        ctx.searcher.game_key,
    );
    if best >= beta {
        return best;
    }
    if best > alpha {
        alpha = best;
    }

    // TT PROBE
    if let Some(hit) = ctx.tt.retrieve(pos.hash_key, alpha, beta, 0, ply as i32) {
        mv = hit.best_move;
        if hit.cutoff {
            if hit.score >= beta {
                ctx.searcher.update_history(pos, Move::SENTINEL, mv, 1, ply);
            }
            if !is_pv {
                return hit.score;
            }
        }
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

    // MAIN LOOP — special moves (captures, killers, checks)
    let mut new_pv: [Move; MAX_PLY] = unsafe { std::mem::zeroed() };
    let mut picker =
        SpecialPicker::new(mv, ctx.searcher.killer[ply][0], ctx.searcher.killer[ply][1]);

    loop {
        let (mv, _flag) = picker.next_move(pos, &ctx.searcher.history);
        if mv.is_none() {
            break;
        }

        let mut u = Undo::new();
        pos.do_move(mv, &mut u);
        if pos.illegal() {
            pos.undo_move(mv, &u);
            continue;
        }

        let score = -quiesce(ctx, pos, ply + 1, -beta, -alpha, &mut new_pv);

        pos.undo_move(mv, &u);
        if ctx.searcher.abort_search && ctx.searcher.root_depth > 1 {
            return 0;
        }

        if score >= beta {
            ctx.tt
                .store(pos.hash_key, mv, score, tt::LOWER, 0, ply as i32);
            return score;
        }

        if score > best {
            best = score;
            if score > alpha {
                alpha = score;
                build_pv(pv, &new_pv, mv);
            }
        }
    }

    // If no special move improved the score, the stand-pat evaluation stands.
    // (The in_check branch is unreachable here because we route to quiesce_flee at the top.)

    if pv[0].is_some() {
        ctx.tt
            .store(pos.hash_key, pv[0], best, tt::EXACT, 0, ply as i32);
    } else {
        ctx.tt
            .store(pos.hash_key, Move::NONE, best, tt::UPPER, 0, ply as i32);
    }

    best
}

/// Quiescence layer 1 — evasion search when in check (tries all legal moves).
pub fn quiesce_flee(
    ctx: &mut SearchCtx,
    pos: &mut Position,
    ply: usize,
    mut alpha: i32,
    beta: i32,
    pv: &mut [Move],
) -> i32 {
    ctx.searcher.nodes += 1;
    ctx.searcher.check_timeout();
    if ctx.searcher.abort_search && ctx.searcher.root_depth > 1 {
        return 0;
    }
    pv[0] = Move::NONE;
    if pos.is_draw() && ply > 0 {
        return pos.draw_score(ctx.par.draw_score, ctx.par.prog_side);
    }

    let mut mv = Move::NONE;
    let is_pv = alpha != beta - 1;

    // TT PROBE
    if let Some(hit) = ctx.tt.retrieve(pos.hash_key, alpha, beta, 0, ply as i32) {
        mv = hit.best_move;
        if hit.cutoff {
            if hit.score >= beta {
                ctx.searcher.update_history(pos, Move::SENTINEL, mv, 1, ply);
            }
            if !is_pv {
                return hit.score;
            }
        }
    }

    if ply >= MAX_PLY - 1 {
        return eval::evaluate(
            pos,
            ctx.par,
            ctx.eval_hash,
            ctx.pawn_tt,
            ctx.searcher.game_key,
        );
    }

    let mut best = -INF;
    let mut new_pv: [Move; MAX_PLY] = unsafe { std::mem::zeroed() };
    let mut picker = MovePicker::new(
        mv,
        Move::NONE,
        -1,
        ctx.searcher.killer[ply][0],
        ctx.searcher.killer[ply][1],
    );

    loop {
        let (mv, _flag) = picker.next_move(pos, &ctx.searcher.history);
        if mv.is_none() {
            break;
        }

        let mut u = Undo::new();
        pos.do_move(mv, &mut u);
        if pos.illegal() {
            pos.undo_move(mv, &u);
            continue;
        }

        let in_check_after = pos.in_check();
        let score = if in_check_after {
            -quiesce_flee(ctx, pos, ply + 1, -beta, -alpha, &mut new_pv)
        } else {
            -quiesce(ctx, pos, ply + 1, -beta, -alpha, &mut new_pv)
        };

        pos.undo_move(mv, &u);
        if ctx.searcher.abort_search && ctx.searcher.root_depth > 1 {
            return 0;
        }

        if score >= beta {
            ctx.tt
                .store(pos.hash_key, mv, score, tt::LOWER, 0, ply as i32);
            return score;
        }

        if score > best {
            best = score;
            if score > alpha {
                alpha = score;
                build_pv(pv, &new_pv, mv);
            }
        }
    }

    if best == -INF {
        return -MATE + ply as i32;
    }

    if pv[0].is_some() {
        ctx.tt
            .store(pos.hash_key, pv[0], best, tt::EXACT, 0, ply as i32);
    } else {
        ctx.tt
            .store(pos.hash_key, Move::NONE, best, tt::UPPER, 0, ply as i32);
    }

    best
}

/// Quiescence layer 2 — standard capture-only resolution with delta pruning.
pub fn quiesce(
    ctx: &mut SearchCtx,
    pos: &mut Position,
    ply: usize,
    mut alpha: i32,
    beta: i32,
    pv: &mut [Move],
) -> i32 {
    // Evasion when in check
    if pos.in_check() {
        return quiesce_flee(ctx, pos, ply, alpha, beta, pv);
    }

    ctx.searcher.nodes += 1;
    ctx.searcher.check_timeout();

    if ctx.searcher.abort_search && ctx.searcher.root_depth > 1 {
        return 0;
    }
    pv[0] = Move::NONE;
    if pos.is_draw() {
        return pos.draw_score(ctx.par.draw_score, ctx.par.prog_side);
    }

    if ply >= MAX_PLY - 1 {
        return eval::evaluate(
            pos,
            ctx.par,
            ctx.eval_hash,
            ctx.pawn_tt,
            ctx.searcher.game_key,
        );
    }

    // STAND PAT
    let mut best = eval::evaluate(
        pos,
        ctx.par,
        ctx.eval_hash,
        ctx.pawn_tt,
        ctx.searcher.game_key,
    );
    let floor = best;
    let alpha_floor = alpha;
    if best >= beta {
        return best;
    }
    if best > alpha {
        alpha = best;
    }

    // CAPTURE LOOP
    let op = !pos.side;
    let mut picker = CapturesPicker::new(pos);
    let mut new_pv: [Move; MAX_PLY] = unsafe { std::mem::zeroed() };

    loop {
        let mv = picker.next();
        if mv.is_none() {
            break;
        }

        // DELTA PRUNING
        let op_pieces = pos.count(op, N) + pos.count(op, B) + pos.count(op, R) + pos.count(op, Q);

        if op_pieces > 1 {
            // Prune if captured piece + margin < alpha
            if floor + TP_VALUE[pos.tp_on_sq(mv.to_sq()).index()] + 150 < alpha_floor {
                continue;
            }
            // Prune likely losing captures
            if bad_capture(pos, mv) {
                continue;
            }
        }

        let mut u = Undo::new();
        pos.do_move(mv, &mut u);
        if pos.illegal() {
            pos.undo_move(mv, &u);
            continue;
        }

        let score = -quiesce(ctx, pos, ply + 1, -beta, -alpha, &mut new_pv);

        pos.undo_move(mv, &u);
        if ctx.searcher.abort_search && ctx.searcher.root_depth > 1 {
            return 0;
        }

        if score >= beta {
            return score;
        }

        if score > best {
            best = score;
            if score > alpha {
                alpha = score;
                build_pv(pv, &new_pv, mv);
            }
        }
    }

    best
}

// ============================================================================
// PV helper
// ============================================================================

pub fn build_pv(pv: &mut [Move], new_pv: &[Move], mv: Move) {
    pv[0] = mv;
    let limit = new_pv.len().min(MAX_PLY - 1);
    let mut len = 0;
    while len < limit {
        if new_pv[len].is_none() {
            break;
        }
        len += 1;
    }
    // Single memcpy instead of per-element copy
    pv[1..1 + len].copy_from_slice(&new_pv[..len]);
    if 1 + len < pv.len() {
        pv[1 + len] = Move::NONE;
    }
}

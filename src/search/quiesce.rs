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

// Quiescence search.
// Three variants: Quiesce (captures), QuiesceChecks (+ checks), QuiesceFlee (evasion).

use crate::board::moves::*;
use crate::board::position::{Position, Undo};
use crate::board::types::*;
use crate::eval;
use crate::search::ordering::*;
use crate::tt::{self, TransTable};

/// QuiesceChecks — considers captures + checks + killers (called from Search at depth=0)
pub fn quiesce_checks(
    pos: &mut Position,
    searcher: &mut Searcher,
    tt: &mut TransTable,
    par: &eval::params::EvalParams,
    eval_hash: &mut Vec<eval::EvalHashEntry>,
    pawn_tt: &mut eval::pawn_hash::PawnHash,
    ply: usize,
    mut alpha: i32,
    beta: i32,
    pv: &mut [Move],
) -> i32 {
    if pos.in_check() {
        return quiesce_flee(
            pos, searcher, tt, par, eval_hash, pawn_tt, ply, alpha, beta, pv,
        );
    }

    // EARLY EXIT
    searcher.nodes += 1;
    searcher.check_timeout();
    if searcher.abort_search && searcher.root_depth > 1 {
        return 0;
    }
    pv[0] = Move::NONE;
    if pos.is_draw() && ply > 0 {
        return pos.draw_score(par.draw_score, par.prog_side);
    }

    let mut mv = Move::NONE;
    let is_pv = alpha != beta - 1;

    // STAND PAT
    let mut best = eval::evaluate(pos, par, eval_hash, pawn_tt, searcher.game_key);
    if best >= beta {
        return best;
    }
    if best > alpha {
        alpha = best;
    }

    // TT PROBE
    let mut tt_score = 0i32;
    let mut tt_flag = 0u8;
    if tt.retrieve(
        pos.hash_key,
        &mut mv,
        &mut tt_score,
        &mut tt_flag,
        alpha,
        beta,
        0,
        ply as i32,
    ) {
        if tt_score >= beta {
            searcher.update_history(pos, Move(u16::MAX), mv, 1, ply); // -1 sentinel → use MAX
        }
        if !is_pv {
            return tt_score;
        }
    }

    // MAX PLY GUARD
    if ply >= MAX_PLY - 1 {
        return eval::evaluate(pos, par, eval_hash, pawn_tt, searcher.game_key);
    }

    // MAIN LOOP — special moves (captures, killers, checks)
    let mut new_pv = [Move::NONE; MAX_PLY];
    let mut picker = SpecialPicker::new(mv, searcher.killer[ply][0], searcher.killer[ply][1]);

    loop {
        let (mv, _flag) = picker.next_move(pos, &searcher.history);
        if mv.is_none() {
            break;
        }

        let mut u = Undo::uninit();
        pos.do_move(mv, &mut u);
        if pos.illegal() {
            pos.undo_move(mv, &u);
            continue;
        }

        let score = -quiesce(
            pos,
            searcher,
            tt,
            par,
            eval_hash,
            pawn_tt,
            ply + 1,
            -beta,
            -alpha,
            &mut new_pv,
        );

        pos.undo_move(mv, &u);
        if searcher.abort_search && searcher.root_depth > 1 {
            return 0;
        }

        if score >= beta {
            tt.store(pos.hash_key, mv, score, tt::LOWER, 0, ply as i32);
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
        return if pos.in_check() {
            -MATE + ply as i32
        } else {
            0
        };
    }

    if !pv[0].is_none() {
        tt.store(pos.hash_key, pv[0], best, tt::EXACT, 0, ply as i32);
    } else {
        tt.store(pos.hash_key, Move::NONE, best, tt::UPPER, 0, ply as i32);
    }

    best
}

/// QuiesceFlee — evasion search when in check (tries all moves)
pub fn quiesce_flee(
    pos: &mut Position,
    searcher: &mut Searcher,
    tt: &mut TransTable,
    par: &eval::params::EvalParams,
    eval_hash: &mut Vec<eval::EvalHashEntry>,
    pawn_tt: &mut eval::pawn_hash::PawnHash,
    ply: usize,
    mut alpha: i32,
    beta: i32,
    pv: &mut [Move],
) -> i32 {
    searcher.nodes += 1;
    searcher.check_timeout();
    if searcher.abort_search && searcher.root_depth > 1 {
        return 0;
    }
    pv[0] = Move::NONE;
    if pos.is_draw() && ply > 0 {
        return pos.draw_score(par.draw_score, par.prog_side);
    }

    let mut mv = Move::NONE;
    let is_pv = alpha != beta - 1;

    // TT PROBE
    let mut tt_score = 0i32;
    let mut tt_flag = 0u8;
    if tt.retrieve(
        pos.hash_key,
        &mut mv,
        &mut tt_score,
        &mut tt_flag,
        alpha,
        beta,
        0,
        ply as i32,
    ) {
        if tt_score >= beta {
            searcher.update_history(pos, Move(u16::MAX), mv, 1, ply);
        }
        if !is_pv {
            return tt_score;
        }
    }

    if ply >= MAX_PLY - 1 {
        return eval::evaluate(pos, par, eval_hash, pawn_tt, searcher.game_key);
    }

    let mut best = -INF;
    let mut new_pv = [Move::NONE; MAX_PLY];
    let mut picker = MovePicker::new(
        mv,
        Move::NONE,
        -1,
        searcher.killer[ply][0],
        searcher.killer[ply][1],
    );

    loop {
        let (mv, _flag) = picker.next_move(pos, &searcher.history);
        if mv.is_none() {
            break;
        }

        let mut u = Undo::uninit();
        pos.do_move(mv, &mut u);
        if pos.illegal() {
            pos.undo_move(mv, &u);
            continue;
        }

        let in_check_after = pos.in_check();
        let score = if in_check_after {
            -quiesce_flee(
                pos,
                searcher,
                tt,
                par,
                eval_hash,
                pawn_tt,
                ply + 1,
                -beta,
                -alpha,
                &mut new_pv,
            )
        } else {
            -quiesce(
                pos,
                searcher,
                tt,
                par,
                eval_hash,
                pawn_tt,
                ply + 1,
                -beta,
                -alpha,
                &mut new_pv,
            )
        };

        pos.undo_move(mv, &u);
        if searcher.abort_search && searcher.root_depth > 1 {
            return 0;
        }

        if score >= beta {
            tt.store(pos.hash_key, mv, score, tt::LOWER, 0, ply as i32);
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

    if !pv[0].is_none() {
        tt.store(pos.hash_key, pv[0], best, tt::EXACT, 0, ply as i32);
    } else {
        tt.store(pos.hash_key, Move::NONE, best, tt::UPPER, 0, ply as i32);
    }

    best
}

/// Quiesce — standard quiescence (captures only, no checks)
pub fn quiesce(
    pos: &mut Position,
    searcher: &mut Searcher,
    tt: &mut TransTable,
    par: &eval::params::EvalParams,
    eval_hash: &mut Vec<eval::EvalHashEntry>,
    pawn_tt: &mut eval::pawn_hash::PawnHash,
    ply: usize,
    mut alpha: i32,
    beta: i32,
    pv: &mut [Move],
) -> i32 {
    // Evasion when in check
    if pos.in_check() {
        return quiesce_flee(
            pos, searcher, tt, par, eval_hash, pawn_tt, ply, alpha, beta, pv,
        );
    }

    searcher.nodes += 1;
    searcher.check_timeout();

    if searcher.abort_search && searcher.root_depth > 1 {
        return 0;
    }
    pv[0] = Move::NONE;
    if pos.is_draw() {
        return pos.draw_score(par.draw_score, par.prog_side);
    }

    if ply >= MAX_PLY - 1 {
        return eval::evaluate(pos, par, eval_hash, pawn_tt, searcher.game_key);
    }

    // STAND PAT
    let mut best = eval::evaluate(pos, par, eval_hash, pawn_tt, searcher.game_key);
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
    let mut new_pv = [Move::NONE; MAX_PLY];

    loop {
        let mv = picker.next();
        if mv.is_none() {
            break;
        }

        // DELTA PRUNING
        let op_pieces = pos.cnt[op.index()][N.index()]
            + pos.cnt[op.index()][B.index()]
            + pos.cnt[op.index()][R.index()]
            + pos.cnt[op.index()][Q.index()];

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

        let mut u = Undo::uninit();
        pos.do_move(mv, &mut u);
        if pos.illegal() {
            pos.undo_move(mv, &u);
            continue;
        }

        let score = -quiesce(
            pos,
            searcher,
            tt,
            par,
            eval_hash,
            pawn_tt,
            ply + 1,
            -beta,
            -alpha,
            &mut new_pv,
        );

        pos.undo_move(mv, &u);
        if searcher.abort_search && searcher.root_depth > 1 {
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
    // SAFETY: scanning for sentinel in bounded array; index always < MAX_PLY
    let mut len = 0;
    while len < MAX_PLY - 1 {
        if unsafe { new_pv.get_unchecked(len) }.is_none() {
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

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

//! Passed pawn evaluation — bonuses for passed pawns and unstoppable pawn detection.

use super::eval_data::EvalData;
use super::params::EvalParams;
use crate::board::bitboard::*;
use crate::board::distance;
use crate::board::masks;
use crate::board::position::Position;
use crate::board::types::*;

/// Evaluate passed pawns — bonuses for advancement, king proximity, and free path.
pub fn evaluate_passers(p: &Position, e: &mut EvalData, par: &EvalParams, sd: Color) {
    let op = !sd;
    let si = sd.index();
    let oi = op.index();
    let mut mg_tot = 0;
    let mut eg_tot = 0;

    let mut bb_pieces = p.pawns(sd);
    while bb_pieces.is_not_empty() {
        let sq = bb_pieces.pop_lsb();
        let bb_pawn = Bitboard::from_sq(sq);
        let bb_stop = shift_fwd(bb_pawn, sd);

        // Pawn threatening enemy minor
        if (bb_stop & p.occ_bb()).is_empty() && (bb_stop & e.p_can_take[oi]).is_empty() {
            if (pawn_attacks_bb(bb_stop, sd) & (p.bishops(op) | p.knights(op))).is_not_empty() {
                e.add_both(sd, par.p_thr);
            }
            // Double pawn push threat
            if (bb_pawn & (RANK_2_BB | RANK_7_BB)).is_not_empty() {
                let next = shift_fwd(bb_stop, sd);
                if (next & p.occ_bb()).is_empty()
                    && (next & e.p_can_take[oi]).is_empty()
                    && (pawn_attacks_bb(next, sd) & (p.bishops(op) | p.knights(op))).is_not_empty()
                {
                    e.add_both(sd, par.p_thr);
                }
            }
        }

        // Passed pawns
        if (masks::passed(sd, sq) & p.pawns(op)).is_empty() {
            let mut mul = 100;
            if (bb_pawn & e.p_takes[si]).is_not_empty() {
                mul += par.p_defmul;
            }
            if (bb_stop & e.p_takes[si]).is_not_empty() {
                mul += par.p_stopmul;
            }

            if (bb_stop & p.occ_bb()).is_not_empty() {
                mul -= par.p_bl_mul;
            } else if (bb_stop & e.all_att[si]).is_not_empty()
                && (bb_stop & !e.all_att[oi]).is_not_empty()
            {
                mul += par.p_ourstop_mul;
            } else if (bb_stop & e.all_att[oi]).is_not_empty()
                && (bb_stop & !e.all_att[si]).is_not_empty()
            {
                mul -= par.p_oppstop_mul;
            }

            let r = rank_of(sq) as usize;
            let mg_tmp = par.passed_bonus_mg[si][r];
            let eg_tmp = par.passed_bonus_eg[si][r]
                - ((par.passed_bonus_eg[si][r] * distance::bonus(sq, p.king_sq[oi])) / 30)
                + ((par.passed_bonus_eg[si][r] * distance::bonus(sq, p.king_sq[si])) / 90);

            mg_tot += (mg_tmp * mul) / 100;
            eg_tot += (eg_tmp * mul) / 100;
        }
    }

    e.add(
        sd,
        (mg_tot * par.w_passers) / 100,
        (eg_tot * par.w_passers) / 100,
    );
}

/// Evaluate unstoppable passed pawns — determines if a passer reaches promotion first.
pub fn evaluate_unstoppable(e: &mut EvalData, p: &Position) {
    let w_dist = unstoppable_distance(p, WC);
    let b_dist = unstoppable_distance(p, BC);

    if w_dist < b_dist - 1 {
        e.add(WC, 0, 500);
    }
    if b_dist < w_dist - 1 {
        e.add(BC, 0, 500);
    }
}

/// Compute the minimum promotion distance of a side's unstoppable passed pawns.
/// Returns 8 if no unstoppable passer exists.
fn unstoppable_distance(p: &Position, sd: Color) -> i32 {
    let op = !sd;
    // Opponent must have no pieces (only king + pawns)
    if p.count(op, N) + p.count(op, B) + p.count(op, R) + p.count(op, Q) != 0 {
        return 8;
    }

    let king_sq = p.king_sq(op);
    let tempo = i32::from(p.side == op);
    let mut best = 8;
    let mut bb = p.pawns(sd);

    while bb.is_not_empty() {
        let sq = bb.pop_lsb();
        if (masks::passed(sd, sq) & p.pawns(op)).is_empty() {
            let bb_span = get_front_span(Bitboard::from_sq(sq), sd);
            // Promotion square: same file, on the opponent's back rank
            let prom_sq = if sd == WC { 56 + (sq & 7) } else { sq & 7 };
            let prom_dist = 5.min(distance::metric(sq, prom_sq));
            if prom_dist < (distance::metric(king_sq, prom_sq) - tempo) {
                let d = if (bb_span & p.kings(sd)).is_not_empty() {
                    prom_dist + 1
                } else {
                    prom_dist
                };
                best = best.min(d);
            }
        }
    }
    best
}

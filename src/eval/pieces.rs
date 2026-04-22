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

//! Piece evaluation — placement, mobility, outposts, and king attack contribution.

use super::eval_data::EvalData;
use super::params::EvalParams;
use super::pst;
use crate::board::attacks;
use crate::board::bitboard::*;
use crate::board::distance;
use crate::board::masks;
use crate::board::position::Position;
use crate::board::types::*;
use crate::movegen::see;

/// Outpost map — ranks 4-6 for white, ranks 3-5 for black, excluding files A and H.
/// Inner files mask: `(ranks) & bbNotA & bbNotH`.
pub const OUTPOST_MAP: [Bitboard; 2] = [
    Bitboard((RANK_4_BB.0 | RANK_5_BB.0 | RANK_6_BB.0) & NOT_A_FILE.0 & NOT_H_FILE.0),
    Bitboard((RANK_3_BB.0 | RANK_4_BB.0 | RANK_5_BB.0) & NOT_A_FILE.0 & NOT_H_FILE.0),
];

fn evaluate_outpost(
    p: &Position,
    e: &mut EvalData,
    par: &EvalParams,
    sd: Color,
    pc: PieceType,
    sq: i32,
    outpost: &mut i32,
) {
    let op = !sd;

    // Minor piece shielded by own pawn
    if (Bitboard::from_sq(sq) & masks::HOME[sd.index()]).is_not_empty() {
        let stop = shift_fwd(Bitboard::from_sq(sq), sd);
        if (stop & p.pawns(sd)).is_not_empty() {
            *outpost += par.bn_shield;
        }
    }

    // Base outpost bonus from special PST
    let tmp = par.sp_pst[sd.index()][pc.index()][sq as usize];
    if tmp != 0 {
        let mut mul = 0;
        if (Bitboard::from_sq(sq) & !e.p_can_take[op.index()]).is_not_empty() {
            mul += 2;
        }
        if (Bitboard::from_sq(sq) & e.p_takes[sd.index()]).is_not_empty() {
            mul += 1;
        }
        if (Bitboard::from_sq(sq) & e.two_pawns_take[sd.index()]).is_not_empty() {
            mul += 1;
        }
        *outpost += (tmp * mul) / 2;
    }
}

/// Evaluate piece placement, mobility, tropism, outposts, and line control for one side.
pub fn evaluate_pieces(p: &Position, e: &mut EvalData, par: &EvalParams, sd: Color) {
    let op = !sd;
    let si = sd.index();
    let oi = op.index();

    let mut r_on_7th = 0i32;
    let mut mob_mg = 0i32;
    let mut mob_eg = 0i32;
    let mut tropism_mg = 0i32;
    let mut tropism_eg = 0i32;
    let mut lines_mg = 0i32;
    let mut lines_eg = 0i32;
    let mut fwd_weight = 0i32;
    let mut fwd_cnt = 0usize;
    let mut outpost = 0i32;
    let mut center_control = 2 * (e.p_takes[si] & masks::CENTER).popcount();

    // King attack zone (shared with the sacrifice classifier in search::ordering).
    let king_sq = p.king_sq(op);
    let bb_zone = attacks::king_attack_zone(king_sq, op);

    // Check threat bitboards
    let occ = p.occ_bb();
    let n_checks = attacks::knight_attacks(king_sq) & !p.cl_bb[si] & !e.p_takes[oi];
    let b_checks = attacks::bishop_attacks(occ, king_sq) & !p.cl_bb[si] & !e.p_takes[oi];
    let r_checks = attacks::rook_attacks(occ, king_sq) & !p.cl_bb[si] & !e.p_takes[oi];
    let q_checks = r_checks & b_checks;
    let bb_excluded = p.pawns(sd);

    // === Knight eval ===
    let mut bb_pieces = p.knights(sd);
    while bb_pieces.is_not_empty() {
        let sq = bb_pieces.pop_lsb();

        tropism_mg += par.ntr_mg * distance::bonus(sq, king_sq);
        tropism_eg += par.ntr_eg * distance::bonus(sq, king_sq);

        if (Bitboard::from_sq(sq) & masks::AWAY[si]).is_not_empty() {
            fwd_weight += par.n_fwd;
            fwd_cnt += 1;
        }

        let bb_control = attacks::knight_attacks(sq) & !p.cl_bb[si];
        center_control += (bb_control & masks::CENTER).popcount();
        if (bb_control & !e.p_takes[oi] & masks::AWAY[si]).is_empty() {
            e.add_both(sd, par.n_owh);
        }
        e.all_att[si] |= attacks::knight_attacks(sq);
        e.ev_att[si] |= bb_control;
        if (bb_control & n_checks).is_not_empty() {
            e.att[si] += par.n_chk;
        }

        // Reachable outposts
        let bb_possible = bb_control & !e.p_takes[oi] & !e.p_can_take[oi] & OUTPOST_MAP[si];
        if bb_possible.is_not_empty() {
            e.add(sd, par.n_reach, 2);
        }

        // King attack contribution
        let bb_attack = attacks::knight_attacks(sq);
        if (bb_attack & bb_zone).is_not_empty() {
            e.wood[si] += 1;
            e.att[si] += par.n_att1 * (bb_attack & (bb_zone & !e.p_takes[oi])).popcount();
            e.att[si] += par.n_att2 * (bb_attack & (bb_zone & e.p_takes[oi])).popcount();
        }

        let cnt = (bb_control & !e.p_takes[oi]).popcount() as usize;
        mob_mg += par.n_mob_mg[cnt.min(8)];
        mob_eg += par.n_mob_eg[cnt.min(8)];

        evaluate_outpost(p, e, par, sd, N, sq, &mut outpost);
    }

    // === Bishop eval ===
    bb_pieces = p.bishops(sd);
    while bb_pieces.is_not_empty() {
        let sq = bb_pieces.pop_lsb();

        tropism_mg += par.btr_mg * distance::bonus(sq, king_sq);
        tropism_eg += par.btr_eg * distance::bonus(sq, king_sq);

        if (Bitboard::from_sq(sq) & masks::AWAY[si]).is_not_empty() {
            fwd_weight += par.b_fwd;
            fwd_cnt += 1;
        }

        let bb_control = attacks::bishop_attacks(occ, sq);
        center_control += (bb_control & masks::CENTER).popcount();
        e.all_att[si] |= bb_control;
        e.ev_att[si] |= bb_control;
        if (bb_control & masks::AWAY[si]).is_empty() {
            e.add_both(sd, par.b_owh);
        }
        if (bb_control & b_checks).is_not_empty() {
            e.att[si] += par.b_chk;
        }

        // X-ray through own queen for king attack
        let bb_attack = attacks::bishop_attacks(occ ^ p.queens(sd), sq);
        if (bb_attack & bb_zone).is_not_empty() {
            e.wood[si] += 1;
            e.att[si] += par.b_att1 * (bb_attack & (bb_zone & !e.p_takes[oi])).popcount();
            e.att[si] += par.b_att2 * (bb_attack & (bb_zone & e.p_takes[oi])).popcount();
        }

        let cnt = (bb_control & !e.p_takes[oi] & !bb_excluded).popcount() as usize;
        mob_mg += par.b_mob_mg[cnt.min(13)];
        mob_eg += par.b_mob_eg[cnt.min(13)];

        let bb_possible = bb_control & !e.p_takes[oi] & !e.p_can_take[oi] & OUTPOST_MAP[si];
        if bb_possible.is_not_empty() {
            e.add(sd, par.b_reach, 2);
        }

        evaluate_outpost(p, e, par, sd, B, sq, &mut outpost);

        // Bishops side by side
        if (shift_north(Bitboard::from_sq(sq)) & p.bishops(sd)).is_not_empty() {
            e.add_both(sd, par.b_touch);
        }
        if (shift_east(Bitboard::from_sq(sq)) & p.bishops(sd)).is_not_empty() {
            e.add_both(sd, par.b_touch);
        }

        // Pawns on same color as bishop
        let (own_p_cnt, opp_p_cnt) = if (WHITE_SQUARES & Bitboard::from_sq(sq)).is_not_empty() {
            (
                (WHITE_SQUARES & p.pawns(sd)).popcount() - 4,
                (WHITE_SQUARES & p.pawns(op)).popcount() - 4,
            )
        } else {
            (
                (BLACK_SQUARES & p.pawns(sd)).popcount() - 4,
                (BLACK_SQUARES & p.pawns(op)).popcount() - 4,
            )
        };
        e.add_both(sd, par.b_own_p * own_p_cnt + par.b_opp_p * opp_p_cnt);
    }

    // === Rook eval ===
    bb_pieces = p.rooks(sd);
    while bb_pieces.is_not_empty() {
        let sq = bb_pieces.pop_lsb();

        tropism_mg += par.rtr_mg * distance::bonus(sq, king_sq);
        tropism_eg += par.rtr_eg * distance::bonus(sq, king_sq);

        if (Bitboard::from_sq(sq) & masks::AWAY[si]).is_not_empty() {
            fwd_weight += par.r_fwd;
            fwd_cnt += 1;
        }

        let bb_control = attacks::rook_attacks(occ, sq);
        e.all_att[si] |= bb_control;
        e.ev_att[si] |= bb_control;

        if (bb_control & !p.cl_bb[si] & r_checks).is_not_empty() && p.queens(sd).is_not_empty() {
            e.att[si] += par.r_chk;
            let mut bb_contact = (bb_control & attacks::king_attacks(king_sq)) & r_checks;
            while bb_contact.is_not_empty() {
                let csq = bb_contact.pop_lsb();
                if see::see(p, sq, csq) >= 0 {
                    e.att[si] += par.r_contact;
                    break;
                }
            }
        }

        // X-ray through own straight movers for king attack
        let bb_attack = attacks::rook_attacks(occ ^ p.straight_movers(sd), sq);
        if (bb_attack & bb_zone).is_not_empty() {
            e.wood[si] += 1;
            e.att[si] += par.r_att1 * (bb_attack & (bb_zone & !e.p_takes[oi])).popcount();
            e.att[si] += par.r_att2 * (bb_attack & (bb_zone & e.p_takes[oi])).popcount();
        }

        let cnt = (bb_control & !bb_excluded).popcount() as usize;
        mob_mg += par.r_mob_mg[cnt.min(14)];
        mob_eg += par.r_mob_eg[cnt.min(14)];

        // File evaluation
        let bb_file = fill_north(Bitboard::from_sq(sq)) | fill_south(Bitboard::from_sq(sq));

        if (bb_file & p.queens(op)).is_not_empty() {
            lines_mg += par.roq_mg;
            lines_eg += par.roq_eg;
        }

        if (bb_file & p.pawns(sd)).is_empty() {
            if (bb_file & p.pawns(op)).is_empty() {
                lines_mg += par.rof_mg;
                lines_eg += par.rof_eg;
            } else if (bb_file & (p.pawns(op) & e.p_takes[oi])).is_not_empty() {
                lines_mg += par.rbh_mg;
                lines_eg += par.rbh_eg;
            } else {
                lines_mg += par.rgh_mg;
                lines_eg += par.rgh_eg;
            }
        }

        // Rook on 7th rank
        if (Bitboard::from_sq(sq) & REL_RANK_BB[si][6]).is_not_empty()
            && ((p.pawns(op) & REL_RANK_BB[si][6]).is_not_empty()
                || (p.kings(op) & REL_RANK_BB[si][7]).is_not_empty())
        {
            lines_mg += par.rsr_mg;
            lines_eg += par.rsr_eg;
            r_on_7th += 1;
        }
    }

    // === Queen eval ===
    bb_pieces = p.queens(sd);
    while bb_pieces.is_not_empty() {
        let sq = bb_pieces.pop_lsb();

        tropism_mg += par.qtr_mg * distance::bonus(sq, king_sq);
        tropism_eg += par.qtr_eg * distance::bonus(sq, king_sq);

        if (Bitboard::from_sq(sq) & masks::AWAY[si]).is_not_empty() {
            fwd_weight += par.q_fwd;
            fwd_cnt += 1;
        }

        let bb_control = attacks::queen_attacks(occ, sq);
        e.all_att[si] |= bb_control;
        if (bb_control & q_checks).is_not_empty() {
            e.att[si] += par.q_chk;
            let mut bb_contact = bb_control & attacks::king_attacks(king_sq);
            while bb_contact.is_not_empty() {
                let csq = bb_contact.pop_lsb();
                if see::see(p, sq, csq) >= 0 {
                    e.att[si] += par.q_contact;
                    break;
                }
            }
        }

        let bb_attack = attacks::bishop_attacks(occ ^ p.diag_movers(sd), sq)
            | attacks::rook_attacks(occ ^ p.straight_movers(sd), sq);
        if (bb_attack & bb_zone).is_not_empty() {
            e.wood[si] += 1;
            e.att[si] += par.q_att1 * (bb_attack & (bb_zone & !e.p_takes[oi])).popcount();
            e.att[si] += par.q_att2 * (bb_attack & (bb_zone & e.p_takes[oi])).popcount();
        }

        let cnt = (bb_control & !bb_excluded).popcount().min(27) as usize;
        mob_mg += par.q_mob_mg[cnt];
        mob_eg += par.q_mob_eg[cnt];

        // Queen on 7th
        if (Bitboard::from_sq(sq) & REL_RANK_BB[si][6]).is_not_empty()
            && ((p.pawns(op) & REL_RANK_BB[si][6]).is_not_empty()
                || (p.kings(op) & REL_RANK_BB[si][7]).is_not_empty())
        {
            lines_mg += par.qsr_mg;
            lines_eg += par.qsr_eg;
        }
    }

    // Composite factors
    if r_on_7th > 1 {
        lines_mg += par.rs2_mg;
        lines_eg += par.rs2_eg;
    }

    // Weight and add
    e.add(
        sd,
        (par.sd_mob[si] * mob_mg) / 100,
        (par.sd_mob[si] * mob_eg) / 100,
    );
    e.add(
        sd,
        (par.w_tropism * tropism_mg) / 100,
        (par.w_tropism * tropism_eg) / 100,
    );
    e.add(
        sd,
        (par.w_lines * lines_mg) / 100,
        (par.w_lines * lines_eg) / 100,
    );
    let fc = fwd_cnt.min(15);
    e.add(sd, (par.w_fwd * pst::FWD_BONUS[fc] * fwd_weight) / 100, 0);
    e.add_both(sd, (par.w_outposts * outpost) / 100);
    e.add(sd, (par.w_center * center_control) / 100, 0);
}

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

// Pawn structure evaluation — doubled, isolated, backward, passed, and candidate pawns.

use super::eval_data::{self, EvalData};
use super::params::{self, EvalParams};
use super::pst;
use crate::board::bitboard::*;
use crate::board::masks;
use crate::board::position::Position;
use crate::board::types::*;

pub fn evaluate_pawn_struct(
    p: &Position,
    e: &mut EvalData,
    par: &EvalParams,
    pawn_tt: &mut super::pawn_hash::PawnHash,
) {
    // Try pawn hash first
    if let Some((mg, eg)) = pawn_tt.retrieve(p.pawn_key) {
        e.mg_pawns[0] = mg;
        e.eg_pawns[0] = eg;
        e.mg_pawns[1] = 0;
        e.eg_pawns[1] = 0;
        return;
    }

    // Clear pawn scores
    e.mg_pawns = [0; 2];
    e.eg_pawns = [0; 2];

    evaluate_pawns(p, e, par, WC);
    evaluate_pawns(p, e, par, BC);
    evaluate_king(p, e, par, WC);
    evaluate_king(p, e, par, BC);

    // Center binds
    let mut tmp = 0;
    if (e.two_pawns_take[0] & Bitboard::from_sq(D5)).is_not_empty() {
        tmp += par.p_bind;
    }
    if (e.two_pawns_take[0] & Bitboard::from_sq(E5)).is_not_empty() {
        tmp += par.p_bind;
    }
    if (e.two_pawns_take[0] & Bitboard::from_sq(D6)).is_not_empty() {
        tmp += par.p_bind;
    }
    if (e.two_pawns_take[0] & Bitboard::from_sq(E6)).is_not_empty() {
        tmp += par.p_bind;
    }
    if p.is_on_sq(WC, P, B3) && (e.two_pawns_take[0] & Bitboard::from_sq(B5)).is_not_empty() {
        tmp -= par.p_badbind;
    }
    if p.is_on_sq(WC, P, G3) && (e.two_pawns_take[0] & Bitboard::from_sq(G5)).is_not_empty() {
        tmp -= par.p_badbind;
    }
    eval_data::add(e, WC, tmp, 0);

    tmp = 0;
    if (e.two_pawns_take[1] & Bitboard::from_sq(D4)).is_not_empty() {
        tmp += par.p_bind;
    }
    if (e.two_pawns_take[1] & Bitboard::from_sq(E4)).is_not_empty() {
        tmp += par.p_bind;
    }
    if (e.two_pawns_take[1] & Bitboard::from_sq(D3)).is_not_empty() {
        tmp += par.p_bind;
    }
    if (e.two_pawns_take[1] & Bitboard::from_sq(E3)).is_not_empty() {
        tmp += par.p_bind;
    }
    if p.is_on_sq(BC, P, B6) && (e.two_pawns_take[1] & Bitboard::from_sq(B4)).is_not_empty() {
        tmp -= par.p_badbind;
    }
    if p.is_on_sq(BC, P, G6) && (e.two_pawns_take[1] & Bitboard::from_sq(G4)).is_not_empty() {
        tmp -= par.p_badbind;
    }
    eval_data::add(e, BC, tmp, 0);

    // King on empty wing
    let bb_all_pawns = p.pawns(WC) | p.pawns(BC);
    if bb_all_pawns.is_not_empty() {
        if (bb_all_pawns & masks::K_SIDE).is_empty() {
            eval_data::add_pawns(
                e,
                WC,
                pst::EMPTY_KS[p.king_sq[0] as usize],
                pst::EMPTY_KS[p.king_sq[0] as usize],
            );
            eval_data::add_pawns(
                e,
                BC,
                pst::EMPTY_KS[p.king_sq[1] as usize],
                pst::EMPTY_KS[p.king_sq[1] as usize],
            );
        }
        if (bb_all_pawns & masks::Q_SIDE).is_empty() {
            eval_data::add_pawns(
                e,
                WC,
                pst::EMPTY_QS[p.king_sq[0] as usize],
                pst::EMPTY_QS[p.king_sq[0] as usize],
            );
            eval_data::add_pawns(
                e,
                BC,
                pst::EMPTY_QS[p.king_sq[1] as usize],
                pst::EMPTY_QS[p.king_sq[1] as usize],
            );
        }
    }

    // Pawn islands
    let w_pawn_files = fill_south(p.pawns(WC)).0 & 0xff;
    let w_islands = (((!w_pawn_files) >> 1) & w_pawn_files).count_ones() as i32;
    let b_pawn_files = fill_south(p.pawns(BC)).0 & 0xff;
    let b_islands = (((!b_pawn_files) >> 1) & b_pawn_files).count_ones() as i32;
    e.mg_pawns[0] -= (w_islands - b_islands) * par.p_isl;
    e.eg_pawns[0] -= (w_islands - b_islands) * par.p_isl;

    // Apply weight and store as delta
    let mg = (par.w_struct * (e.mg_pawns[0] - e.mg_pawns[1])) / 100;
    let eg = (par.w_struct * (e.eg_pawns[0] - e.eg_pawns[1])) / 100;
    e.mg_pawns[0] = mg;
    e.eg_pawns[0] = eg;
    e.mg_pawns[1] = 0;
    e.eg_pawns[1] = 0;

    // Store in pawn hash
    pawn_tt.store(p.pawn_key, mg, eg);
}

fn evaluate_pawns(p: &Position, e: &mut EvalData, par: &EvalParams, sd: Color) {
    let op = !sd;
    let si = sd.index();
    let mut mass_mg = 0;
    let mut mass_eg = 0;

    let mut bb_pieces = p.pawns(sd);
    while bb_pieces.is_not_empty() {
        let sq = bb_pieces.pop_lsb();
        let front_span = get_front_span(Bitboard::from_sq(sq), sd);
        let fl_unopposed = i32::from((front_span & p.pawns(op)).is_empty());
        let fl_phalanx = (shift_sideways(Bitboard::from_sq(sq)) & p.pawns(sd)).is_not_empty();
        let fl_defended = (Bitboard::from_sq(sq) & e.p_takes[si]).is_not_empty();

        // Candidate passers
        if fl_unopposed != 0
            && (fl_phalanx || fl_defended)
            && (masks::passed(sd, sq) & p.pawns(op)).popcount() == 1
        {
            let r = rank_of(sq) as usize;
            eval_data::add_pawns(e, sd, par.cand_bonus_mg[si][r], par.cand_bonus_eg[si][r]);
        }

        // Doubled pawn
        if (front_span & p.pawns(sd)).is_not_empty() {
            eval_data::add_pawns(e, sd, par.db_mid, par.db_end);
        }

        // Supported pawn
        if fl_phalanx {
            mass_mg += par.sp_pst[si][params::PHA_MG][sq as usize];
            mass_eg += par.sp_pst[si][params::PHA_EG][sq as usize];
        } else if fl_defended {
            mass_mg += par.sp_pst[si][params::DEF_MG][sq as usize];
            mass_eg += par.sp_pst[si][params::DEF_EG][sq as usize];
        }

        // Isolated pawn
        if (masks::adjacent(file_of(sq)) & p.pawns(sd)).is_empty() {
            eval_data::add_pawns(e, sd, par.iso_mg + par.iso_of * fl_unopposed, par.iso_eg);
        }
        // Backward pawn
        else if (masks::supported(sd, sq) & p.pawns(sd)).is_empty() {
            eval_data::add_pawns(
                e,
                sd,
                par.backward_malus_mg[file_of(sq) as usize] + par.bk_ope * fl_unopposed,
                par.bk_end,
            );
        }
    }

    eval_data::add_pawns(
        e,
        sd,
        (mass_mg * par.w_mass) / 100,
        (mass_eg * par.w_mass) / 100,
    );
}

fn evaluate_king(p: &Position, e: &mut EvalData, par: &EvalParams, sd: Color) {
    let mut shield = 0;
    let mut storm = 0;
    let mut sq = p.king_sq(sd);

    // Normalize king square
    if (Bitboard::from_sq(sq) & masks::KS_CASTLE[sd.index()]).is_not_empty() {
        sq = if sd == WC { G1 } else { G8 };
    }
    if (Bitboard::from_sq(sq) & masks::QS_CASTLE[sd.index()]).is_not_empty() {
        sq = if sd == WC { B1 } else { B8 };
    }

    let bb_king_file = fill_north(Bitboard::from_sq(sq)) | fill_south(Bitboard::from_sq(sq));
    evaluate_king_file(p, sd, bb_king_file, &mut shield, &mut storm, par);

    let bb_east = shift_east(bb_king_file);
    if bb_east.is_not_empty() {
        evaluate_king_file(p, sd, bb_east, &mut shield, &mut storm, par);
    }
    let bb_west = shift_west(bb_king_file);
    if bb_west.is_not_empty() {
        evaluate_king_file(p, sd, bb_west, &mut shield, &mut storm, par);
    }

    eval_data::add_pawns(
        e,
        sd,
        (par.w_shield * shield) / 100 + (par.w_storm * storm) / 100,
        0,
    );
    eval_data::add_pawns(e, sd, evaluate_chains(p, par, sd), 0);
}

fn evaluate_king_file(
    p: &Position,
    sd: Color,
    bb_file: Bitboard,
    shield: &mut i32,
    storm: &mut i32,
    par: &EvalParams,
) {
    let mut shelter = evaluate_file_shelter(bb_file & p.pawns(sd), sd, par);
    if (p.kings(sd) & bb_file).is_not_empty() {
        shelter = (shelter * 120) / 100;
    }
    if (bb_file & CENTRAL_FILES).is_not_empty() {
        shelter /= 2;
    }
    *shield += shelter;
    *storm += evaluate_file_storm(bb_file & p.pawns(!sd), sd, par);
}

fn evaluate_file_shelter(bb: Bitboard, sd: Color, par: &EvalParams) -> i32 {
    let si = sd.index();
    if bb.is_empty() {
        return par.p_sh_none;
    }
    if (bb & REL_RANK_BB[si][1]).is_not_empty() {
        return par.p_sh_2;
    }
    if (bb & REL_RANK_BB[si][2]).is_not_empty() {
        return par.p_sh_3;
    }
    if (bb & REL_RANK_BB[si][3]).is_not_empty() {
        return par.p_sh_4;
    }
    if (bb & REL_RANK_BB[si][4]).is_not_empty() {
        return par.p_sh_5;
    }
    if (bb & REL_RANK_BB[si][5]).is_not_empty() {
        return par.p_sh_6;
    }
    if (bb & REL_RANK_BB[si][6]).is_not_empty() {
        return par.p_sh_7;
    }
    0
}

fn evaluate_file_storm(bb: Bitboard, sd: Color, par: &EvalParams) -> i32 {
    let si = sd.index();
    if bb.is_empty() {
        return par.p_st_open;
    }
    if (bb & REL_RANK_BB[si][2]).is_not_empty() {
        return par.p_st_3;
    }
    if (bb & REL_RANK_BB[si][3]).is_not_empty() {
        return par.p_st_4;
    }
    if (bb & REL_RANK_BB[si][4]).is_not_empty() {
        return par.p_st_5;
    }
    0
}

fn evaluate_chains(p: &Position, par: &EvalParams, sd: Color) -> i32 {
    let op = !sd;
    let sq = p.king_sq[sd.index()];
    let mut mg_result = 0;

    let rel = |s: i32| -> i32 { params::rel_sq(s as usize, sd) as i32 };
    let owp = |s: i32| -> bool { p.is_on_sq(sd, P, rel(s)) };
    let opp = |s: i32| -> bool { p.is_on_sq(op, P, rel(s)) };
    let con = |bb: Bitboard, a: i32, b: i32| -> bool {
        (bb & Bitboard::from_sq(rel(a))).is_not_empty()
            && (bb & Bitboard::from_sq(rel(b))).is_not_empty()
    };

    if (Bitboard::from_sq(sq) & masks::KS_CASTLE[sd.index()]).is_not_empty() {
        if opp(E4) {
            if con(p.pawns(op), D5, C6) {
                mg_result -= if owp(D4) && owp(E3) {
                    par.p_bigchain
                } else {
                    par.p_smallchain
                };
            }
            if con(p.pawns(op), D5, F3) {
                mg_result -= if owp(E3) {
                    par.p_bigchain
                } else {
                    par.p_smallchain
                };
            }
        }
        if opp(E5) {
            if con(p.pawns(op), F4, D6) {
                if opp(G5) {
                    mg_result -= par.p_cs1;
                    if opp(H4) {
                        return par.p_csfail;
                    }
                }
                if opp(G4) {
                    mg_result -= par.p_cs2;
                }
                mg_result -= if owp(E4) && owp(D5) {
                    par.p_bigchain
                } else {
                    par.p_smallchain
                };
            }
            if con(p.pawns(op), G3, F4) {
                mg_result -= if owp(F3) {
                    par.p_bigchain
                } else {
                    par.p_smallchain
                };
            }
        }
    }

    if (Bitboard::from_sq(sq) & masks::QS_CASTLE[sd.index()]).is_not_empty() {
        if opp(D4) {
            if con(p.pawns(op), E5, F6) {
                mg_result -= if owp(E4) && owp(D3) {
                    par.p_bigchain
                } else {
                    par.p_smallchain
                };
            }
            if con(p.pawns(op), F5, C3) {
                mg_result -= if owp(D3) {
                    par.p_bigchain
                } else {
                    par.p_smallchain
                };
            }
        }
        if opp(D5) {
            if con(p.pawns(op), C4, E6) {
                if opp(B5) {
                    mg_result -= par.p_cs1;
                    if opp(A4) {
                        return par.p_csfail;
                    }
                }
                if opp(B4) {
                    mg_result -= par.p_cs2;
                }
                mg_result -= if owp(E4) && owp(D5) {
                    par.p_bigchain
                } else {
                    par.p_smallchain
                };
            }
            if con(p.pawns(op), B3, C4) {
                mg_result -= if owp(C3) {
                    par.p_bigchain
                } else {
                    par.p_smallchain
                };
            }
        }
    }

    (mg_result * par.w_chains) / 100
}

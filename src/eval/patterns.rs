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

// Pattern evaluation — fianchetto, trapped pieces, blocked pawns, and special patterns.

use super::eval_data::{self, EvalData};
use super::params::EvalParams;
use crate::board::bitboard::*;
use crate::board::masks;
use crate::board::position::Position;
use crate::board::types::*;

// Special masks matching Mask.wb_special / Mask.bb_special
const WB_SPECIAL: Bitboard = Bitboard(
    (1u64 << A6)
        | (1u64 << A7)
        | (1u64 << B8)
        | (1u64 << H6)
        | (1u64 << H7)
        | (1u64 << G8)
        | (1u64 << C1)
        | (1u64 << F1)
        | (1u64 << B2)
        | (1u64 << G2),
);
const BB_SPECIAL: Bitboard = Bitboard(
    (1u64 << A3)
        | (1u64 << A2)
        | (1u64 << B1)
        | (1u64 << H3)
        | (1u64 << H2)
        | (1u64 << G1)
        | (1u64 << C8)
        | (1u64 << F8)
        | (1u64 << B7)
        | (1u64 << G7),
);

pub fn evaluate_bishop_patterns(p: &Position, e: &mut EvalData, par: &EvalParams) {
    if (p.bishops(WC) & WB_SPECIAL).is_not_empty() {
        // White bishop trapped
        if p.is_on_sq(WC, B, A6) && p.is_on_sq(BC, P, B5) {
            eval_data::add_both(e, WC, par.b_trap_a3);
        }
        if p.is_on_sq(WC, B, A7) && p.is_on_sq(BC, P, B6) {
            eval_data::add_both(e, WC, par.b_trap_a2);
        }
        if p.is_on_sq(WC, B, B8) && p.is_on_sq(BC, P, C7) {
            eval_data::add_both(e, WC, par.b_trap_a2);
        }
        if p.is_on_sq(WC, B, H6) && p.is_on_sq(BC, P, G5) {
            eval_data::add_both(e, WC, par.b_trap_a3);
        }
        if p.is_on_sq(WC, B, H7) && p.is_on_sq(BC, P, G6) {
            eval_data::add_both(e, WC, par.b_trap_a2);
        }
        if p.is_on_sq(WC, B, G8) && p.is_on_sq(BC, P, F7) {
            eval_data::add_both(e, WC, par.b_trap_a2);
        }
        // Blocked
        if p.is_on_sq(WC, B, C1) {
            if p.is_on_sq(WC, P, D2) && (Bitboard::from_sq(D3) & p.occ_bb()).is_not_empty() {
                eval_data::add(e, WC, par.b_block, 0);
            }
            if (p.kings(WC)
                & (Bitboard::from_sq(B1) | Bitboard::from_sq(A1) | Bitboard::from_sq(A2)))
            .is_not_empty()
            {
                eval_data::add(e, WC, par.b_return, 0);
            }
        }
        if p.is_on_sq(WC, B, F1) {
            if p.is_on_sq(WC, P, E2) && (Bitboard::from_sq(E3) & p.occ_bb()).is_not_empty() {
                eval_data::add(e, WC, par.b_block, 0);
            }
            if (p.kings(WC)
                & (Bitboard::from_sq(G1) | Bitboard::from_sq(H1) | Bitboard::from_sq(H2)))
            .is_not_empty()
            {
                eval_data::add(e, WC, par.b_return, 0);
            }
        }
        // Fianchetto
        if p.is_on_sq(WC, B, B2) {
            if p.is_on_sq(WC, P, C3) {
                eval_data::add(e, WC, par.b_bf_mg, par.b_bf_eg);
            }
            if p.is_on_sq(WC, P, B3) && (p.is_on_sq(WC, P, A2) || p.is_on_sq(WC, P, C2)) {
                eval_data::add_both(e, WC, par.b_fianch);
            }
            if p.is_on_sq(BC, P, D4) && (p.is_on_sq(BC, P, E5) || p.is_on_sq(BC, P, C5)) {
                eval_data::add_both(e, WC, par.b_badf);
            }
            if (p.kings(WC) & masks::QS_CASTLE[0]).is_not_empty() {
                eval_data::add(e, WC, par.b_king, 0);
            }
        }
        if p.is_on_sq(WC, B, G2) {
            if p.is_on_sq(WC, P, F3) {
                eval_data::add(e, WC, par.b_bf_mg, par.b_bf_eg);
            }
            if p.is_on_sq(WC, P, G3) && (p.is_on_sq(WC, P, H2) || p.is_on_sq(WC, P, F2)) {
                eval_data::add_both(e, WC, par.b_fianch);
            }
            if p.is_on_sq(BC, P, E4) && (p.is_on_sq(BC, P, D5) || p.is_on_sq(BC, P, F5)) {
                eval_data::add_both(e, WC, par.b_badf);
            }
            if (p.kings(WC) & masks::KS_CASTLE[0]).is_not_empty() {
                eval_data::add(e, WC, par.b_king, 0);
            }
        }
    }

    if (p.bishops(BC) & BB_SPECIAL).is_not_empty() {
        if p.is_on_sq(BC, B, A3) && p.is_on_sq(WC, P, B4) {
            eval_data::add_both(e, BC, par.b_trap_a3);
        }
        if p.is_on_sq(BC, B, A2) && p.is_on_sq(WC, P, B3) {
            eval_data::add_both(e, BC, par.b_trap_a2);
        }
        if p.is_on_sq(BC, B, B1) && p.is_on_sq(WC, P, C2) {
            eval_data::add_both(e, BC, par.b_trap_a2);
        }
        if p.is_on_sq(BC, B, H3) && p.is_on_sq(WC, P, G4) {
            eval_data::add_both(e, BC, par.b_trap_a3);
        }
        if p.is_on_sq(BC, B, H2) && p.is_on_sq(WC, P, G3) {
            eval_data::add_both(e, BC, par.b_trap_a2);
        }
        if p.is_on_sq(BC, B, G1) && p.is_on_sq(WC, P, F2) {
            eval_data::add_both(e, BC, par.b_trap_a2);
        }
        if p.is_on_sq(BC, B, C8) {
            if p.is_on_sq(BC, P, D7) && (Bitboard::from_sq(D6) & p.occ_bb()).is_not_empty() {
                eval_data::add(e, BC, par.b_block, 0);
            }
            if (p.kings(BC)
                & (Bitboard::from_sq(B8) | Bitboard::from_sq(A8) | Bitboard::from_sq(A7)))
            .is_not_empty()
            {
                eval_data::add(e, BC, par.b_return, 0);
            }
        }
        if p.is_on_sq(BC, B, F8) {
            if p.is_on_sq(BC, P, E7) && (Bitboard::from_sq(E6) & p.occ_bb()).is_not_empty() {
                eval_data::add(e, BC, par.b_block, 0);
            }
            if (p.kings(BC)
                & (Bitboard::from_sq(G8) | Bitboard::from_sq(H8) | Bitboard::from_sq(H7)))
            .is_not_empty()
            {
                eval_data::add(e, BC, par.b_return, 0);
            }
        }
        if p.is_on_sq(BC, B, B7) {
            if p.is_on_sq(BC, P, C6) {
                eval_data::add(e, BC, par.b_bf_mg, par.b_bf_eg);
            }
            if p.is_on_sq(BC, P, B6) && (p.is_on_sq(BC, P, A7) || p.is_on_sq(BC, P, C7)) {
                eval_data::add_both(e, BC, par.b_fianch);
            }
            if p.is_on_sq(WC, P, D5) && (p.is_on_sq(WC, P, E4) || p.is_on_sq(WC, P, C4)) {
                eval_data::add_both(e, BC, par.b_badf);
            }
            if (p.kings(BC) & masks::QS_CASTLE[1]).is_not_empty() {
                eval_data::add(e, BC, par.b_king, 0);
            }
        }
        if p.is_on_sq(BC, B, G7) {
            if p.is_on_sq(BC, P, F6) {
                eval_data::add(e, BC, par.b_bf_mg, par.b_bf_eg);
            }
            if p.is_on_sq(BC, P, G6) && (p.is_on_sq(BC, P, H7) || p.is_on_sq(BC, P, G6)) {
                eval_data::add_both(e, BC, par.b_fianch);
            }
            if p.is_on_sq(WC, P, E5) && (p.is_on_sq(WC, P, D4) || p.is_on_sq(WC, P, F4)) {
                eval_data::add_both(e, BC, par.b_badf);
            }
            if (p.kings(BC) & masks::KS_CASTLE[1]).is_not_empty() {
                eval_data::add(e, BC, par.b_king, 0);
            }
        }
    }
}

pub fn evaluate_knight_patterns(p: &Position, e: &mut EvalData, par: &EvalParams) {
    if p.is_on_sq(WC, N, A7) && p.is_on_sq(BC, P, A6) && p.is_on_sq(BC, P, B7) {
        eval_data::add_both(e, WC, par.n_trap);
    }
    if p.is_on_sq(WC, N, H7) && p.is_on_sq(BC, P, H6) && p.is_on_sq(BC, P, G7) {
        eval_data::add_both(e, WC, par.n_trap);
    }
    if p.is_on_sq(BC, N, A2) && p.is_on_sq(WC, P, A3) && p.is_on_sq(WC, P, B2) {
        eval_data::add_both(e, BC, par.n_trap);
    }
    if p.is_on_sq(BC, N, H2) && p.is_on_sq(WC, P, H3) && p.is_on_sq(WC, P, G2) {
        eval_data::add_both(e, BC, par.n_trap);
    }
}

pub fn evaluate_king_patterns(p: &Position, e: &mut EvalData, par: &EvalParams) {
    if (p.kings(WC) & RANK_1_BB).is_not_empty() {
        if p.is_on_sq(WC, K, H1) && p.is_on_sq(WC, P, H2) && p.is_on_sq(WC, P, G2) {
            eval_data::add_both(e, WC, par.k_no_luft);
        }
        if p.is_on_sq(WC, K, G1)
            && p.is_on_sq(WC, P, H2)
            && p.is_on_sq(WC, P, G2)
            && p.is_on_sq(WC, P, F2)
        {
            eval_data::add_both(e, WC, par.k_no_luft);
        }
        if p.is_on_sq(WC, K, A1) && p.is_on_sq(WC, P, A2) && p.is_on_sq(WC, P, B2) {
            eval_data::add_both(e, WC, par.k_no_luft);
        }
        if p.is_on_sq(WC, K, B1)
            && p.is_on_sq(WC, P, A2)
            && p.is_on_sq(WC, P, B2)
            && p.is_on_sq(WC, P, C2)
        {
            eval_data::add_both(e, WC, par.k_no_luft);
        }
        // Rook blocked
        let km1 = Bitboard::from_sq(F1) | Bitboard::from_sq(G1);
        let rm1 = Bitboard::from_sq(G1) | Bitboard::from_sq(H1) | Bitboard::from_sq(H2);
        if (p.kings(WC) & km1).is_not_empty() && (p.rooks(WC) & rm1).is_not_empty() {
            eval_data::add(e, WC, par.r_block_mg, par.r_block_eg);
        }
        let km2 = Bitboard::from_sq(B1) | Bitboard::from_sq(C1);
        let rm2 = Bitboard::from_sq(A1) | Bitboard::from_sq(B1) | Bitboard::from_sq(A2);
        if (p.kings(WC) & km2).is_not_empty() && (p.rooks(WC) & rm2).is_not_empty() {
            eval_data::add(e, WC, par.r_block_mg, par.r_block_eg);
        }
        // Castling rights
        if p.is_on_sq(WC, K, E1) {
            if p.castling & W_KS != 0 {
                eval_data::add(e, WC, par.k_castle, 0);
            } else if p.castling & W_QS != 0 {
                eval_data::add(e, WC, (par.k_castle * 2) / 3, 0);
            }
        }
    }
    if (p.kings(BC) & RANK_8_BB).is_not_empty() {
        if p.is_on_sq(BC, K, H8) && p.is_on_sq(BC, P, H7) && p.is_on_sq(BC, P, G7) {
            eval_data::add_both(e, BC, par.k_no_luft);
        }
        if p.is_on_sq(BC, K, G8)
            && p.is_on_sq(BC, P, H7)
            && p.is_on_sq(BC, P, G7)
            && p.is_on_sq(BC, P, F7)
        {
            eval_data::add_both(e, BC, par.k_no_luft);
        }
        if p.is_on_sq(BC, K, A8) && p.is_on_sq(BC, P, A7) && p.is_on_sq(BC, P, B7) {
            eval_data::add_both(e, BC, par.k_no_luft);
        }
        if p.is_on_sq(BC, K, B8)
            && p.is_on_sq(BC, P, A7)
            && p.is_on_sq(BC, P, B7)
            && p.is_on_sq(BC, P, C7)
        {
            eval_data::add_both(e, BC, par.k_no_luft);
        }
        let km1 = Bitboard::from_sq(F8) | Bitboard::from_sq(G8);
        let rm1 = Bitboard::from_sq(G8) | Bitboard::from_sq(H8) | Bitboard::from_sq(H7);
        if (p.kings(BC) & km1).is_not_empty() && (p.rooks(BC) & rm1).is_not_empty() {
            eval_data::add(e, BC, par.r_block_mg, par.r_block_eg);
        }
        let km2 = Bitboard::from_sq(B8) | Bitboard::from_sq(C8);
        let rm2 = Bitboard::from_sq(B8) | Bitboard::from_sq(A8) | Bitboard::from_sq(A7);
        if (p.kings(BC) & km2).is_not_empty() && (p.rooks(BC) & rm2).is_not_empty() {
            eval_data::add(e, BC, par.r_block_mg, par.r_block_eg);
        }
        if p.is_on_sq(BC, K, E8) {
            if p.castling & B_KS != 0 {
                eval_data::add(e, BC, par.k_castle, 0);
            } else if p.castling & B_QS != 0 {
                eval_data::add(e, BC, (par.k_castle * 2) / 3, 0);
            }
        }
    }
}

pub fn evaluate_central_patterns(p: &Position, e: &mut EvalData, par: &EvalParams) {
    if p.is_on_sq(WC, P, D4)
        && (p.bishops(WC)
            & (Bitboard::from_sq(H2)
                | Bitboard::from_sq(G3)
                | Bitboard::from_sq(F4)
                | Bitboard::from_sq(G5)
                | Bitboard::from_sq(H4)))
        .is_not_empty()
    {
        eval_data::add(e, WC, par.b_wing, 0);
    }
    if p.is_on_sq(WC, P, E4)
        && (p.bishops(WC)
            & (Bitboard::from_sq(A2)
                | Bitboard::from_sq(B3)
                | Bitboard::from_sq(C4)
                | Bitboard::from_sq(B5)
                | Bitboard::from_sq(A4)))
        .is_not_empty()
    {
        eval_data::add(e, WC, par.b_wing, 0);
    }
    if p.is_on_sq(BC, P, D5)
        && (p.bishops(BC)
            & (Bitboard::from_sq(H7)
                | Bitboard::from_sq(G6)
                | Bitboard::from_sq(F5)
                | Bitboard::from_sq(G4)
                | Bitboard::from_sq(H5)))
        .is_not_empty()
    {
        eval_data::add(e, BC, par.b_wing, 0);
    }
    if p.is_on_sq(BC, P, E5)
        && (p.bishops(BC)
            & (Bitboard::from_sq(A7)
                | Bitboard::from_sq(B6)
                | Bitboard::from_sq(C5)
                | Bitboard::from_sq(B4)
                | Bitboard::from_sq(A5)))
        .is_not_empty()
    {
        eval_data::add(e, BC, par.b_wing, 0);
    }
    // Knight blocking c pawn
    if p.is_on_sq(WC, P, C2)
        && p.is_on_sq(WC, P, D4)
        && p.is_on_sq(WC, N, C3)
        && (p.pawns(WC) & Bitboard::from_sq(E4)).is_empty()
    {
        eval_data::add(e, WC, par.n_block, 0);
    }
    if p.is_on_sq(BC, P, C7)
        && p.is_on_sq(BC, P, D5)
        && p.is_on_sq(BC, N, C6)
        && (p.pawns(BC) & Bitboard::from_sq(E5)).is_empty()
    {
        eval_data::add(e, BC, par.n_block, 0);
    }
}

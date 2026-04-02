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

//! Pattern evaluation — fianchetto, trapped pieces, blocked pawns, and special patterns.

use super::eval_data::EvalData;
use super::params::EvalParams;
use crate::board::bitboard::*;
use crate::board::masks;
use crate::board::position::Position;
use crate::board::types::*;

/// Evaluate bishop-specific positional patterns for one side.
/// All squares specified in white-relative form, mapped via `sd.rel_sq()`.
fn bishop_patterns_for_side(p: &Position, e: &mut EvalData, par: &EvalParams, sd: Color) {
    let op = !sd;
    let r = |sq: i32| sd.rel_sq(sq);

    // Trapped bishop patterns (A-file)
    if p.is_on_sq(sd, B, r(A6)) && p.is_on_sq(op, P, r(B5)) {
        e.add_both(sd, par.b_trap_a3);
    }
    if p.is_on_sq(sd, B, r(A7)) && p.is_on_sq(op, P, r(B6)) {
        e.add_both(sd, par.b_trap_a2);
    }
    if p.is_on_sq(sd, B, r(B8)) && p.is_on_sq(op, P, r(C7)) {
        e.add_both(sd, par.b_trap_a2);
    }

    // Trapped bishop patterns (H-file)
    if p.is_on_sq(sd, B, r(H6)) && p.is_on_sq(op, P, r(G5)) {
        e.add_both(sd, par.b_trap_a3);
    }
    if p.is_on_sq(sd, B, r(H7)) && p.is_on_sq(op, P, r(G6)) {
        e.add_both(sd, par.b_trap_a2);
    }
    if p.is_on_sq(sd, B, r(G8)) && p.is_on_sq(op, P, r(F7)) {
        e.add_both(sd, par.b_trap_a2);
    }

    // Blocked bishop on C1
    if p.is_on_sq(sd, B, r(C1)) {
        if p.is_on_sq(sd, P, r(D2)) && (Bitboard::from_sq(r(D3)) & p.occ_bb()).is_not_empty() {
            e.add(sd, par.b_block, 0);
        }
        if (p.kings(sd)
            & (Bitboard::from_sq(r(B1)) | Bitboard::from_sq(r(A1)) | Bitboard::from_sq(r(A2))))
        .is_not_empty()
        {
            e.add(sd, par.b_return, 0);
        }
    }

    // Blocked bishop on F1
    if p.is_on_sq(sd, B, r(F1)) {
        if p.is_on_sq(sd, P, r(E2)) && (Bitboard::from_sq(r(E3)) & p.occ_bb()).is_not_empty() {
            e.add(sd, par.b_block, 0);
        }
        if (p.kings(sd)
            & (Bitboard::from_sq(r(G1)) | Bitboard::from_sq(r(H1)) | Bitboard::from_sq(r(H2))))
        .is_not_empty()
        {
            e.add(sd, par.b_return, 0);
        }
    }

    // Fianchetto on B2
    if p.is_on_sq(sd, B, r(B2)) {
        if p.is_on_sq(sd, P, r(C3)) {
            e.add(sd, par.b_bf_mg, par.b_bf_eg);
        }
        if p.is_on_sq(sd, P, r(B3)) && (p.is_on_sq(sd, P, r(A2)) || p.is_on_sq(sd, P, r(C2))) {
            e.add_both(sd, par.b_fianch);
        }
        if p.is_on_sq(op, P, r(D4)) && (p.is_on_sq(op, P, r(E5)) || p.is_on_sq(op, P, r(C5))) {
            e.add_both(sd, par.b_badf);
        }
        if (p.kings(sd) & masks::QS_CASTLE[sd.index()]).is_not_empty() {
            e.add(sd, par.b_king, 0);
        }
    }

    // Fianchetto on G2
    if p.is_on_sq(sd, B, r(G2)) {
        if p.is_on_sq(sd, P, r(F3)) {
            e.add(sd, par.b_bf_mg, par.b_bf_eg);
        }
        if p.is_on_sq(sd, P, r(G3)) && (p.is_on_sq(sd, P, r(H2)) || p.is_on_sq(sd, P, r(F2))) {
            e.add_both(sd, par.b_fianch);
        }
        if p.is_on_sq(op, P, r(E4)) && (p.is_on_sq(op, P, r(D5)) || p.is_on_sq(op, P, r(F5))) {
            e.add_both(sd, par.b_badf);
        }
        if (p.kings(sd) & masks::KS_CASTLE[sd.index()]).is_not_empty() {
            e.add(sd, par.b_king, 0);
        }
    }
}

/// Evaluate bishop-specific positional patterns (fianchetto, trapped, return, bad bishop).
pub fn evaluate_bishop_patterns(p: &Position, e: &mut EvalData, par: &EvalParams) {
    bishop_patterns_for_side(p, e, par, WC);
    bishop_patterns_for_side(p, e, par, BC);
}

/// Evaluate knight-specific positional patterns (trapped on rim).
pub fn evaluate_knight_patterns(p: &Position, e: &mut EvalData, par: &EvalParams) {
    for &sd in &[WC, BC] {
        let op = !sd;
        let r = |sq: i32| sd.rel_sq(sq);
        // Knight trapped on A7/H7 by enemy pawns
        if p.is_on_sq(sd, N, r(A7)) && p.is_on_sq(op, P, r(A6)) && p.is_on_sq(op, P, r(B7)) {
            e.add_both(sd, par.n_trap);
        }
        if p.is_on_sq(sd, N, r(H7)) && p.is_on_sq(op, P, r(H6)) && p.is_on_sq(op, P, r(G7)) {
            e.add_both(sd, par.n_trap);
        }
    }
}

/// Evaluate king shelter, rook-blocked, and castling patterns for one side.
/// All squares are specified in white-relative form and mapped via `sd.rel_sq()`.
fn king_shelter_for_side(p: &Position, e: &mut EvalData, par: &EvalParams, sd: Color) {
    let home_rank = if sd == WC { RANK_1_BB } else { RANK_8_BB };
    if (p.kings(sd) & home_rank).is_empty() {
        return;
    }

    let r = |sq: i32| sd.rel_sq(sq);

    // No-luft patterns (king + pawns blocking escape)
    if p.is_on_sq(sd, K, r(H1)) && p.is_on_sq(sd, P, r(H2)) && p.is_on_sq(sd, P, r(G2)) {
        e.add_both(sd, par.k_no_luft);
    }
    if p.is_on_sq(sd, K, r(G1))
        && p.is_on_sq(sd, P, r(H2))
        && p.is_on_sq(sd, P, r(G2))
        && p.is_on_sq(sd, P, r(F2))
    {
        e.add_both(sd, par.k_no_luft);
    }
    if p.is_on_sq(sd, K, r(A1)) && p.is_on_sq(sd, P, r(A2)) && p.is_on_sq(sd, P, r(B2)) {
        e.add_both(sd, par.k_no_luft);
    }
    if p.is_on_sq(sd, K, r(B1))
        && p.is_on_sq(sd, P, r(A2))
        && p.is_on_sq(sd, P, r(B2))
        && p.is_on_sq(sd, P, r(C2))
    {
        e.add_both(sd, par.k_no_luft);
    }

    // Rook blocked by own king (kingside)
    let km1 = Bitboard::from_sq(r(F1)) | Bitboard::from_sq(r(G1));
    let rm1 = Bitboard::from_sq(r(G1)) | Bitboard::from_sq(r(H1)) | Bitboard::from_sq(r(H2));
    if (p.kings(sd) & km1).is_not_empty() && (p.rooks(sd) & rm1).is_not_empty() {
        e.add(sd, par.r_block_mg, par.r_block_eg);
    }
    // Rook blocked by own king (queenside)
    let km2 = Bitboard::from_sq(r(B1)) | Bitboard::from_sq(r(C1));
    let rm2 = Bitboard::from_sq(r(A1)) | Bitboard::from_sq(r(B1)) | Bitboard::from_sq(r(A2));
    if (p.kings(sd) & km2).is_not_empty() && (p.rooks(sd) & rm2).is_not_empty() {
        e.add(sd, par.r_block_mg, par.r_block_eg);
    }

    // Castling rights bonus
    let (ks, qs) = if sd == WC { (W_KS, W_QS) } else { (B_KS, B_QS) };
    if p.is_on_sq(sd, K, r(E1)) {
        if p.castling & ks != 0 {
            e.add(sd, par.k_castle, 0);
        } else if p.castling & qs != 0 {
            e.add(sd, (par.k_castle * 2) / 3, 0);
        }
    }
}

/// Evaluate king shelter patterns — pawn shield and storm penalties.
pub fn evaluate_king_patterns(p: &Position, e: &mut EvalData, par: &EvalParams) {
    king_shelter_for_side(p, e, par, WC);
    king_shelter_for_side(p, e, par, BC);
}

/// Central pattern evaluation for one side (bishop-wing, knight blocking c-pawn).
/// All squares in white-relative form, mapped via `sd.rel_sq()`.
fn central_patterns_for_side(p: &Position, e: &mut EvalData, par: &EvalParams, sd: Color) {
    let r = |sq: i32| sd.rel_sq(sq);

    // Bishop on the wing with a center pawn
    if p.is_on_sq(sd, P, r(D4))
        && (p.bishops(sd)
            & (Bitboard::from_sq(r(H2))
                | Bitboard::from_sq(r(G3))
                | Bitboard::from_sq(r(F4))
                | Bitboard::from_sq(r(G5))
                | Bitboard::from_sq(r(H4))))
        .is_not_empty()
    {
        e.add(sd, par.b_wing, 0);
    }
    if p.is_on_sq(sd, P, r(E4))
        && (p.bishops(sd)
            & (Bitboard::from_sq(r(A2))
                | Bitboard::from_sq(r(B3))
                | Bitboard::from_sq(r(C4))
                | Bitboard::from_sq(r(B5))
                | Bitboard::from_sq(r(A4))))
        .is_not_empty()
    {
        e.add(sd, par.b_wing, 0);
    }

    // Knight blocking c pawn
    if p.is_on_sq(sd, P, r(C2))
        && p.is_on_sq(sd, P, r(D4))
        && p.is_on_sq(sd, N, r(C3))
        && (p.pawns(sd) & Bitboard::from_sq(r(E4))).is_empty()
    {
        e.add(sd, par.n_block, 0);
    }
}

pub fn evaluate_central_patterns(p: &Position, e: &mut EvalData, par: &EvalParams) {
    central_patterns_for_side(p, e, par, WC);
    central_patterns_for_side(p, e, par, BC);
}

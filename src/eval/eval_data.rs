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

// EvalData — per-evaluation scratch data, matching eData struct.

use crate::board::attacks;
use crate::board::bitboard::*;
use crate::board::position::Position;
use crate::board::types::*;

pub struct EvalData {
    pub mg: [i32; 2],
    pub eg: [i32; 2],
    pub mg_pawns: [i32; 2],
    pub eg_pawns: [i32; 2],
    pub att: [i32; 2],  // king attack score accumulator
    pub wood: [i32; 2], // number of attackers
    pub p_takes: [Bitboard; 2],
    pub two_pawns_take: [Bitboard; 2],
    pub p_can_take: [Bitboard; 2],
    pub all_att: [Bitboard; 2],
    pub ev_att: [Bitboard; 2], // non-pawn, non-king attacks
}

impl EvalData {
    pub fn new() -> Self {
        // SAFETY: All fields are numeric (i32) or Bitboard(u64), where zero
        // is a valid value. This avoids 11 individual field initializations.
        unsafe { std::mem::zeroed() }
    }
}

/// Init pawn helper bitboards and attack maps.
pub fn init_pawn_data(p: &Position, e: &mut EvalData) {
    e.p_takes[WC.index()] = w_pawn_attacks(p.pawns(WC));
    e.p_takes[BC.index()] = b_pawn_attacks(p.pawns(BC));
    e.p_can_take[WC.index()] = fill_north(e.p_takes[WC.index()]);
    e.p_can_take[BC.index()] = fill_south(e.p_takes[BC.index()]);
    e.two_pawns_take[WC.index()] = w_double_pawn_attacks(p.pawns(WC));
    e.two_pawns_take[BC.index()] = b_double_pawn_attacks(p.pawns(BC));

    // Init attack maps with pawn attacks + king attacks
    e.all_att[WC.index()] = e.p_takes[WC.index()] | attacks::king_attacks(p.king_sq(WC));
    e.all_att[BC.index()] = e.p_takes[BC.index()] | attacks::king_attacks(p.king_sq(BC));
    e.ev_att[WC.index()] = Bitboard::EMPTY;
    e.ev_att[BC.index()] = Bitboard::EMPTY;
}

#[inline(always)]
pub fn add(e: &mut EvalData, sd: Color, mg_val: i32, eg_val: i32) {
    e.mg[sd.index()] += mg_val;
    e.eg[sd.index()] += eg_val;
}

#[inline(always)]
pub fn add_both(e: &mut EvalData, sd: Color, val: i32) {
    e.mg[sd.index()] += val;
    e.eg[sd.index()] += val;
}

#[inline(always)]
pub fn add_pawns(e: &mut EvalData, sd: Color, mg_val: i32, eg_val: i32) {
    e.mg_pawns[sd.index()] += mg_val;
    e.eg_pawns[sd.index()] += eg_val;
}

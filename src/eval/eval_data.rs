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

//! Per-evaluation scratch data — accumulates attack maps, king attack counts, and score pairs.

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

impl Default for EvalData {
    fn default() -> Self {
        Self {
            mg: [0; 2],
            eg: [0; 2],
            mg_pawns: [0; 2],
            eg_pawns: [0; 2],
            att: [0; 2],
            wood: [0; 2],
            p_takes: [Bitboard::EMPTY; 2],
            two_pawns_take: [Bitboard::EMPTY; 2],
            p_can_take: [Bitboard::EMPTY; 2],
            all_att: [Bitboard::EMPTY; 2],
            ev_att: [Bitboard::EMPTY; 2],
        }
    }
}

impl EvalData {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add midgame/endgame score for a side.
    #[inline(always)]
    pub fn add(&mut self, sd: Color, mg_val: i32, eg_val: i32) {
        self.mg[sd.index()] += mg_val;
        self.eg[sd.index()] += eg_val;
    }

    /// Add the same value to both mg and eg for a side.
    #[inline(always)]
    pub fn add_both(&mut self, sd: Color, val: i32) {
        self.mg[sd.index()] += val;
        self.eg[sd.index()] += val;
    }

    /// Add midgame/endgame pawn score for a side.
    #[inline(always)]
    pub fn add_pawns(&mut self, sd: Color, mg_val: i32, eg_val: i32) {
        self.mg_pawns[sd.index()] += mg_val;
        self.eg_pawns[sd.index()] += eg_val;
    }

    /// Init pawn helper bitboards and attack maps.
    pub fn init_pawn_data(&mut self, p: &Position) {
        self.p_takes[WC.index()] = w_pawn_attacks(p.pawns(WC));
        self.p_takes[BC.index()] = b_pawn_attacks(p.pawns(BC));
        self.p_can_take[WC.index()] = fill_north(self.p_takes[WC.index()]);
        self.p_can_take[BC.index()] = fill_south(self.p_takes[BC.index()]);
        self.two_pawns_take[WC.index()] = w_double_pawn_attacks(p.pawns(WC));
        self.two_pawns_take[BC.index()] = b_double_pawn_attacks(p.pawns(BC));

        // Init attack maps with pawn attacks + king attacks
        self.all_att[WC.index()] = self.p_takes[WC.index()] | attacks::king_attacks(p.king_sq(WC));
        self.all_att[BC.index()] = self.p_takes[BC.index()] | attacks::king_attacks(p.king_sq(BC));
        self.ev_att[WC.index()] = Bitboard::EMPTY;
        self.ev_att[BC.index()] = Bitboard::EMPTY;
    }
}

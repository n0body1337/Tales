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

//! Static Exchange Evaluation (SEE) — determines the material outcome of a capture sequence.

use crate::board::attacks;
use crate::board::bitboard::Bitboard;
use crate::board::position::Position;
use crate::board::types::*;

/// Static Exchange Evaluation (SEE) — determines if a capture sequence wins material.
pub fn see(pos: &Position, from: i32, to: i32) -> i32 {
    let mut score = [0i32; 32];
    score[0] = TP_VALUE[pos.tp_on_sq(to).index()];

    let mut occ = pos.occ_bb() ^ Bitboard::from_sq(from);

    // Find all attackers (including x-ray through removed pieces)
    let mut attackers = attacks::attacks_to(to, occ, &pos.cl_bb, &pos.tp_bb);
    attackers = attackers
        | (attacks::bishop_attacks(occ, to) & (pos.tp_bb[B.index()] | pos.tp_bb[Q.index()]))
        | (attacks::rook_attacks(occ, to) & (pos.tp_bb[R.index()] | pos.tp_bb[Q.index()]));
    attackers &= occ;

    let mut piece_type = pos.tp_on_sq(from);

    // Determine side — the side NOT moving first (so that we can call Swap out of turn)
    let mut side = if (Bitboard::from_sq(from) & pos.cl_bb[BC.index()]).is_empty() {
        BC
    } else {
        WC
    };

    let mut ply: usize = 1;

    // Iterate through attackers
    while (attackers & pos.cl_bb[side.index()]).is_not_empty() {
        // Break on king capture
        if piece_type == K {
            score[ply] = INF;
            ply += 1;
            break;
        }

        score[ply] = -score[ply - 1] + TP_VALUE[piece_type.index()];

        // Find next weakest attacker
        let mut found = false;
        for tp_idx in 0..6 {
            let tp = PieceType::from_index(tp_idx);
            let type_bb = pos.pc_bb(side, tp) & attackers;
            if type_bb.is_not_empty() {
                piece_type = tp;
                // Remove weakest attacker (LSB of type_bb)
                occ ^= type_bb.lsb_bb();
                found = true;
                break;
            }
        }

        if !found {
            break;
        }

        // Discover new attackers through removed piece
        attackers = attackers
            | (attacks::bishop_attacks(occ, to) & (pos.tp_bb[B.index()] | pos.tp_bb[Q.index()]))
            | (attacks::rook_attacks(occ, to) & (pos.tp_bb[R.index()] | pos.tp_bb[Q.index()]));
        attackers &= occ;

        side = !side;
        ply += 1;
    }

    // Unwind score stack (negamax)
    while ply > 1 {
        ply -= 1;
        score[ply - 1] = -(-score[ply - 1]).max(score[ply]);
    }

    score[0]
}

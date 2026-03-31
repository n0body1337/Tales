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

// Move generation.
// Three generators: captures (+ promotions), quiet (+ castling), special (checking moves).

use crate::board::attacks;
use crate::board::bitboard::*;
use crate::board::moves::*;
use crate::board::position::Position;
use crate::board::types::*;

use super::movelist::MoveList;

// ============================================================================
// Helper: encode move inline
// ============================================================================

#[inline(always)]
fn encode(move_type: u16, to: i32, from: i32) -> Move {
    Move::new(from, to, move_type)
}

#[inline(always)]
fn encode_normal(to: i32, from: i32) -> Move {
    Move::normal(from, to)
}

// ============================================================================
// GenerateCaptures — all captures + promotions (including non-capture promos)
// Generate all capture and promotion moves
// ============================================================================

#[inline]
pub fn generate_captures(pos: &Position, list: &mut MoveList) {
    let sd = pos.side;
    let op = !sd;
    let occ = pos.occ_bb();

    if sd == WC {
        // White pawn captures on rank 7 → promotion captures
        let mut bb =
            Bitboard((pos.pawns(WC).0 & !FILE_A_BB.0 & RANK_7_BB.0) << 7) & pos.cl_bb[BC.index()];
        while bb.is_not_empty() {
            let to = bb.pop_lsb();
            list.push(encode(Q_PROM, to, to - 7));
            list.push(encode(R_PROM, to, to - 7));
            list.push(encode(B_PROM, to, to - 7));
            list.push(encode(N_PROM, to, to - 7));
        }

        bb = Bitboard((pos.pawns(WC).0 & !FILE_H_BB.0 & RANK_7_BB.0) << 9) & pos.cl_bb[BC.index()];
        while bb.is_not_empty() {
            let to = bb.pop_lsb();
            list.push(encode(Q_PROM, to, to - 9));
            list.push(encode(R_PROM, to, to - 9));
            list.push(encode(B_PROM, to, to - 9));
            list.push(encode(N_PROM, to, to - 9));
        }

        // Non-capture promotions (push to rank 8)
        bb = Bitboard((pos.pawns(WC).0 & RANK_7_BB.0) << 8) & pos.unocc_bb();
        while bb.is_not_empty() {
            let to = bb.pop_lsb();
            list.push(encode(Q_PROM, to, to - 8));
            list.push(encode(R_PROM, to, to - 8));
            list.push(encode(B_PROM, to, to - 8));
            list.push(encode(N_PROM, to, to - 8));
        }

        // Normal pawn captures (non-rank-7)
        bb = Bitboard((pos.pawns(WC).0 & !FILE_A_BB.0 & !RANK_7_BB.0) << 7) & pos.cl_bb[BC.index()];
        while bb.is_not_empty() {
            let to = bb.pop_lsb();
            list.push(encode_normal(to, to - 7));
        }

        bb = Bitboard((pos.pawns(WC).0 & !FILE_H_BB.0 & !RANK_7_BB.0) << 9) & pos.cl_bb[BC.index()];
        while bb.is_not_empty() {
            let to = bb.pop_lsb();
            list.push(encode_normal(to, to - 9));
        }

        // En passant
        let ep = pos.ep_sq;
        if ep != NO_SQ {
            if Bitboard((pos.pawns(WC).0 & !FILE_A_BB.0) << 7).contains(ep) {
                list.push(encode(EP_CAP, ep, ep - 7));
            }
            if Bitboard((pos.pawns(WC).0 & !FILE_H_BB.0) << 9).contains(ep) {
                list.push(encode(EP_CAP, ep, ep - 9));
            }
        }
    } else {
        // Black pawn captures on rank 2 → promotion captures
        let mut bb =
            Bitboard((pos.pawns(BC).0 & !FILE_A_BB.0 & RANK_2_BB.0) >> 9) & pos.cl_bb[WC.index()];
        while bb.is_not_empty() {
            let to = bb.pop_lsb();
            list.push(encode(Q_PROM, to, to + 9));
            list.push(encode(R_PROM, to, to + 9));
            list.push(encode(B_PROM, to, to + 9));
            list.push(encode(N_PROM, to, to + 9));
        }

        bb = Bitboard((pos.pawns(BC).0 & !FILE_H_BB.0 & RANK_2_BB.0) >> 7) & pos.cl_bb[WC.index()];
        while bb.is_not_empty() {
            let to = bb.pop_lsb();
            list.push(encode(Q_PROM, to, to + 7));
            list.push(encode(R_PROM, to, to + 7));
            list.push(encode(B_PROM, to, to + 7));
            list.push(encode(N_PROM, to, to + 7));
        }

        // Non-capture promotions
        bb = Bitboard((pos.pawns(BC).0 & RANK_2_BB.0) >> 8) & pos.unocc_bb();
        while bb.is_not_empty() {
            let to = bb.pop_lsb();
            list.push(encode(Q_PROM, to, to + 8));
            list.push(encode(R_PROM, to, to + 8));
            list.push(encode(B_PROM, to, to + 8));
            list.push(encode(N_PROM, to, to + 8));
        }

        // Normal pawn captures (non-rank-2)
        bb = Bitboard((pos.pawns(BC).0 & !FILE_A_BB.0 & !RANK_2_BB.0) >> 9) & pos.cl_bb[WC.index()];
        while bb.is_not_empty() {
            let to = bb.pop_lsb();
            list.push(encode_normal(to, to + 9));
        }

        bb = Bitboard((pos.pawns(BC).0 & !FILE_H_BB.0 & !RANK_2_BB.0) >> 7) & pos.cl_bb[WC.index()];
        while bb.is_not_empty() {
            let to = bb.pop_lsb();
            list.push(encode_normal(to, to + 7));
        }

        // En passant
        let ep = pos.ep_sq;
        if ep != NO_SQ {
            if Bitboard((pos.pawns(BC).0 & !FILE_A_BB.0) >> 9).contains(ep) {
                list.push(encode(EP_CAP, ep, ep + 9));
            }
            if Bitboard((pos.pawns(BC).0 & !FILE_H_BB.0) >> 7).contains(ep) {
                list.push(encode(EP_CAP, ep, ep + 7));
            }
        }
    }

    // Piece captures: knight, bishop, rook, queen, king
    let them = pos.cl_bb[op.index()];

    let mut pieces = pos.knights(sd);
    while pieces.is_not_empty() {
        let from = pieces.pop_lsb();
        let mut moves = attacks::knight_attacks(from) & them;
        while moves.is_not_empty() {
            list.push(encode_normal(moves.pop_lsb(), from));
        }
    }

    pieces = pos.bishops(sd);
    while pieces.is_not_empty() {
        let from = pieces.pop_lsb();
        let mut moves = attacks::bishop_attacks(occ, from) & them;
        while moves.is_not_empty() {
            list.push(encode_normal(moves.pop_lsb(), from));
        }
    }

    pieces = pos.rooks(sd);
    while pieces.is_not_empty() {
        let from = pieces.pop_lsb();
        let mut moves = attacks::rook_attacks(occ, from) & them;
        while moves.is_not_empty() {
            list.push(encode_normal(moves.pop_lsb(), from));
        }
    }

    pieces = pos.queens(sd);
    while pieces.is_not_empty() {
        let from = pieces.pop_lsb();
        let mut moves = attacks::queen_attacks(occ, from) & them;
        while moves.is_not_empty() {
            list.push(encode_normal(moves.pop_lsb(), from));
        }
    }

    let mut moves = attacks::king_attacks(pos.king_sq(sd)) & them;
    while moves.is_not_empty() {
        list.push(encode_normal(moves.pop_lsb(), pos.king_sq(sd)));
    }
}

// ============================================================================
// GenerateQuiet — non-captures: castling, double pawn push, single push, piece moves
// Generate all non-capture, non-promotion moves
// ============================================================================

#[inline]
pub fn generate_quiet(pos: &Position, list: &mut MoveList) {
    let sd = pos.side;
    let occ = pos.occ_bb();
    let empty = pos.unocc_bb();

    if sd == WC {
        // White castling
        if (pos.castling & W_KS) != 0
            && (occ.0 & 0x0000_0000_0000_0060) == 0
            && !attacks::is_attacked(E1, BC, occ, &pos.cl_bb, &pos.tp_bb)
            && !attacks::is_attacked(F1, BC, occ, &pos.cl_bb, &pos.tp_bb)
        {
            list.push(encode(CASTLE, G1, E1));
        }
        if (pos.castling & W_QS) != 0
            && (occ.0 & 0x0000_0000_0000_000E) == 0
            && !attacks::is_attacked(E1, BC, occ, &pos.cl_bb, &pos.tp_bb)
            && !attacks::is_attacked(D1, BC, occ, &pos.cl_bb, &pos.tp_bb)
        {
            list.push(encode(CASTLE, C1, E1));
        }

        // Double pawn push
        let mut bb = Bitboard(((pos.pawns(WC).0 & RANK_2_BB.0) << 8) & empty.0);
        bb = Bitboard((bb.0 << 8) & empty.0);
        while bb.is_not_empty() {
            let to = bb.pop_lsb();
            list.push(encode(EP_SET, to, to - 16));
        }

        // Single pawn push (non-rank-7, since rank-7 pushes are promotions in GenerateCaptures)
        bb = Bitboard((pos.pawns(WC).0 & !RANK_7_BB.0) << 8) & empty;
        while bb.is_not_empty() {
            let to = bb.pop_lsb();
            list.push(encode_normal(to, to - 8));
        }
    } else {
        // Black castling
        if (pos.castling & B_KS) != 0
            && (occ.0 & 0x6000_0000_0000_0000) == 0
            && !attacks::is_attacked(E8, WC, occ, &pos.cl_bb, &pos.tp_bb)
            && !attacks::is_attacked(F8, WC, occ, &pos.cl_bb, &pos.tp_bb)
        {
            list.push(encode(CASTLE, G8, E8));
        }
        if (pos.castling & B_QS) != 0
            && (occ.0 & 0x0E00_0000_0000_0000) == 0
            && !attacks::is_attacked(E8, WC, occ, &pos.cl_bb, &pos.tp_bb)
            && !attacks::is_attacked(D8, WC, occ, &pos.cl_bb, &pos.tp_bb)
        {
            list.push(encode(CASTLE, C8, E8));
        }

        // Double pawn push
        let mut bb = Bitboard(((pos.pawns(BC).0 & RANK_7_BB.0) >> 8) & empty.0);
        bb = Bitboard((bb.0 >> 8) & empty.0);
        while bb.is_not_empty() {
            let to = bb.pop_lsb();
            list.push(encode(EP_SET, to, to + 16));
        }

        // Single pawn push (non-rank-2)
        bb = Bitboard((pos.pawns(BC).0 & !RANK_2_BB.0) >> 8) & empty;
        while bb.is_not_empty() {
            let to = bb.pop_lsb();
            list.push(encode_normal(to, to + 8));
        }
    }

    // Piece quiet moves: knight, bishop, rook, queen, king

    let mut pieces = pos.knights(sd);
    while pieces.is_not_empty() {
        let from = pieces.pop_lsb();
        let mut moves = attacks::knight_attacks(from) & empty;
        while moves.is_not_empty() {
            list.push(encode_normal(moves.pop_lsb(), from));
        }
    }

    pieces = pos.bishops(sd);
    while pieces.is_not_empty() {
        let from = pieces.pop_lsb();
        let mut moves = attacks::bishop_attacks(occ, from) & empty;
        while moves.is_not_empty() {
            list.push(encode_normal(moves.pop_lsb(), from));
        }
    }

    pieces = pos.rooks(sd);
    while pieces.is_not_empty() {
        let from = pieces.pop_lsb();
        let mut moves = attacks::rook_attacks(occ, from) & empty;
        while moves.is_not_empty() {
            list.push(encode_normal(moves.pop_lsb(), from));
        }
    }

    pieces = pos.queens(sd);
    while pieces.is_not_empty() {
        let from = pieces.pop_lsb();
        let mut moves = attacks::queen_attacks(occ, from) & empty;
        while moves.is_not_empty() {
            list.push(encode_normal(moves.pop_lsb(), from));
        }
    }

    let mut moves = attacks::king_attacks(pos.king_sq(sd)) & empty;
    while moves.is_not_empty() {
        list.push(encode_normal(moves.pop_lsb(), pos.king_sq(sd)));
    }
}

// ============================================================================
// GenerateSpecial — quiet checking moves only (for quiescence search)
// Generate special evasion and check moves (used in quiescence search)
// ============================================================================

#[inline]
pub fn generate_special(pos: &Position, list: &mut MoveList) {
    let sd = pos.side;
    let op = !sd;
    let occ = pos.occ_bb();
    let empty = pos.unocc_bb();

    let king_sq = pos.king_sq(op);
    let n_check = attacks::knight_attacks(king_sq);
    let r_check = attacks::rook_attacks(occ, king_sq);
    let b_check = attacks::bishop_attacks(occ, king_sq);
    let p_check = shift_fwd(shift_sideways(Bitboard::from_sq(king_sq)), op);

    if sd == WC {
        // Double push checking
        let mut bb = Bitboard(((pos.pawns(WC).0 & RANK_2_BB.0) << 8) & empty.0);
        bb = Bitboard((bb.0 << 8) & empty.0) & p_check;
        while bb.is_not_empty() {
            let to = bb.pop_lsb();
            list.push(encode(EP_SET, to, to - 16));
        }

        // Single push checking
        bb = Bitboard((pos.pawns(WC).0 & !RANK_7_BB.0) << 8) & empty & p_check;
        while bb.is_not_empty() {
            let to = bb.pop_lsb();
            list.push(encode_normal(to, to - 8));
        }
    } else {
        let mut bb = Bitboard(((pos.pawns(BC).0 & RANK_7_BB.0) >> 8) & empty.0);
        bb = Bitboard((bb.0 >> 8) & empty.0) & p_check;
        while bb.is_not_empty() {
            let to = bb.pop_lsb();
            list.push(encode(EP_SET, to, to + 16));
        }

        bb = Bitboard((pos.pawns(BC).0 & !RANK_2_BB.0) >> 8) & empty & p_check;
        while bb.is_not_empty() {
            let to = bb.pop_lsb();
            list.push(encode_normal(to, to + 8));
        }
    }

    // Knight — direct checks or discovered
    let mut pieces = pos.knights(sd);
    while pieces.is_not_empty() {
        let from = pieces.pop_lsb();
        let discovers = can_discover_check(pos, from, op);
        let mut moves = attacks::knight_attacks(from) & empty;
        if !discovers {
            moves &= n_check;
        }
        while moves.is_not_empty() {
            list.push(encode_normal(moves.pop_lsb(), from));
        }
    }

    // Bishop — direct or discovered via straight movers
    pieces = pos.bishops(sd);
    while pieces.is_not_empty() {
        let from = pieces.pop_lsb();
        let discovers = can_discover_check_by(pos, pos.straight_movers(sd), op, from);
        let mut moves = attacks::bishop_attacks(occ, from) & empty;
        if !discovers {
            moves &= b_check;
        }
        while moves.is_not_empty() {
            list.push(encode_normal(moves.pop_lsb(), from));
        }
    }

    // Rook — direct or discovered via diag movers
    pieces = pos.rooks(sd);
    while pieces.is_not_empty() {
        let from = pieces.pop_lsb();
        let discovers = can_discover_check_by(pos, pos.diag_movers(sd), op, from);
        let mut moves = attacks::rook_attacks(occ, from) & empty;
        if !discovers {
            moves &= r_check;
        }
        while moves.is_not_empty() {
            list.push(encode_normal(moves.pop_lsb(), from));
        }
    }

    // Queen — only direct checks (no discovered)
    pieces = pos.queens(sd);
    while pieces.is_not_empty() {
        let from = pieces.pop_lsb();
        let mut moves = attacks::queen_attacks(occ, from) & empty;
        moves &= r_check | b_check;
        while moves.is_not_empty() {
            list.push(encode_normal(moves.pop_lsb(), from));
        }
    }
}

// ============================================================================
// CanDiscoverCheck — tests whether moving a piece reveals a discovered check
// ============================================================================

/// Can any checker discover a check by moving `from` out of the way?
fn can_discover_check(pos: &Position, from: i32, op: Color) -> bool {
    let checkers = pos.queens(pos.side) | pos.rooks(pos.side) | pos.bishops(pos.side);
    can_discover_check_by(pos, checkers, op, from)
}

/// Can the given `checkers` bitboard discover check by moving `from` out of the way?
fn can_discover_check_by(pos: &Position, mut checkers: Bitboard, op: Color, from: i32) -> bool {
    let occ = pos.occ_bb();
    while checkers.is_not_empty() {
        let checker = checkers.pop_lsb();
        let ray = between(checker, pos.king_sq[op.index()]);
        if Bitboard::from_sq(from) & ray != Bitboard::EMPTY && (ray & occ).popcount() == 1 {
            return true;
        }
    }
    false
}

// ============================================================================
// Perft — node-count verification for move generation correctness.
// ============================================================================

use crate::board::position::Undo;

/// Perft function — count leaf nodes at depth `depth`.
pub fn perft(pos: &mut Position, depth: i32) -> u64 {
    if depth == 0 {
        return 1;
    }

    let mut list = MoveList::new();
    generate_captures(pos, &mut list);
    generate_quiet(pos, &mut list);

    let mut nodes: u64 = 0;
    for i in 0..list.count {
        let mv = list.get(i);
        let mut u = Undo::uninit();
        pos.do_move(mv, &mut u);
        if !pos.illegal() {
            nodes += perft(pos, depth - 1);
        }
        pos.undo_move(mv, &u);
    }
    nodes
}

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

// Endgame evaluation — draw detection, draw scaling, and checkmate helper.

use super::params::EvalParams;
use crate::board::attacks;
use crate::board::bitboard::*;
use crate::board::distance;
use crate::board::masks;
use crate::board::position::Position;
use crate::board::types::*;

// ============================================================================
// BN mate tables — drive the king toward the correct corner
// ============================================================================

#[rustfmt::skip]
const BN_WB: [i32; 64] = [
      0,   0,  15,  30,  45,  60,  85, 100,
      0,  15,  30,  45,  60,  85, 100,  85,
     15,  30,  45,  60,  85, 100,  85,  60,
     30,  45,  60,  85, 100,  85,  60,  45,
     45,  60,  85, 100,  85,  60,  45,  30,
     60,  85, 100,  85,  60,  45,  30,  15,
     85, 100,  85,  60,  45,  30,  15,   0,
    100,  85,  60,  45,  30,  15,   0,   0,
];

#[rustfmt::skip]
const BN_BB: [i32; 64] = [
    100,  85,  60,  45,  30,  15,   0,   0,
     85, 100,  85,  60,  45,  30,  15,   0,
     60,  85, 100,  85,  60,  45,  30,  15,
     45,  60,  85, 100,  85,  60,  45,  30,
     30,  45,  60,  85, 100,  85,  60,  45,
     15,  30,  45,  60,  85, 100,  85,  60,
      0,  15,  30,  45,  60,  85, 100,  85,
      0,   0,  15,  30,  45,  60,  85, 100,
];

// ============================================================================
// King blocking masks for rook-pawn draws
// ============================================================================

const BB_KING_BLOCK_H: [Bitboard; 2] = [
    // White's A-pawn promotes on H8 side:  H8, H7, G8, G7
    Bitboard((1u64 << H8) | (1u64 << H7) | (1u64 << G8) | (1u64 << G7)),
    // Black's A-pawn promotes on H1 side:  H1, H2, G1, G2
    Bitboard((1u64 << H1) | (1u64 << H2) | (1u64 << G1) | (1u64 << G2)),
];

const BB_KING_BLOCK_A: [Bitboard; 2] = [
    // White's A-pawn promotes on A8 side: A8, A7, B8, B7
    Bitboard((1u64 << A8) | (1u64 << A7) | (1u64 << B8) | (1u64 << B7)),
    // Black's A-pawn promotes on A1 side: A1, A2, B1, B2
    Bitboard((1u64 << A1) | (1u64 << A2) | (1u64 << B1) | (1u64 << B2)),
];

// ============================================================================
// CheckmateHelper — evaluates checkmate-driving positions (KBN vs K, KR vs K, etc.)
// ============================================================================

pub fn checkmate_helper(p: &Position, par: &EvalParams) -> i32 {
    let mut result = 0;

    // KQ vs Kx: drive enemy king towards the edge (white has queen)
    if p.cnt[0][Q.index()] > 0
        && p.cnt[0][P.index()] == 0
        && p.cnt[1][Q.index()] == 0
        && p.cnt[1][P.index()] == 0
        && p.cnt[1][R.index()] + p.cnt[1][B.index()] + p.cnt[1][N.index()] <= 1
    {
        result += 200;
        result += 10 * distance::bonus(p.king_sq(WC), p.king_sq(BC));
        result -= par.eg_pst[1][K.index()][p.king_sq(BC) as usize];
        return result;
    }

    // KQ vs Kx: drive enemy king towards the edge (black has queen)
    if p.cnt[1][Q.index()] > 0
        && p.cnt[1][P.index()] == 0
        && p.cnt[0][Q.index()] == 0
        && p.cnt[0][P.index()] == 0
        && p.cnt[0][R.index()] + p.cnt[0][B.index()] + p.cnt[0][N.index()] <= 1
    {
        result -= 200;
        result -= 10 * distance::bonus(p.king_sq(WC), p.king_sq(BC));
        result += par.eg_pst[1][K.index()][p.king_sq(BC) as usize];
        return result;
    }

    // Weaker side has bare king (KQK, KRK, KBBK + bigger advantage) — white stronger
    if p.cnt[1][P.index()]
        + p.cnt[1][N.index()]
        + p.cnt[1][B.index()]
        + p.cnt[1][R.index()]
        + p.cnt[1][Q.index()]
        == 0
        && ((p.cnt[0][Q.index()] + p.cnt[0][R.index()] > 0) || p.cnt[0][B.index()] > 1)
    {
        result += 200;
        result += 10 * distance::bonus(p.king_sq(WC), p.king_sq(BC));
        result -= par.eg_pst[1][K.index()][p.king_sq(BC) as usize];
    }

    // Weaker side has bare king — black stronger
    if p.cnt[0][P.index()]
        + p.cnt[0][N.index()]
        + p.cnt[0][B.index()]
        + p.cnt[0][R.index()]
        + p.cnt[0][Q.index()]
        == 0
        && ((p.cnt[1][Q.index()] + p.cnt[1][R.index()] > 0) || p.cnt[1][B.index()] > 1)
    {
        result -= 200;
        result -= 10 * distance::bonus(p.king_sq(WC), p.king_sq(BC));
        result += par.eg_pst[1][K.index()][p.king_sq(BC) as usize];
    }

    // KBN vs K specialized code
    if p.cnt[0][P.index()] == 0 && p.cnt[1][P.index()] == 0 && p.phase == 2 {
        if p.cnt[0][B.index()] == 1 && p.cnt[0][N.index()] == 1 {
            if (p.bishops(WC) & WHITE_SQUARES).is_not_empty() {
                result -= 2 * BN_BB[p.king_sq(BC) as usize];
            }
            if (p.bishops(WC) & BLACK_SQUARES).is_not_empty() {
                result -= 2 * BN_WB[p.king_sq(BC) as usize];
            }
        }

        if p.cnt[1][B.index()] == 1 && p.cnt[1][N.index()] == 1 {
            if (p.bishops(BC) & WHITE_SQUARES).is_not_empty() {
                result += 2 * BN_BB[p.king_sq(WC) as usize];
            }
            if (p.bishops(BC) & BLACK_SQUARES).is_not_empty() {
                result += 2 * BN_WB[p.king_sq(WC) as usize];
            }
        }
    }

    result
}

// ============================================================================
// GetDrawFactor — scales evaluation toward draws in insufficient material positions
// ============================================================================

pub fn get_draw_factor(p: &Position, sd: Color) -> i32 {
    let op = !sd;
    let si = sd.index();
    let oi = op.index();

    if p.phase < 2 && p.cnt[si][P.index()] == 0 {
        return 0; // KK, KmK, KmKp, KmKpp
    }

    if p.phase == 0 {
        return scale_pawns_only(p, sd, op);
    }

    if p.phase == 1 {
        if p.cnt[si][B.index()] == 1 {
            return scale_kbpk(p, sd, op);
        }
        if p.cnt[si][N.index()] == 1 {
            return scale_knpk(p, sd, op);
        }
    }

    if p.phase == 2 {
        if p.cnt[si][N.index()] == 2 && p.cnt[si][P.index()] == 0 {
            if p.cnt[oi][P.index()] == 0 {
                return 0; // KNNK(m)
            }
            return 8; // KNNK(m)(p)
        }

        if p.cnt[si][B.index()] == 2 && p.cnt[si][P.index()] == 0 {
            // KBBK, same coloured bishops
            if (p.bishops(sd) & WHITE_SQUARES).more_than_one()
                || (p.bishops(sd) & BLACK_SQUARES).more_than_one()
            {
                return 0;
            }
        }

        // KBPKm, king blocks
        if p.cnt[si][B.index()] == 1
            && p.cnt[oi][B.index()] + p.cnt[oi][N.index()] == 1
            && p.cnt[si][P.index()] == 1
            && p.cnt[oi][P.index()] == 0
            && (Bitboard::from_sq(p.king_sq(op)) & get_front_span(p.pawns(sd), sd)).is_not_empty()
            && not_on_bishop_color(p, sd, p.king_sq(op))
        {
            return 0;
        }

        if p.cnt[si][B.index()] == 1 && p.cnt[oi][B.index()] == 1 && different_bishops(p) {
            if (masks::HOME[si] & p.pawns(sd)).is_not_empty()
                && p.cnt[si][P.index()] == 1
                && p.cnt[oi][P.index()] == 0
            {
                return 8; // KBPKB, BOC, pawn on own half
            }
            return 32; // BOC, any number of pawns
        }
    }

    if p.phase == 3 && p.cnt[si][P.index()] == 0 {
        if p.cnt[si][R.index()] == 1 && p.cnt[oi][B.index()] + p.cnt[oi][N.index()] == 1 {
            return 16; // KRKm(p)
        }
        if p.cnt[si][B.index()] + p.cnt[si][N.index()] == 2 && p.cnt[oi][B.index()] == 1 {
            return 8; // KmmKB(p)
        }
        if p.cnt[si][B.index()] == 1
            && p.cnt[si][N.index()] == 1
            && p.cnt[oi][B.index()] + p.cnt[oi][N.index()] == 1
        {
            return 8; // KBNKm(p)
        }
    }

    if p.phase == 4 && p.cnt[si][R.index()] == 1 && p.cnt[oi][R.index()] == 1 {
        if p.cnt[si][P.index()] == 0 && p.cnt[oi][P.index()] == 0 {
            return 8; // KRKR
        }
        if p.cnt[si][P.index()] == 1 && p.cnt[oi][P.index()] == 0 {
            return scale_krpkr(p, sd, op); // KRPKR
        }
    }

    if p.phase == 5
        && p.cnt[si][P.index()] == 0
        && p.cnt[si][R.index()] == 1
        && p.cnt[si][B.index()] + p.cnt[si][N.index()] == 1
        && p.cnt[oi][R.index()] == 1
    {
        return 16; // KRMKR(p)
    }

    if p.phase == 6
        && p.cnt[si][Q.index()] == 1
        && p.cnt[oi][R.index()] == 1
        && p.cnt[si][P.index()] == 0
    {
        return scale_kqkrp(p, sd, op);
    }

    if p.phase == 7
        && p.cnt[si][P.index()] == 0
        && p.cnt[si][R.index()] == 2
        && p.cnt[oi][B.index()] + p.cnt[oi][N.index()] == 1
        && p.cnt[oi][R.index()] == 1
    {
        return 16; // KRRKRm(p)
    }

    if p.phase == 9 && p.cnt[si][P.index()] == 0 {
        if p.cnt[si][R.index()] == 2
            && p.cnt[si][B.index()] + p.cnt[si][N.index()] == 1
            && p.cnt[oi][R.index()] == 2
        {
            return 16; // KRRMKRR(p)
        }
        if p.cnt[si][Q.index()] == 1
            && p.cnt[si][B.index()] + p.cnt[si][N.index()] == 1
            && p.cnt[oi][Q.index()] == 1
        {
            return 16; // KQmKQ(p)
        }
    }

    64 // default
}

// ============================================================================
// Scale functions — adjust draw factors based on material configuration
// ============================================================================

fn scale_pawns_only(p: &Position, sd: Color, op: Color) -> i32 {
    let si = sd.index();

    if p.cnt[op.index()][P.index()] == 0 {
        // All pawns on the h file
        if (p.pawns(sd) & FILE_H_BB) == p.pawns(sd)
            && (p.kings(op) & BB_KING_BLOCK_H[si]).is_not_empty()
        {
            return 0;
        }
        // All pawns on the a file
        if (p.pawns(sd) & FILE_A_BB) == p.pawns(sd)
            && (p.kings(op) & BB_KING_BLOCK_A[si]).is_not_empty()
        {
            return 0;
        }
    }

    64 // default
}

fn scale_knpk(p: &Position, sd: Color, op: Color) -> i32 {
    let si = sd.index();
    let oi = op.index();

    // KNPK draw rule: king blocking an edge pawn on 7th rank draws
    if p.cnt[si][N.index()] == 1 && p.cnt[si][P.index()] == 1 && p.cnt[oi][P.index()] == 0 {
        // Edge A pawn: pawn on A7 (rel), enemy king on A8 (rel)
        if (rel_sq_bb(A7, sd) & p.pawns(sd)).is_not_empty()
            && (rel_sq_bb(A8, sd) & p.kings(op)).is_not_empty()
        {
            return 0;
        }
        // Edge H pawn: pawn on H7 (rel), enemy king on H8 (rel)
        if (rel_sq_bb(H7, sd) & p.pawns(sd)).is_not_empty()
            && (rel_sq_bb(H8, sd) & p.kings(op)).is_not_empty()
        {
            return 0;
        }
    }

    64 // default
}

fn scale_kbpk(p: &Position, sd: Color, op: Color) -> i32 {
    let si = sd.index();

    // All pawns on H file with wrong-color bishop
    if (p.pawns(sd) & FILE_H_BB) == p.pawns(sd)
        && not_on_bishop_color(p, sd, rel_sq(H8, sd))
        && (p.kings(op) & BB_KING_BLOCK_H[si]).is_not_empty()
    {
        return 0;
    }

    // All pawns on A file with wrong-color bishop
    if (p.pawns(sd) & FILE_A_BB) == p.pawns(sd)
        && not_on_bishop_color(p, sd, rel_sq(A8, sd))
        && (p.kings(op) & BB_KING_BLOCK_A[si]).is_not_empty()
    {
        return 0;
    }

    64 // default
}

fn scale_krpkr(p: &Position, sd: Color, op: Color) -> i32 {
    let si = sd.index();

    // Specific KRPKR dead draws
    // A-pawn on 7th, rook on 8th, enemy rook on A file, enemy king on H7/G7
    if (rel_sq_bb(A7, sd) & p.pawns(sd)).is_not_empty()
        && (rel_sq_bb(A8, sd) & p.rooks(sd)).is_not_empty()
        && (FILE_A_BB & p.rooks(op)).is_not_empty()
        && ((rel_sq_bb(H7, sd) & p.kings(op)).is_not_empty()
            || (rel_sq_bb(G7, sd) & p.kings(op)).is_not_empty())
    {
        return 0;
    }

    // H-pawn on 7th, rook on 8th, enemy rook on H file, enemy king on A7/B7
    if (rel_sq_bb(H7, sd) & p.pawns(sd)).is_not_empty()
        && (rel_sq_bb(H8, sd) & p.rooks(sd)).is_not_empty()
        && (FILE_H_BB & p.rooks(op)).is_not_empty()
        && ((rel_sq_bb(A7, sd) & p.kings(op)).is_not_empty()
            || (rel_sq_bb(B7, sd) & p.kings(op)).is_not_empty())
    {
        return 0;
    }

    let bb_span = get_front_span(p.pawns(sd), sd);
    let prom_sq = (REL_RANK_BB[si][7] & bb_span).lsb();
    let strong_king = p.king_sq(sd);
    let weak_king = p.king_sq(op);
    let strong_pawn = p.pawns(sd).lsb();
    let weak_rook = p.rooks(op).lsb();
    let tempo = i32::from(p.side == sd);
    let bb_safe_zone = Bitboard(masks::HOME[si].0 ^ REL_RANK_BB[si][4].0);

    if (p.pawns(sd) & bb_safe_zone).is_not_empty() {
        // king of the weaker side blocks pawn
        if (shift_fwd(p.pawns(sd), sd) & p.kings(op)).is_not_empty()
            && distance::metric(strong_king, strong_pawn) - tempo >= 2
            && distance::metric(strong_king, weak_rook) - tempo >= 2
        {
            return 0;
        }

        // third rank defence
        if distance::metric(weak_king, prom_sq) <= 1
            && strong_king <= H5
            && (p.rooks(op) & REL_RANK_BB[si][5]).is_not_empty()
        {
            return 0;
        }
    } else {
        // advanced enemy pawn — continuation of third rank defence
        if (p.pawns(sd) & REL_RANK_BB[si][5]).is_not_empty()
            && distance::metric(weak_king, prom_sq) <= 1
            && ((p.kings(sd) & bb_safe_zone).is_not_empty()
                || (tempo == 0 && (p.kings(sd) & REL_RANK_BB[si][5]).is_not_empty()))
            && (p.rooks(op) & REL_RANK_BB[si][0]).is_not_empty()
        {
            return 0;
        }
    }

    // catch-all bonus for well-positioned defending king
    if (p.kings(op) & bb_span).is_not_empty() {
        return 32; // defending king on pawn's path: 1/2
    }

    64 // default: no scaling
}

fn scale_kqkrp(p: &Position, _sd: Color, op: Color) -> i32 {
    let si = (!op).index();

    let bb_defended = p.pawns(op) & REL_RANK_BB[si][6];
    let bb_defended = bb_defended & attacks::king_attacks(p.king_sq(op));

    // fortress: rook defended by a pawn on the 7th rank (relative to sd), pawn defended by king
    if (p.rooks(op) & pawn_attacks_bb(bb_defended, op)).is_not_empty() {
        return 8;
    }

    64 // default
}

// ============================================================================
// Helper functions
// ============================================================================

fn not_on_bishop_color(p: &Position, bish_side: Color, sq: i32) -> bool {
    let bb_sq = Bitboard::from_sq(sq);
    if (WHITE_SQUARES & p.bishops(bish_side)).is_empty() && (bb_sq & WHITE_SQUARES).is_not_empty() {
        return true;
    }
    if (BLACK_SQUARES & p.bishops(bish_side)).is_empty() && (bb_sq & BLACK_SQUARES).is_not_empty() {
        return true;
    }
    false
}

fn different_bishops(p: &Position) -> bool {
    if (WHITE_SQUARES & p.bishops(WC)).is_not_empty()
        && (BLACK_SQUARES & p.bishops(BC)).is_not_empty()
    {
        return true;
    }
    if (BLACK_SQUARES & p.bishops(WC)).is_not_empty()
        && (WHITE_SQUARES & p.bishops(BC)).is_not_empty()
    {
        return true;
    }
    false
}

/// Relative square: flips for black.
fn rel_sq(sq: i32, sd: Color) -> i32 {
    if sd == WC { sq } else { sq ^ 56 }
}

/// Get a bitboard for a relative square.
fn rel_sq_bb(sq: i32, sd: Color) -> Bitboard {
    Bitboard::from_sq(rel_sq(sq, sd))
}

/// Front span of pawns.
pub fn get_front_span(pawns: Bitboard, sd: Color) -> Bitboard {
    let mut bb = pawns;
    if sd == WC {
        bb = Bitboard(bb.0 | (bb.0 << 8));
        bb = Bitboard(bb.0 | (bb.0 << 16));
        bb = Bitboard(bb.0 | (bb.0 << 32));
    } else {
        bb = Bitboard(bb.0 | (bb.0 >> 8));
        bb = Bitboard(bb.0 | (bb.0 >> 16));
        bb = Bitboard(bb.0 | (bb.0 >> 32));
    }
    bb
}

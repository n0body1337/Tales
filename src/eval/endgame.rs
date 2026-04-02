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

//! Endgame evaluation — draw detection, draw scaling, and checkmate assistance.

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

/// Mate-driving bonus for one side: penalizes the weaker king's PST and rewards proximity.
fn mate_drive(p: &Position, par: &EvalParams, sd: Color) -> i32 {
    let op = !sd;
    let mut result = 0;

    // KQ vs Kx: strong side has queen, weak side has at most one minor
    if p.count(sd, Q) > 0
        && p.count(sd, P) == 0
        && p.count(op, Q) == 0
        && p.count(op, P) == 0
        && p.count(op, R) + p.count(op, B) + p.count(op, N) <= 1
    {
        result += 200;
        result += 10 * distance::bonus(p.king_sq(sd), p.king_sq(op));
        result -= par.eg_pst[op.index()][K.index()][p.king_sq(op) as usize];
        return result;
    }

    // Bare king: weaker side has no material at all
    if p.count(op, P) + p.count(op, N) + p.count(op, B) + p.count(op, R) + p.count(op, Q) == 0
        && ((p.count(sd, Q) + p.count(sd, R) > 0) || p.count(sd, B) > 1)
    {
        result += 200;
        result += 10 * distance::bonus(p.king_sq(sd), p.king_sq(op));
        result -= par.eg_pst[op.index()][K.index()][p.king_sq(op) as usize];
    }

    result
}

/// KBN vs K corner-driving bonus for one side.
fn kbn_adjust(p: &Position, sd: Color) -> i32 {
    let op = !sd;
    let mut result = 0;
    if p.count(sd, B) == 1 && p.count(sd, N) == 1 {
        if (p.bishops(sd) & WHITE_SQUARES).is_not_empty() {
            result += 2 * BN_BB[p.king_sq(op) as usize];
        }
        if (p.bishops(sd) & BLACK_SQUARES).is_not_empty() {
            result += 2 * BN_WB[p.king_sq(op) as usize];
        }
    }
    result
}

/// Checkmate-driving evaluation for positions with decisive material advantage.
pub fn checkmate_helper(p: &Position, par: &EvalParams) -> i32 {
    let mut result = mate_drive(p, par, WC) - mate_drive(p, par, BC);

    // KBN vs K specialized corner-driving
    if p.count(WC, P) == 0 && p.count(BC, P) == 0 && p.phase == 2 {
        result += kbn_adjust(p, WC) - kbn_adjust(p, BC);
    }

    result
}

// ============================================================================
// GetDrawFactor — scales evaluation toward draws in insufficient material positions
// ============================================================================

/// Draw factor scaling — returns 0..64 where 64 = no scaling, 0 = drawn.
///
/// Phase is the sum of non-pawn piece weights: N=1, B=1, R=2, Q=4.
/// Example: phase 2 = two minors OR one rook; phase 4 = two rooks, etc.
pub fn get_draw_factor(p: &Position, sd: Color) -> i32 {
    let op = !sd;
    let si = sd.index();

    if p.phase < 2 && p.count(sd, P) == 0 {
        return 0; // KK, KmK, KmKp, KmKpp
    }

    if p.phase == 0 {
        // pawns only
        return scale_pawns_only(p, sd, op);
    }

    if p.phase == 1 {
        // one minor
        if p.count(sd, B) == 1 {
            return scale_kbpk(p, sd, op);
        }
        if p.count(sd, N) == 1 {
            return scale_knpk(p, sd, op);
        }
    }

    if p.phase == 2 {
        // two minors or one rook
        if p.count(sd, N) == 2 && p.count(sd, P) == 0 {
            if p.count(op, P) == 0 {
                return 0; // KNNK(m)
            }
            return 8; // KNNK(m)(p)
        }

        if p.count(sd, B) == 2 && p.count(sd, P) == 0 {
            // KBBK, same coloured bishops
            if (p.bishops(sd) & WHITE_SQUARES).more_than_one()
                || (p.bishops(sd) & BLACK_SQUARES).more_than_one()
            {
                return 0;
            }
        }

        // KBPKm, king blocks
        if p.count(sd, B) == 1
            && p.count(op, B) + p.count(op, N) == 1
            && p.count(sd, P) == 1
            && p.count(op, P) == 0
            && (Bitboard::from_sq(p.king_sq(op)) & get_front_span_inclusive(p.pawns(sd), sd))
                .is_not_empty()
            && not_on_bishop_color(p, sd, p.king_sq(op))
        {
            return 0;
        }

        if p.count(sd, B) == 1 && p.count(op, B) == 1 && different_bishops(p) {
            if (masks::HOME[si] & p.pawns(sd)).is_not_empty()
                && p.count(sd, P) == 1
                && p.count(op, P) == 0
            {
                return 8; // KBPKB, BOC, pawn on own half
            }
            return 32; // BOC, any number of pawns
        }
    }

    if p.phase == 3 && p.count(sd, P) == 0 {
        // R+m or 3 minors
        if p.count(sd, R) == 1 && p.count(op, B) + p.count(op, N) == 1 {
            return 16; // KRKm(p)
        }
        if p.count(sd, B) + p.count(sd, N) == 2 && p.count(op, B) == 1 {
            return 8; // KmmKB(p)
        }
        if p.count(sd, B) == 1 && p.count(sd, N) == 1 && p.count(op, B) + p.count(op, N) == 1 {
            return 8; // KBNKm(p)
        }
    }

    if p.phase == 4 && p.count(sd, R) == 1 && p.count(op, R) == 1 {
        // two rooks total
        if p.count(sd, P) == 0 && p.count(op, P) == 0 {
            return 8; // KRKR
        }
        if p.count(sd, P) == 1 && p.count(op, P) == 0 {
            return scale_krpkr(p, sd, op); // KRPKR
        }
    }

    if p.phase == 5
        // R+R+m or R+Q
        && p.count(sd, P) == 0
        && p.count(sd, R) == 1
        && p.count(sd, B) + p.count(sd, N) == 1
        && p.count(op, R) == 1
    {
        return 16; // KRMKR(p)
    }

    if p.phase == 6
        // Q+R total
        && p.count(sd, Q) == 1
        && p.count(op, R) == 1
        && p.count(sd, P) == 0
    {
        return scale_kqkrp(p, sd, op);
    }

    if p.phase == 7
        // two rooks + minor
        && p.count(sd, P) == 0
        && p.count(sd, R) == 2
        && p.count(op, B) + p.count(op, N) == 1
        && p.count(op, R) == 1
    {
        return 16; // KRRKRm(p)
    }

    if p.phase == 9 && p.count(sd, P) == 0 {
        // Q+R+m or 2R+minors
        if p.count(sd, R) == 2 && p.count(sd, B) + p.count(sd, N) == 1 && p.count(op, R) == 2 {
            return 16; // KRRMKRR(p)
        }
        if p.count(sd, Q) == 1 && p.count(sd, B) + p.count(sd, N) == 1 && p.count(op, Q) == 1 {
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

    if p.count(op, P) == 0 {
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
    // KNPK draw rule: king blocking an edge pawn on 7th rank draws
    if p.count(sd, N) == 1 && p.count(sd, P) == 1 && p.count(op, P) == 0 {
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
        && not_on_bishop_color(p, sd, sd.rel_sq(H8))
        && (p.kings(op) & BB_KING_BLOCK_H[si]).is_not_empty()
    {
        return 0;
    }

    // All pawns on A file with wrong-color bishop
    if (p.pawns(sd) & FILE_A_BB) == p.pawns(sd)
        && not_on_bishop_color(p, sd, sd.rel_sq(A8))
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

    let bb_span = get_front_span_inclusive(p.pawns(sd), sd);
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

/// Get a bitboard for a relative square.
fn rel_sq_bb(sq: i32, sd: Color) -> Bitboard {
    Bitboard::from_sq(sd.rel_sq(sq))
}

/// Front span of pawns — all squares forward INCLUDING the starting rank.
/// Distinct from `bitboard::get_front_span()` which EXCLUDES the starting rank.
pub fn get_front_span_inclusive(pawns: Bitboard, sd: Color) -> Bitboard {
    if sd == WC {
        fill_north(pawns)
    } else {
        fill_south(pawns)
    }
}

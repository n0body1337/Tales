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

//! Core chess types — color, piece, square, and castling enums with associated constants.

use std::fmt;
use std::ops::Not;

// ============================================================================
// Color
// ============================================================================

/// Side to move — White or Black.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Color {
    White = 0,
    Black = 1,
}

/// White color shorthand.
pub const WC: Color = Color::White;
/// Black color shorthand.
pub const BC: Color = Color::Black;

impl Not for Color {
    type Output = Color;
    #[inline(always)]
    fn not(self) -> Color {
        match self {
            Color::White => Color::Black,
            Color::Black => Color::White,
        }
    }
}

impl Color {
    /// Returns the index (0 for White, 1 for Black) for array indexing.
    #[inline(always)]
    pub fn index(self) -> usize {
        self as usize
    }

    /// Mirrors a square for perspective — Black sees the board from the opposite side.
    #[inline(always)]
    pub fn rel_sq(self, sq: i32) -> i32 {
        if self == WC { sq } else { sq ^ 56 }
    }
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Color::White => write!(f, "w"),
            Color::Black => write!(f, "b"),
        }
    }
}

// ============================================================================
// PieceType
// ============================================================================

/// Piece type without color information.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum PieceType {
    Pawn = 0,
    Knight = 1,
    Bishop = 2,
    Rook = 3,
    Queen = 4,
    King = 5,
    #[default]
    None = 6,
}

pub const P: PieceType = PieceType::Pawn;
pub const N: PieceType = PieceType::Knight;
pub const B: PieceType = PieceType::Bishop;
pub const R: PieceType = PieceType::Rook;
pub const Q: PieceType = PieceType::Queen;
pub const K: PieceType = PieceType::King;
pub const NO_TP: PieceType = PieceType::None;

impl PieceType {
    /// Returns the index (0..=6) for array indexing.
    #[inline(always)]
    pub fn index(self) -> usize {
        self as usize
    }

    /// Constructs a `PieceType` from a raw index.
    #[inline(always)]
    pub fn from_index(idx: usize) -> PieceType {
        debug_assert!(idx <= 6);
        // SAFETY: repr(u8) enum values 0..=6 match all valid indices.
        unsafe { std::mem::transmute(idx as u8) }
    }
}

// ============================================================================
// Piece
// ============================================================================

/// Combined color + piece type, encoded as `(type << 1) | color`.
///
/// WP=0, BP=1, WN=2, BN=3, WB=4, BB=5, WR=6, BR=7, WQ=8, BQ=9, WK=10, BK=11, None=12.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Piece {
    WP = 0,
    BP = 1,
    WN = 2,
    BN = 3,
    WB = 4,
    BB = 5,
    WR = 6,
    BR = 7,
    WQ = 8,
    BQ = 9,
    WK = 10,
    BK = 11,
    None = 12,
}

/// Sentinel for an empty square.
pub const NO_PC: Piece = Piece::None;

impl Piece {
    /// Construct a piece from color and type: piece = (type << 1) | color
    #[inline(always)]
    pub fn new(color: Color, tp: PieceType) -> Piece {
        debug_assert!(tp != NO_TP);
        Piece::from_index(((tp as u8) << 1) | (color as u8))
    }

    /// Constructs a `Piece` from a raw index.
    #[inline(always)]
    pub fn from_index(idx: u8) -> Piece {
        debug_assert!(idx <= 12);
        // SAFETY: repr(u8) enum values 0..=12 match all valid indices.
        unsafe { std::mem::transmute(idx) }
    }

    /// Extract color: piece & 1
    #[inline(always)]
    pub fn color(self) -> Color {
        if (self as u8) & 1 == 0 {
            Color::White
        } else {
            Color::Black
        }
    }

    /// Extract piece type: piece >> 1
    #[inline(always)]
    pub fn piece_type(self) -> PieceType {
        PieceType::from_index((self as u8 >> 1) as usize)
    }

    /// Returns the raw index (0..=12) for array indexing.
    #[inline(always)]
    pub fn index(self) -> usize {
        self as usize
    }
}

// ============================================================================
// Square
// ============================================================================

/// Square index: 0 = A1, 7 = H1, 56 = A8, 63 = H8, 64 = NO_SQ.
pub type Square = i32;

/// Named square constants for the 64-square board.
pub const A1: Square = 0;
pub const B1: Square = 1;
pub const C1: Square = 2;
pub const D1: Square = 3;
pub const E1: Square = 4;
pub const F1: Square = 5;
pub const G1: Square = 6;
pub const H1: Square = 7;
pub const A2: Square = 8;
pub const B2: Square = 9;
pub const C2: Square = 10;
pub const D2: Square = 11;
pub const E2: Square = 12;
pub const F2: Square = 13;
pub const G2: Square = 14;
pub const H2: Square = 15;
#[allow(dead_code)]
pub const A3: Square = 16;
pub const B3: Square = 17;
pub const C3: Square = 18;
pub const D3: Square = 19;
pub const E3: Square = 20;
pub const F3: Square = 21;
pub const G3: Square = 22;
#[allow(dead_code)]
pub const H3: Square = 23;
pub const A4: Square = 24;
pub const B4: Square = 25;
pub const C4: Square = 26;
pub const D4: Square = 27;
pub const E4: Square = 28;
pub const F4: Square = 29;
pub const G4: Square = 30;
pub const H4: Square = 31;
#[allow(dead_code)]
pub const A5: Square = 32;
pub const B5: Square = 33;
pub const C5: Square = 34;
pub const D5: Square = 35;
pub const E5: Square = 36;
pub const F5: Square = 37;
pub const G5: Square = 38;
pub const H5: Square = 39;
pub const A6: Square = 40;
pub const B6: Square = 41;
pub const C6: Square = 42;
pub const D6: Square = 43;
pub const E6: Square = 44;
pub const F6: Square = 45;
pub const G6: Square = 46;
pub const H6: Square = 47;
pub const A7: Square = 48;
pub const B7: Square = 49;
pub const C7: Square = 50;
#[allow(dead_code)]
pub const D7: Square = 51;
#[allow(dead_code)]
pub const E7: Square = 52;
pub const F7: Square = 53;
pub const G7: Square = 54;
pub const H7: Square = 55;
pub const A8: Square = 56;
pub const B8: Square = 57;
pub const C8: Square = 58;
pub const D8: Square = 59;
pub const E8: Square = 60;
pub const F8: Square = 61;
pub const G8: Square = 62;
pub const H8: Square = 63;
pub const NO_SQ: Square = 64;

/// Returns the file (0–7) of a square.
#[inline(always)]
pub fn file_of(sq: Square) -> i32 {
    sq & 7
}

/// Returns the rank (0–7) of a square.
#[inline(always)]
pub fn rank_of(sq: Square) -> i32 {
    sq >> 3
}

/// Constructs a square index from file and rank (both 0–7).
#[inline(always)]
pub fn sq(file: i32, rank: i32) -> Square {
    (rank << 3) | file
}

// ============================================================================
// Castling Rights (bit flags)
// ============================================================================

/// Castling rights represented as a bitmask.
pub type CastlingRights = i32;

/// White kingside castling.
pub const W_KS: CastlingRights = 1;
/// White queenside castling.
pub const W_QS: CastlingRights = 2;
/// Black kingside castling.
pub const B_KS: CastlingRights = 4;
/// Black queenside castling.
pub const B_QS: CastlingRights = 8;
/// All four castling rights combined.
pub const ALL_CASTLING: CastlingRights = W_KS | W_QS | B_KS | B_QS;

// ============================================================================
// Global constants for piece and castling data
// ============================================================================

/// Maximum search depth in plies.
pub const MAX_PLY: usize = 64;
/// Maximum legal moves in any position (generous upper bound).
pub const MAX_MOVES: usize = 256;
/// Infinity score sentinel.
pub const INF: i32 = 32767;
/// Checkmate score baseline.
pub const MATE: i32 = 32000;
/// Scores above this threshold are treated as mate scores.
pub const MAX_EVAL: i32 = 29999;
/// History table saturation threshold.
pub const MAX_HIST: i32 = 1 << 15;
/// Maximum PV line length for display.
pub const MAX_PV: usize = 12;

/// Piece type values for SEE and delta pruning (index by PieceType)
pub const TP_VALUE: [i32; 7] = [100, 325, 325, 500, 1000, 0, 0];

/// Phase values per piece type (used for game phase calculation)
pub const PH_VALUE: [i32; 7] = [0, 1, 1, 2, 4, 0, 0];

/// Castling mask table — compile-time constant.
///
/// `castle_mask(sq)` is ANDed with current castling rights after any move from/to `sq`.
/// Most squares preserve all rights (15). Only rook corners and king squares clear rights.
#[rustfmt::skip]
static CASTLE_MASK_TABLE: [CastlingRights; 64] = {
    let m = ALL_CASTLING; // 15 — preserves all castling rights
    let mut t = [m; 64];
    t[A1 as usize] = W_KS | B_KS | B_QS;  // 1|4|8 = 13 — clears W_QS
    t[E1 as usize] = B_KS | B_QS;          // 4|8 = 12 — clears W_KS | W_QS
    t[H1 as usize] = W_QS | B_KS | B_QS;  // 2|4|8 = 14 — clears W_KS
    t[A8 as usize] = W_KS | W_QS | B_KS;  // 1|2|4 = 7  — clears B_QS
    t[E8 as usize] = W_KS | W_QS;          // 1|2 = 3   — clears B_KS | B_QS
    t[H8 as usize] = W_KS | W_QS | B_QS;  // 1|2|8 = 11 — clears B_KS
    t
};

/// Get the castling mask for a given square.
#[inline(always)]
pub fn castle_mask(sq: i32) -> CastlingRights {
    debug_assert!((0..64).contains(&sq));
    // SAFETY: sq is always a valid square (0-63), enforced by callers.
    unsafe { *CASTLE_MASK_TABLE.get_unchecked(sq as usize) }
}

/// Backward-compatible init (now a no-op since the table is const).
pub fn init_castle_mask() {
    // No-op — CASTLE_MASK_TABLE is computed at compile time.
}

/// Formats a square as algebraic notation (e.g., `"e4"`), or `"-"` for [`NO_SQ`].
pub fn sq_to_string(sq: Square) -> String {
    if sq == NO_SQ {
        return "-".to_string();
    }
    let file = (b'a' + file_of(sq) as u8) as char;
    let rank = (b'1' + rank_of(sq) as u8) as char;
    format!("{file}{rank}")
}

// ============================================================================
// Start position FEN
// ============================================================================

/// FEN string for the standard chess starting position.
pub const START_POS: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq -";

// ============================================================================
// Zobrist
// ============================================================================

/// Side-to-move Zobrist key — XORed into the hash when it is Black's turn.
pub const SIDE_RANDOM: u64 = !0u64;

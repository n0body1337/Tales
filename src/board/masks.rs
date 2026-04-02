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

//! Pre-computed bitmasks — adjacent files, passed pawn masks, and pawn support masks.

use std::sync::OnceLock;

use super::bitboard::*;
use super::types::*;

// ============================================================================
// Static mask tables — initialized once via OnceLock
// ============================================================================

struct MaskTables {
    adjacent: [Bitboard; 8],
    passed: [[Bitboard; 64]; 2],
    supported: [[Bitboard; 64]; 2],
}

static TABLES: OnceLock<Box<MaskTables>> = OnceLock::new();

fn tables() -> &'static MaskTables {
    TABLES.get().expect("masks::init() not called")
}

// ============================================================================
// Const masks
// ============================================================================

/// Own half of the board
pub const HOME: [Bitboard; 2] = [
    Bitboard(RANK_1_BB.0 | RANK_2_BB.0 | RANK_3_BB.0 | RANK_4_BB.0), // white
    Bitboard(RANK_5_BB.0 | RANK_6_BB.0 | RANK_7_BB.0 | RANK_8_BB.0), // black
];

/// Enemy half of the board
pub const AWAY: [Bitboard; 2] = [
    Bitboard(RANK_5_BB.0 | RANK_6_BB.0 | RANK_7_BB.0 | RANK_8_BB.0), // white
    Bitboard(RANK_1_BB.0 | RANK_2_BB.0 | RANK_3_BB.0 | RANK_4_BB.0), // black
];

/// Kingside castling zone
pub const KS_CASTLE: [Bitboard; 2] = [
    Bitboard(0x0000_0000_0000_00F0), // f1-h1 area
    Bitboard(0xF000_0000_0000_0000), // f8-h8 area
];

/// Queenside castling zone
pub const QS_CASTLE: [Bitboard; 2] = [
    Bitboard(0x0000_0000_0000_000F), // a1-d1 area
    Bitboard(0x0F00_0000_0000_0000), // a8-d8 area
];

/// King side / queen side
pub const K_SIDE: Bitboard = Bitboard(FILE_E_BB.0 | FILE_F_BB.0 | FILE_G_BB.0 | FILE_H_BB.0);
pub const Q_SIDE: Bitboard = Bitboard(FILE_A_BB.0 | FILE_B_BB.0 | FILE_C_BB.0 | FILE_D_BB.0);
pub const CENTER: Bitboard = Bitboard(
    (FILE_C_BB.0 | FILE_D_BB.0 | FILE_E_BB.0 | FILE_F_BB.0)
        & (RANK_3_BB.0 | RANK_4_BB.0 | RANK_5_BB.0 | RANK_6_BB.0),
);

// ============================================================================
// Init function
// ============================================================================

/// Initialize mask tables. Called from board::init().
pub fn init() {
    TABLES
        .set(Box::new({
            let mut t = MaskTables {
                adjacent: [Bitboard(0); 8],
                passed: [[Bitboard(0); 64]; 2],
                supported: [[Bitboard(0); 64]; 2],
            };

            // Adjacent files
            for col in 0..8usize {
                t.adjacent[col] = Bitboard(0);
                if col > 0 {
                    t.adjacent[col].0 |= FILE_A_BB.0 << (col - 1);
                }
                if col < 7 {
                    t.adjacent[col].0 |= FILE_A_BB.0 << (col + 1);
                }
            }

            // Supported masks
            for sq in 0..64i32 {
                let bb = Bitboard::from_sq(sq);

                t.supported[WC.index()][sq as usize] = shift_sideways(bb);
                t.supported[WC.index()][sq as usize] = Bitboard(
                    t.supported[WC.index()][sq as usize].0
                        | fill_south(t.supported[WC.index()][sq as usize]).0,
                );

                t.supported[BC.index()][sq as usize] = shift_sideways(bb);
                t.supported[BC.index()][sq as usize] = Bitboard(
                    t.supported[BC.index()][sq as usize].0
                        | fill_north(t.supported[BC.index()][sq as usize]).0,
                );
            }

            // Passed pawn masks
            for sq in 0..64i32 {
                let bb = Bitboard::from_sq(sq);

                t.passed[WC.index()][sq as usize] = fill_north_excl(bb);
                t.passed[WC.index()][sq as usize] = t.passed[WC.index()][sq as usize]
                    | shift_sideways(t.passed[WC.index()][sq as usize]);

                t.passed[BC.index()][sq as usize] = fill_south_excl(bb);
                t.passed[BC.index()][sq as usize] = t.passed[BC.index()][sq as usize]
                    | shift_sideways(t.passed[BC.index()][sq as usize]);
            }

            t
        }))
        .ok();
}

// ============================================================================
// Accessors
// ============================================================================

/// Adjacent files mask for a given file (0-7).
#[inline(always)]
pub fn adjacent(file: i32) -> Bitboard {
    debug_assert!((0..8).contains(&file));
    tables().adjacent[file as usize]
}

/// Passed pawn mask for a pawn of `color` on `sq`.
#[inline(always)]
pub fn passed(color: Color, sq: i32) -> Bitboard {
    debug_assert!((0..64).contains(&sq));
    tables().passed[color.index()][sq as usize]
}

/// Supported mask for a pawn of `color` on `sq`.
#[inline(always)]
pub fn supported(color: Color, sq: i32) -> Bitboard {
    debug_assert!((0..64).contains(&sq));
    tables().supported[color.index()][sq as usize]
}

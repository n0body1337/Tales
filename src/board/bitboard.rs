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

//! Bitboard type — wraps `u64` with chess-specific operations and shift functions.

use std::fmt;
use std::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, BitXor, BitXorAssign, Not, Shl, Shr};
use std::sync::OnceLock;

use super::types::*;

// ============================================================================
// Bitboard newtype
// ============================================================================

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub struct Bitboard(pub u64);

impl Bitboard {
    pub const EMPTY: Bitboard = Bitboard(0);

    #[inline(always)]
    pub fn from_sq(sq: Square) -> Bitboard {
        debug_assert!((0..64).contains(&sq));
        Bitboard(1u64 << sq)
    }

    #[inline(always)]
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    #[inline(always)]
    pub fn is_not_empty(self) -> bool {
        self.0 != 0
    }

    #[inline(always)]
    pub fn more_than_one(self) -> bool {
        (self.0 & self.0.wrapping_sub(1)) != 0
    }

    #[inline(always)]
    pub fn popcount(self) -> i32 {
        self.0.count_ones() as i32
    }

    /// Least significant bit — returns the square index (0..63)
    #[inline(always)]
    pub fn lsb(self) -> Square {
        debug_assert!(self.0 != 0);
        self.0.trailing_zeros() as Square
    }

    /// Least significant bit as a bitboard — isolates the lowest set bit.
    #[inline(always)]
    pub fn lsb_bb(self) -> Bitboard {
        debug_assert!(self.0 != 0);
        Bitboard(self.0 & self.0.wrapping_neg())
    }

    /// Pop (remove and return) the least significant bit
    #[inline(always)]
    pub fn pop_lsb(&mut self) -> Square {
        let sq = self.lsb();
        self.0 &= self.0 - 1;
        sq
    }

    /// Check if a specific square is set
    #[inline(always)]
    pub fn contains(self, sq: Square) -> bool {
        (self.0 & (1u64 << sq)) != 0
    }
}

// ============================================================================
// Operator impls
// ============================================================================

impl BitAnd for Bitboard {
    type Output = Bitboard;
    #[inline(always)]
    fn bitand(self, rhs: Bitboard) -> Bitboard {
        Bitboard(self.0 & rhs.0)
    }
}

impl BitAndAssign for Bitboard {
    #[inline(always)]
    fn bitand_assign(&mut self, rhs: Bitboard) {
        self.0 &= rhs.0;
    }
}

impl BitOr for Bitboard {
    type Output = Bitboard;
    #[inline(always)]
    fn bitor(self, rhs: Bitboard) -> Bitboard {
        Bitboard(self.0 | rhs.0)
    }
}

impl BitOrAssign for Bitboard {
    #[inline(always)]
    fn bitor_assign(&mut self, rhs: Bitboard) {
        self.0 |= rhs.0;
    }
}

impl BitXor for Bitboard {
    type Output = Bitboard;
    #[inline(always)]
    fn bitxor(self, rhs: Bitboard) -> Bitboard {
        Bitboard(self.0 ^ rhs.0)
    }
}

impl BitXorAssign for Bitboard {
    #[inline(always)]
    fn bitxor_assign(&mut self, rhs: Bitboard) {
        self.0 ^= rhs.0;
    }
}

impl Not for Bitboard {
    type Output = Bitboard;
    #[inline(always)]
    fn not(self) -> Bitboard {
        Bitboard(!self.0)
    }
}

impl Shl<i32> for Bitboard {
    type Output = Bitboard;
    #[inline(always)]
    fn shl(self, rhs: i32) -> Bitboard {
        Bitboard(self.0 << rhs)
    }
}

impl Shr<i32> for Bitboard {
    type Output = Bitboard;
    #[inline(always)]
    fn shr(self, rhs: i32) -> Bitboard {
        Bitboard(self.0 >> rhs)
    }
}

impl fmt::Debug for Bitboard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Bitboard(0x{:016X})", self.0)
    }
}

impl fmt::Display for Bitboard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for rank in (0..8).rev() {
            for file in 0..8 {
                let sq = sq(file, rank);
                if self.contains(sq) {
                    write!(f, "X ")?;
                } else {
                    write!(f, ". ")?;
                }
            }
            writeln!(f, " {}", rank + 1)?;
        }
        writeln!(f, "a b c d e f g h")
    }
}

// ============================================================================
// Iterator — yields squares one at a time (destructive)
// ============================================================================

impl Iterator for Bitboard {
    type Item = Square;

    #[inline(always)]
    fn next(&mut self) -> Option<Square> {
        if self.is_empty() {
            None
        } else {
            Some(self.pop_lsb())
        }
    }
}

// ============================================================================
// Rank / File bitboard constants
// ============================================================================

pub const RANK_1_BB: Bitboard = Bitboard(0x0000_0000_0000_00FF);
pub const RANK_2_BB: Bitboard = Bitboard(0x0000_0000_0000_FF00);
pub const RANK_3_BB: Bitboard = Bitboard(0x0000_0000_00FF_0000);
pub const RANK_4_BB: Bitboard = Bitboard(0x0000_0000_FF00_0000);
pub const RANK_5_BB: Bitboard = Bitboard(0x0000_00FF_0000_0000);
pub const RANK_6_BB: Bitboard = Bitboard(0x0000_FF00_0000_0000);
pub const RANK_7_BB: Bitboard = Bitboard(0x00FF_0000_0000_0000);
pub const RANK_8_BB: Bitboard = Bitboard(0xFF00_0000_0000_0000);

pub const FILE_A_BB: Bitboard = Bitboard(0x0101_0101_0101_0101);
pub const FILE_B_BB: Bitboard = Bitboard(0x0202_0202_0202_0202);
pub const FILE_C_BB: Bitboard = Bitboard(0x0404_0404_0404_0404);
pub const FILE_D_BB: Bitboard = Bitboard(0x0808_0808_0808_0808);
pub const FILE_E_BB: Bitboard = Bitboard(0x1010_1010_1010_1010);
pub const FILE_F_BB: Bitboard = Bitboard(0x2020_2020_2020_2020);
pub const FILE_G_BB: Bitboard = Bitboard(0x4040_4040_4040_4040);
pub const FILE_H_BB: Bitboard = Bitboard(0x8080_8080_8080_8080);

pub const NOT_A_FILE: Bitboard = Bitboard(!0x0101_0101_0101_0101);
pub const NOT_H_FILE: Bitboard = Bitboard(!0x8080_8080_8080_8080);

pub const WHITE_SQUARES: Bitboard = Bitboard(0x55AA_55AA_55AA_55AA);
pub const BLACK_SQUARES: Bitboard = Bitboard(0xAA55_AA55_AA55_AA55);

pub const CENTRAL_FILES: Bitboard = Bitboard(
    0x0404_0404_0404_0404 | 0x0808_0808_0808_0808 | 0x1010_1010_1010_1010 | 0x2020_2020_2020_2020,
);

/// Relative rank bitboards (indexed \[color\]\[rank\])
pub const REL_RANK_BB: [[Bitboard; 8]; 2] = [
    [
        RANK_1_BB, RANK_2_BB, RANK_3_BB, RANK_4_BB, RANK_5_BB, RANK_6_BB, RANK_7_BB, RANK_8_BB,
    ],
    [
        RANK_8_BB, RANK_7_BB, RANK_6_BB, RANK_5_BB, RANK_4_BB, RANK_3_BB, RANK_2_BB, RANK_1_BB,
    ],
];

// ============================================================================
// Shift functions
// ============================================================================

#[inline(always)]
pub fn shift_north(bb: Bitboard) -> Bitboard {
    bb << 8
}

#[inline(always)]
pub fn shift_south(bb: Bitboard) -> Bitboard {
    bb >> 8
}

#[inline(always)]
pub fn shift_west(bb: Bitboard) -> Bitboard {
    Bitboard((bb.0 & NOT_A_FILE.0) >> 1)
}

#[inline(always)]
pub fn shift_east(bb: Bitboard) -> Bitboard {
    Bitboard((bb.0 & NOT_H_FILE.0) << 1)
}

#[inline(always)]
pub fn shift_nw(bb: Bitboard) -> Bitboard {
    Bitboard((bb.0 & NOT_A_FILE.0) << 7)
}

#[inline(always)]
pub fn shift_ne(bb: Bitboard) -> Bitboard {
    Bitboard((bb.0 & NOT_H_FILE.0) << 9)
}

#[inline(always)]
pub fn shift_sw(bb: Bitboard) -> Bitboard {
    Bitboard((bb.0 & NOT_A_FILE.0) >> 9)
}

#[inline(always)]
pub fn shift_se(bb: Bitboard) -> Bitboard {
    Bitboard((bb.0 & NOT_H_FILE.0) >> 7)
}

#[inline(always)]
pub fn shift_fwd(bb: Bitboard, color: Color) -> Bitboard {
    match color {
        Color::White => shift_north(bb),
        Color::Black => shift_south(bb),
    }
}

#[inline(always)]
pub fn shift_sideways(bb: Bitboard) -> Bitboard {
    shift_west(bb) | shift_east(bb)
}

// ============================================================================
// Fill functions
// ============================================================================

#[inline(always)]
pub fn fill_north(mut bb: Bitboard) -> Bitboard {
    bb.0 |= bb.0 << 8;
    bb.0 |= bb.0 << 16;
    bb.0 |= bb.0 << 32;
    bb
}

#[inline(always)]
pub fn fill_south(mut bb: Bitboard) -> Bitboard {
    bb.0 |= bb.0 >> 8;
    bb.0 |= bb.0 >> 16;
    bb.0 |= bb.0 >> 32;
    bb
}

#[inline(always)]
pub fn fill_north_excl(bb: Bitboard) -> Bitboard {
    fill_north(shift_north(bb))
}

#[inline(always)]
pub fn fill_south_excl(bb: Bitboard) -> Bitboard {
    fill_south(shift_south(bb))
}

#[inline(always)]
pub fn get_front_span(bb: Bitboard, color: Color) -> Bitboard {
    match color {
        Color::White => fill_north_excl(bb),
        Color::Black => fill_south_excl(bb),
    }
}

// ============================================================================
// Pawn control functions
// ============================================================================

#[inline(always)]
pub fn w_pawn_attacks(bb: Bitboard) -> Bitboard {
    shift_ne(bb) | shift_nw(bb)
}

#[inline(always)]
pub fn b_pawn_attacks(bb: Bitboard) -> Bitboard {
    shift_se(bb) | shift_sw(bb)
}

#[inline(always)]
pub fn pawn_attacks_bb(bb: Bitboard, color: Color) -> Bitboard {
    match color {
        Color::White => w_pawn_attacks(bb),
        Color::Black => b_pawn_attacks(bb),
    }
}

#[inline(always)]
pub fn w_double_pawn_attacks(bb: Bitboard) -> Bitboard {
    shift_ne(bb) & shift_nw(bb)
}

#[inline(always)]
pub fn b_double_pawn_attacks(bb: Bitboard) -> Bitboard {
    shift_se(bb) & shift_sw(bb)
}

// ============================================================================
// Between rays — computed at init, stored in a 64x64 table
// ============================================================================

/// Table of bitboards representing squares between two squares on a line.
/// Computed from the standard Laser/CPW algorithm.
/// Stored as a flattened array with indexing [sq1*64 + sq2].
static BB_BETWEEN: OnceLock<Box<[Bitboard; 64 * 64]>> = OnceLock::new();

/// Get the between-ray bitboard for two squares.
#[inline(always)]
pub fn between(sq1: Square, sq2: Square) -> Bitboard {
    let table = BB_BETWEEN.get().unwrap();
    // SAFETY: indices are valid squares (0-63), so index < 4096.
    unsafe { *table.get_unchecked((sq1 as usize) * 64 + sq2 as usize) }
}

/// Compute between-ray for two squares (from Laser / CPW).
fn compute_between(sq1: i32, sq2: i32) -> Bitboard {
    let m1: u64 = u64::MAX;
    let a2a7: u64 = 0x0001_0101_0101_0100;
    let b2g7: u64 = 0x0040_2010_0804_0200;
    let h1b7: u64 = 0x0002_0408_1020_4080;

    let btwn = (m1 << sq1) ^ (m1 << sq2);
    let file = (sq2 & 7).wrapping_sub(sq1 & 7);
    let rank = ((sq2 | 7).wrapping_sub(sq1)) >> 3;

    let mut line = ((file as u64) & 7).wrapping_sub(1) & a2a7;
    line = line.wrapping_add(2u64.wrapping_mul(((rank as u64) & 7).wrapping_sub(1) >> 58));
    line = line.wrapping_add(((rank.wrapping_sub(file) as u64) & 15).wrapping_sub(1) & b2g7);
    line = line.wrapping_add(((rank.wrapping_add(file) as u64) & 15).wrapping_sub(1) & h1b7);
    line = line.wrapping_mul(btwn & btwn.wrapping_neg());
    Bitboard(line & btwn)
}

/// Initialize between-ray table. Must be called at startup.
pub fn init_between() {
    let mut table = Box::new([Bitboard(0); 64 * 64]);
    for sq1 in 0..64usize {
        for sq2 in 0..64usize {
            table[sq1 * 64 + sq2] = compute_between(sq1 as i32, sq2 as i32);
        }
    }
    BB_BETWEEN.set(table).ok();
}

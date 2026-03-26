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

// Move encoding.
//
// Bits 0-5:   from square
// Bits 6-11:  target square
// Bits 12-15: move type

use super::types::*;
use std::fmt;

// ============================================================================
// Move type flags (bits 12-15)
// ============================================================================

pub const NORMAL: u16 = 0;
pub const CASTLE: u16 = 1;
pub const EP_CAP: u16 = 2;
pub const EP_SET: u16 = 3;
pub const N_PROM: u16 = 4;
pub const B_PROM: u16 = 5;
pub const R_PROM: u16 = 6;
pub const Q_PROM: u16 = 7;

// ============================================================================
// Move newtype (u16)
// ============================================================================

#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Move(pub u16);

impl Move {
    pub const NONE: Move = Move(0);

    /// Construct a move from components
    #[inline(always)]
    pub fn new(from: Square, to: Square, move_type: u16) -> Move {
        Move(((move_type & 0xF) << 12) | ((to as u16 & 63) << 6) | (from as u16 & 63))
    }

    /// Construct a normal move
    #[inline(always)]
    pub fn normal(from: Square, to: Square) -> Move {
        Move(((to as u16 & 63) << 6) | (from as u16 & 63))
    }

    /// From square (bits 0-5)
    #[inline(always)]
    #[allow(clippy::wrong_self_convention)]
    pub fn from_sq(self) -> Square {
        (self.0 & 63) as Square
    }

    /// Target square (bits 6-11)
    #[inline(always)]
    pub fn to_sq(self) -> Square {
        ((self.0 >> 6) & 63) as Square
    }

    /// Move type (bits 12-15)
    #[inline(always)]
    pub fn move_type(self) -> u16 {
        self.0 >> 12
    }

    /// Is this a promotion?
    #[inline(always)]
    pub fn is_prom(self) -> bool {
        (self.0 & 0x4000) != 0
    }

    /// Promotion piece type (valid only if is_prom() is true)
    /// Promotion type: PromType(x) = ((x) >> 12) - 3
    #[inline(always)]
    pub fn prom_type(self) -> PieceType {
        PieceType::from_index(((self.0 >> 12) - 3) as usize)
    }

    /// Is this move empty/null?
    #[inline(always)]
    pub fn is_none(self) -> bool {
        self.0 == 0
    }

    /// Convert to UCI string (e.g., "e2e4", "e7e8q")
    pub fn to_uci_string(self) -> String {
        if self.is_none() {
            return "0000".to_string();
        }
        let from = self.from_sq();
        let to = self.to_sq();
        let mut s = format!(
            "{}{}{}{}",
            (b'a' + file_of(from) as u8) as char,
            (b'1' + rank_of(from) as u8) as char,
            (b'a' + file_of(to) as u8) as char,
            (b'1' + rank_of(to) as u8) as char,
        );
        if self.is_prom() {
            let ch = match self.move_type() {
                N_PROM => 'n',
                B_PROM => 'b',
                R_PROM => 'r',
                Q_PROM => 'q',
                _ => '?',
            };
            s.push(ch);
        }
        s
    }
}

impl fmt::Debug for Move {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Move({})", self.to_uci_string())
    }
}

impl fmt::Display for Move {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_uci_string())
    }
}

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

// Board module — core types, bitboard, position representation

pub mod attacks;
pub mod bitboard;
pub mod distance;
pub mod magic;
pub mod masks;
pub mod moves;
pub mod position;
pub mod types;
pub mod zobrist;

/// Initialize all static board tables. Must be called once at startup.
pub fn init() {
    magic::init();
    attacks::init();
    bitboard::init_between();
    zobrist::init();
    masks::init();
    distance::init();
    types::init_castle_mask();
}

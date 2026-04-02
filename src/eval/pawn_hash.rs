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

//! Pawn hash table — caches pawn structure evaluation results per engine instance.

pub const PAWN_HASH_SIZE: usize = 1 << 16; // 65536 entries
const PAWN_HASH_MASK: usize = PAWN_HASH_SIZE - 1;

#[derive(Clone, Copy, Default)]
pub struct PawnHashEntry {
    pub key: u64,
    pub mg_pawns: i32,
    pub eg_pawns: i32,
}

pub struct PawnHash {
    table: Vec<PawnHashEntry>,
}

impl PawnHash {
    pub fn new() -> Self {
        PawnHash {
            table: vec![PawnHashEntry::default(); PAWN_HASH_SIZE],
        }
    }

    pub fn clear(&mut self) {
        self.table.fill(PawnHashEntry::default());
    }

    /// Try to retrieve cached pawn evaluation. Returns Some((mg, eg)) on hit.
    #[inline]
    pub fn retrieve(&self, pawn_key: u64) -> Option<(i32, i32)> {
        let addr = (pawn_key as usize) & PAWN_HASH_MASK;
        let entry = &self.table[addr];
        if entry.key == pawn_key {
            Some((entry.mg_pawns, entry.eg_pawns))
        } else {
            None
        }
    }

    /// Store pawn evaluation result.
    #[inline]
    pub fn store(&mut self, pawn_key: u64, mg_pawns: i32, eg_pawns: i32) {
        let addr = (pawn_key as usize) & PAWN_HASH_MASK;
        let entry = &mut self.table[addr];
        entry.key = pawn_key;
        entry.mg_pawns = mg_pawns;
        entry.eg_pawns = eg_pawns;
    }
}

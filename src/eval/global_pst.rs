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

//! Global PST tables — initialized once, used by [`Position`](crate::board::position::Position)
//! for incremental PST updates.

use std::sync::OnceLock;

/// Static global PST tables \[color\]\[piece_type\]\[square\]
struct PstTables {
    mg: [[[i32; 64]; 6]; 2],
    eg: [[[i32; 64]; 6]; 2],
}

static TABLES: OnceLock<Box<PstTables>> = OnceLock::new();

/// Initialize from EvalParams (must be called once before any Position operations).
pub fn init(par: &super::params::EvalParams) {
    let t = Box::new(PstTables {
        mg: par.mg_pst,
        eg: par.eg_pst,
    });
    TABLES.set(t).ok();
}

#[inline(always)]
pub fn mg(color_idx: usize, piece_type_idx: usize, sq: usize) -> i32 {
    let t = TABLES.get().unwrap();
    t.mg[color_idx][piece_type_idx][sq]
}

#[inline(always)]
pub fn eg(color_idx: usize, piece_type_idx: usize, sq: usize) -> i32 {
    let t = TABLES.get().unwrap();
    t.eg[color_idx][piece_type_idx][sq]
}

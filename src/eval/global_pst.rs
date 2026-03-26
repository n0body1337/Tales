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

// Global PST tables — initialized once, used by Position for incremental PST updates.
// Global PST accessors — provides Position-level access to mg_pst/eg_pst tables.

/// Static global PST tables [color][piece_type][square]
struct PstTables {
    mg: [[[i32; 64]; 6]; 2],
    eg: [[[i32; 64]; 6]; 2],
}

static mut TABLES: *const PstTables = std::ptr::null();

/// Initialize from EvalParams (must be called once before any Position operations).
pub fn init(par: &super::params::EvalParams) {
    let t = Box::new(PstTables {
        mg: par.mg_pst,
        eg: par.eg_pst,
    });
    unsafe {
        TABLES = Box::into_raw(t);
    }
}

#[inline(always)]
pub fn mg(color_idx: usize, piece_type_idx: usize, sq: usize) -> i32 {
    // SAFETY: init() is called once at startup before any access.
    // All indices are valid: color 0-1, piece_type 0-5, sq 0-63.
    unsafe {
        *(*TABLES)
            .mg
            .get_unchecked(color_idx)
            .get_unchecked(piece_type_idx)
            .get_unchecked(sq)
    }
}

#[inline(always)]
pub fn eg(color_idx: usize, piece_type_idx: usize, sq: usize) -> i32 {
    // SAFETY: init() is called once at startup before any access.
    unsafe {
        *(*TABLES)
            .eg
            .get_unchecked(color_idx)
            .get_unchecked(piece_type_idx)
            .get_unchecked(sq)
    }
}

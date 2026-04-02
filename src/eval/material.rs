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

//! Material evaluation — piece counting, imbalance adjustments, and material balance.

use super::eval_data::EvalData;
use super::params::EvalParams;
use crate::board::position::Position;
use crate::board::types::*;

/// Evaluate material adjustments — closed position knight bonus, open position rook
/// bonus, pair bonuses, and elephantiasis correction.
pub fn evaluate_material(p: &Position, e: &mut EvalData, par: &EvalParams, sd: Color) {
    let op = !sd;

    let mut tmp = par.np_table[p.count(sd, P) as usize] * p.count(sd, N)
        - par.rp_table[p.count(sd, P) as usize] * p.count(sd, R);

    if p.count(sd, N) > 1 {
        tmp += par.n_pair;
    }
    if p.count(sd, R) > 1 {
        tmp += par.r_pair;
    }
    if p.count(sd, B) > 1 {
        tmp += par.b_pair;
    }

    // Elephantiasis correction for queen
    if p.count(sd, Q) > 0 {
        tmp -= par.eleph * (p.count(op, N) + p.count(op, B));
    }

    e.add_both(sd, tmp);
}

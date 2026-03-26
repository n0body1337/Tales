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

// Material evaluation — piece counting, imbalance, and adjustments.

use super::eval_data::{self, EvalData};
use super::params::EvalParams;
use crate::board::position::Position;
use crate::board::types::*;

pub fn evaluate_material(p: &Position, e: &mut EvalData, par: &EvalParams, sd: Color) {
    let op = !sd;
    let si = sd.index();
    let oi = op.index();

    let mut tmp = par.np_table[p.cnt[si][P.index()] as usize] * p.cnt[si][N.index()]
        - par.rp_table[p.cnt[si][P.index()] as usize] * p.cnt[si][R.index()];

    if p.cnt[si][N.index()] > 1 {
        tmp += par.n_pair;
    }
    if p.cnt[si][R.index()] > 1 {
        tmp += par.r_pair;
    }
    if p.cnt[si][B.index()] > 1 {
        tmp += par.b_pair;
    }

    // Elephantiasis correction for queen
    if p.cnt[si][Q.index()] > 0 {
        tmp -= par.eleph * (p.cnt[oi][N.index()] + p.cnt[oi][B.index()]);
    }

    eval_data::add_both(e, sd, tmp);
}

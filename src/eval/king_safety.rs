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

//! King safety evaluation — converts accumulated attack data into a non-linear danger score.

use super::eval_data::EvalData;
use super::params::EvalParams;
use crate::board::position::Position;
use crate::board::types::*;

pub fn evaluate_king_attack(p: &Position, e: &mut EvalData, par: &EvalParams, sd: Color) {
    let si = sd.index();

    if e.wood[si] > 1 {
        // Zero attack score without a queen, otherwise cap at 399
        e.att[si] = if p.queens(sd).is_empty() {
            0
        } else {
            e.att[si].min(399)
        };

        // Look up danger table and apply side-dependent attack weight
        let king_danger = (par.danger[e.att[si] as usize] * par.sd_att[si]) / 100;
        e.add_both(sd, king_danger);
    }
}

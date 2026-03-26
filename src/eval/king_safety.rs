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

// King safety evaluation — evaluates king attack scores using a non-linear danger table.

use super::eval_data::{self, EvalData};
use super::params::EvalParams;
use crate::board::position::Position;
use crate::board::types::*;

pub fn evaluate_king_attack(p: &Position, e: &mut EvalData, par: &EvalParams, sd: Color) {
    let si = sd.index();

    if e.wood[si] > 1 {
        // Cap attack score at 399
        if e.att[si] > 399 {
            e.att[si] = 399;
        }

        // No king attack without a queen — zero att completely
        if p.queens(sd).is_empty() {
            e.att[si] = 0;
        }

        // Look up danger table and apply side-dependent attack weight
        let king_danger = (par.danger[e.att[si] as usize] * par.sd_att[si]) / 100;

        // Add equally to mg and eg, matching Add(e, sd, val)
        eval_data::add_both(e, sd, king_danger);
    }
}

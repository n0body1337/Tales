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

    if e.wood[si] >= par.att_min_wood {
        // Index the non-linear danger table (capped at 399, table is 512 wide).
        let att = e.att[si].clamp(0, 399) as usize;
        let base = par.danger[att] * par.sd_att[si];

        // Rodent zeroed king danger entirely without a queen — which rejects
        // exactly the queen sacrifices a Tal-style engine wants to find. Retain
        // a tunable fraction instead: a rook/bishop/knight-led attack after a
        // queen sac still carries real danger.
        let king_danger = if p.queens(sd).is_empty() {
            (base * par.no_queen_att_pct) / (100 * 100)
        } else {
            base / 100
        };
        e.att[si] = att as i32;
        e.add_both(sd, king_danger);
    }
}

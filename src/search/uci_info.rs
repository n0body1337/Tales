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

//! UCI info output helpers — score formatting and PV line printing.

use crate::board::moves::Move;
use crate::board::types::{MATE, MAX_EVAL};

/// Format a score for UCI output (centipawns or mate-in-N).
pub fn format_score(score: i32) -> String {
    if score > MAX_EVAL {
        format!("score mate {}", (MATE - score + 1) / 2)
    } else if score < -MAX_EVAL {
        format!("score mate {}", -(MATE + score + 1) / 2)
    } else {
        format!("score cp {score}")
    }
}

/// Print the PV portion of a UCI info line (space-separated moves).
pub fn print_pv(pv: &[Move], max_len: usize) {
    for mv in pv.iter().take(max_len).take_while(|m| m.is_some()) {
        print!(" {}", mv.to_uci_string());
    }
    println!();
}

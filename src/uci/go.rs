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

//! `go` command handler — parses time control, probes opening book, and launches search.

use super::EngineState;
use crate::board::position::Position;
use crate::board::types::*;
use crate::search;

/// Parse a "go" command and launch the search (or return a book move).
pub fn parse_go(pos: &mut Position, state: &mut EngineState, tokens: &[&str]) {
    let mut wtime: i64 = -1;
    let mut btime: i64 = -1;
    let mut winc: i64 = 0;
    let mut binc: i64 = 0;
    let mut movestogo: i64 = 40;
    let mut max_depth: i32 = 64;
    let mut movetime: i64 = -1;
    let mut nodes_limit: u64 = 0;
    let mut is_infinite = false;
    let mut is_ponder = false;

    let mut iter = tokens.iter();
    while let Some(&tok) = iter.next() {
        match tok {
            "depth" => max_depth = iter.next().and_then(|t| t.parse().ok()).unwrap_or(64),
            "movetime" => movetime = iter.next().and_then(|t| t.parse().ok()).unwrap_or(5000),
            "nodes" => nodes_limit = iter.next().and_then(|t| t.parse().ok()).unwrap_or(0),
            "wtime" => wtime = iter.next().and_then(|t| t.parse().ok()).unwrap_or(-1),
            "btime" => btime = iter.next().and_then(|t| t.parse().ok()).unwrap_or(-1),
            "winc" => winc = iter.next().and_then(|t| t.parse().ok()).unwrap_or(0),
            "binc" => binc = iter.next().and_then(|t| t.parse().ok()).unwrap_or(0),
            "movestogo" => movestogo = iter.next().and_then(|t| t.parse().ok()).unwrap_or(40),
            "infinite" => {
                is_infinite = true;
            }
            "ponder" => {
                is_ponder = true;
            }
            _ => {}
        }
    }

    // Only honor 'go ponder' when the UCI Ponder option is enabled
    let is_ponder = is_ponder && state.ponder_enabled;

    // Calculate time_limit_ms for Searcher
    let time_limit_ms;
    if movetime > 0 {
        time_limit_ms = movetime as u64;
    } else if wtime >= 0 || btime >= 0 {
        let base = if pos.side == WC { wtime } else { btime };
        let inc = if pos.side == WC { winc } else { binc };
        time_limit_ms = super::time_mgmt::compute_move_time(
            base,
            inc,
            movestogo,
            state.time_buffer,
            state.par.time_percentage,
        );
    } else {
        time_limit_ms = 999_999_999;
    }

    // Set asymmetric parameters based on engine side
    state.par.init_asymmetric(pos.side);

    // Probe the opening book.
    // When UseBook is true and an external book is loaded, use it exclusively.
    // When UseBook is true but no external book could be loaded, fall back to internal.
    // When UseBook is false, always use the internal embedded book.
    if !is_ponder {
        let book_mv = if state.use_book {
            if let Some(ref ext) = state.external_book {
                ext.probe(pos, state.verbose_book, state.book_filter)
            } else {
                crate::book::internal::probe(pos, state.verbose_book, state.book_filter)
            }
        } else {
            crate::book::internal::probe(pos, state.verbose_book, state.book_filter)
        };
        if let Some(mv) = book_mv {
            println!("bestmove {mv}");
            return;
        }
    }

    // Route through thread pool (handles both single and multi-threaded)
    let cfg = search::threads::SmpConfig {
        num_threads: state.num_threads,
        max_depth,
        time_limit_ms: if is_infinite || is_ponder {
            999_999_999
        } else {
            time_limit_ms
        },
        move_overhead_ms: state.searcher.move_overhead_ms,
        game_key: state.searcher.game_key,
        nodes_limit,
        nps_limit: state.par.nps_limit,
        multi_pv: state.multi_pv,
        is_pondering: is_ponder,
        ponder_time_ms: time_limit_ms,
        ponder_enabled: state.ponder_enabled,
    };
    search::threads::lazy_smp_search(
        pos,
        &mut state.tt,
        &state.par,
        &mut state.eval_hash,
        &mut state.pawn_tt,
        &cfg,
    );
}

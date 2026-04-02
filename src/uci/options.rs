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

//! UCI `setoption` handler — maps UCI option names to engine state mutations.

use super::EngineState;
use crate::tt::TransTable;

/// Parse and apply a `setoption name <NAME> value <VALUE>` command.
pub fn parse_setoption(tokens: &[&str], state: &mut EngineState) {
    // Expect: ["name", "<name>", ...] or ["name", "<name>", "value", "<value>"]
    if tokens.len() < 2 {
        return;
    }
    if tokens[0] != "name" {
        return;
    }

    // Build the option name (can be multi-word, e.g. "Clear Hash")
    let value_pos = tokens.iter().position(|&t| t.to_lowercase() == "value");
    let name_end = value_pos.unwrap_or(tokens.len());
    let name = tokens[1..name_end].join(" ").to_lowercase();

    // Get value (if present)
    let value = if let Some(vi) = value_pos {
        if vi + 1 < tokens.len() {
            Some(tokens[vi + 1])
        } else {
            None
        }
    } else {
        None
    };

    match name.as_str() {
        "hash" => {
            if let Some(v) = value
                && let Ok(mb) = v.parse::<usize>()
            {
                let mb = mb.clamp(1, 33_554_432);
                state.tt = TransTable::new(mb);
            }
        }
        "moveoverhead" => {
            if let Some(v) = value
                && let Ok(val) = v.parse::<u64>()
            {
                state.searcher.move_overhead_ms = val.min(5000);
            }
        }
        "threads" => {
            if let Some(v) = value
                && let Ok(val) = v.parse::<usize>()
            {
                state.num_threads = val.clamp(1, 1024);
            }
        }
        "multipv" => {
            if let Some(v) = value
                && let Ok(val) = v.parse::<usize>()
            {
                state.multi_pv = val.clamp(1, 64);
            }
        }
        "clear hash" => {
            state.tt.clear();
        }
        "ponder" => {
            if let Some(v) = value {
                state.ponder_enabled = v == "true";
            }
        }
        "usebook" => {
            if let Some(v) = value {
                state.use_book = v == "true";
                // When enabling the book, try to load the external file
                if state.use_book && state.external_book.is_none() {
                    state.try_load_external_book();
                }
                if !state.use_book {
                    state.external_book = None;
                }
            }
        }
        "verbosebook" => {
            if let Some(v) = value {
                state.verbose_book = v == "true";
            }
        }
        "bookfilter" => {
            if let Some(v) = value
                && let Ok(val) = v.parse::<i32>()
            {
                state.book_filter = val.clamp(0, 100);
            }
        }
        "mainbookfile" => {
            if let Some(v) = value {
                state.book_file = v.to_string();
                // Reload the external book if UseBook is active
                if state.use_book {
                    state.try_load_external_book();
                }
            }
        }
        "timebuffer" => {
            if let Some(v) = value
                && let Ok(val) = v.parse::<i64>()
            {
                state.time_buffer = val.clamp(0, 1000);
            }
        }
        "contempt" => {
            if let Some(v) = value
                && let Ok(val) = v.parse::<i32>()
            {
                state.par.draw_score = val.clamp(-100, 100);
            }
        }
        "evalblur" => {
            if let Some(v) = value
                && let Ok(val) = v.parse::<i32>()
            {
                state.par.eval_blur = val.clamp(0, 40);
            }
        }
        "npslimit" => {
            if let Some(v) = value
                && let Ok(val) = v.parse::<i32>()
            {
                state.par.nps_limit = val.clamp(0, 1_000_000);
            }
        }
        "uci_elo" => {
            if let Some(v) = value
                && let Ok(val) = v.parse::<i32>()
            {
                state.par.elo = val.clamp(800, 2800);
                state.par.set_speed();
            }
        }
        "uci_limitstrength" => {
            if let Some(v) = value {
                state.par.is_weakening = v == "true";
                state.par.set_speed();
            }
        }
        "slowmover" => {
            if let Some(v) = value
                && let Ok(val) = v.parse::<i32>()
            {
                state.par.time_percentage = val.clamp(10, 200);
            }
        }
        "selectivity" => {
            if let Some(v) = value
                && let Ok(val) = v.parse::<i32>()
            {
                state.par.hist_perc = val.clamp(100, 500);
                // hist_limit = -MAX_HIST + ((MAX_HIST * hist_perc) / 100)
                const MAX_HIST: i32 = 1 << 15; // 32768
                state.par.hist_limit = -MAX_HIST + ((MAX_HIST * state.par.hist_perc) / 100);
            }
        }
        _ => {}
    }
}

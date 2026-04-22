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

//! UCI protocol implementation — modular decomposition.
//!
//! Sub-modules:
//! - `go`        — "go" command (time parsing, book probing, search launch)
//! - `options`   — "setoption" handler
//! - `time_mgmt` — time allocation logic
//! - `bench`     — bench command (standard positions)

mod bench;
pub mod epd;
mod go;
mod options;
mod time_mgmt;

use crate::board::moves::Move;
use crate::board::position::Position;
use crate::board::types::*;
use crate::eval;
use crate::search;
use crate::tt::TransTable;
use std::io;

const ENGINE_NAME: &str = concat!("Tales ", env!("CARGO_PKG_VERSION"));
const ENGINE_AUTHOR: &str = "Andre MARTINS";

// ============================================================================
// Engine-wide mutable state managed by the UCI loop
// ============================================================================

pub(crate) struct EngineState {
    pub par: eval::params::EvalParams,
    pub tt: TransTable,
    pub eval_hash: Vec<eval::EvalHashEntry>,
    pub pawn_tt: eval::pawn_hash::PawnHash,
    pub searcher: search::ordering::Searcher,
    pub num_threads: usize,
    // UCI options
    pub multi_pv: usize,
    pub use_book: bool,
    pub verbose_book: bool,
    pub book_filter: i32,
    pub book_file: String,
    pub ponder_enabled: bool,
    pub time_buffer: i64,
    /// Loaded external polyglot book (when `UseBook` is true and file is valid).
    pub external_book: Option<crate::book::external::ExternalBook>,
}

impl EngineState {
    fn new() -> Self {
        let par = eval::params::EvalParams::new();
        eval::global_pst::init(&par);
        EngineState {
            par,
            tt: TransTable::new(16),
            eval_hash: eval::new_eval_hash(),
            pawn_tt: eval::pawn_hash::PawnHash::new(),
            searcher: search::ordering::Searcher::new(),
            num_threads: 1,
            multi_pv: 1,
            use_book: false,
            verbose_book: false,
            book_filter: 20,
            book_file: String::from("book.bin"),
            ponder_enabled: false,
            time_buffer: 50,
            external_book: None,
        }
    }

    /// Try to load the external book from `self.book_file`.
    /// On success, stores it in `self.external_book`.
    /// On failure, prints an error and clears `self.external_book`.
    pub fn try_load_external_book(&mut self) {
        match crate::book::external::ExternalBook::load(&self.book_file) {
            Ok(book) => {
                println!(
                    "info string loaded external book '{}' ({} entries)",
                    self.book_file,
                    book.entry_count()
                );
                self.external_book = Some(book);
            }
            Err(e) => {
                println!("info string error loading external book: {e} — using internal book");
                self.external_book = None;
            }
        }
    }
}

// ============================================================================
// Main UCI loop
// ============================================================================

/// Top-level UCI protocol loop — reads commands from stdin and dispatches.
pub fn uci_loop() {
    // EngineState::new() initializes global PST tables — must happen before any Position operations
    let mut state = EngineState::new();

    let mut pos = Position::new();
    pos.set_position(START_POS);

    // Read one line at a time (don't hold stdin lock across parse_go — needed
    // for check_timeout() stdin polling during search to work without deadlock).
    loop {
        let mut line = String::new();
        if io::stdin().read_line(&mut line).is_err() || line.is_empty() {
            break; // EOF or error
        }
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }

        let mut tokens = line.split_whitespace();
        let Some(cmd) = tokens.next() else {
            continue;
        };

        match cmd {
            "uci" => {
                println!("id name {ENGINE_NAME}");
                println!("id author {ENGINE_AUTHOR}");
                println!();
                println!("option name Hash type spin default 16 min 1 max 33554432");
                println!("option name Threads type spin default 1 min 1 max 1024");
                println!("option name MoveOverhead type spin default 50 min 0 max 5000");
                println!("option name MultiPV type spin default 1 min 1 max 64");
                println!("option name Clear Hash type button");
                println!("option name Ponder type check default false");
                println!("option name UseBook type check default false");
                println!("option name VerboseBook type check default false");
                println!("option name BookFilter type spin default 20 min 0 max 100");
                println!("option name MainBookFile type string default book.bin");
                println!("option name TimeBuffer type spin default 50 min 0 max 1000");
                println!("option name Contempt type spin default 0 min -100 max 100");
                println!("option name EvalBlur type spin default 0 min 0 max 40");
                println!("option name NpsLimit type spin default 0 min 0 max 1000000");
                println!("option name UCI_Elo type spin default 2800 min 800 max 2800");
                println!("option name UCI_LimitStrength type check default false");
                println!("option name SlowMover type spin default 100 min 10 max 200");
                println!("option name Selectivity type spin default 175 min 100 max 500");
                println!("uciok");
            }

            "isready" => {
                println!("readyok");
            }

            "ucinewgame" => {
                state.tt.clear();
                state.searcher.clear_all();
                state
                    .eval_hash
                    .fill(eval::EvalHashEntry { key: 0, score: 0 });
                state.pawn_tt.clear();
                pos.set_position(START_POS);
                // Generate new game_key for eval_blur randomness
                state.searcher.game_key = pos.hash_key
                    ^ (std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_nanos() as u64);
            }

            "setoption" => {
                let rest: Vec<&str> = tokens.collect();
                options::parse_setoption(&rest, &mut state);
            }

            "position" => {
                let rest: Vec<&str> = tokens.collect();
                parse_position(&mut pos, &rest);
            }

            "go" => {
                let rest: Vec<&str> = tokens.collect();
                go::parse_go(&mut pos, &mut state, &rest);
            }

            "ponderhit" => {
                // Ponderhit is handled inside check_timeout()'s stdin polling
                // during search. The searcher transitions from ponder to normal
                // search by clearing is_pondering and applying the real time limit.
                // This handler exists as a fallback if ponderhit arrives between
                // searches (should not happen in normal UCI usage).
                state.searcher.is_pondering = false;
            }

            "stop" => {
                state.searcher.abort_search = true;
            }

            "bench" => {
                let depth: i32 = tokens.next().and_then(|t| t.parse().ok()).unwrap_or(8);
                bench::run_bench(&mut pos, &mut state, depth);
            }

            "print" | "d" => {
                pos.print_board();
            }

            "step" | "stepp" => {
                // Apply moves (stepp also prints the board)
                for token in tokens {
                    let mv = pos.str_to_move(token);
                    if mv != Move::NONE && pos.legal(mv) {
                        let mut u = crate::board::position::Undo::new();
                        pos.do_move(mv, &mut u);
                        if pos.rev_moves == 0 {
                            pos.head = 0;
                        }
                    }
                }
                if cmd == "stepp" {
                    pos.print_board();
                }
            }

            "quit" | "exit" => {
                return;
            }

            _ => {
                // Unknown command, ignore
            }
        }
    }
}

// ============================================================================
// Parse "position [startpos|fen <fen>] [moves <move1> <move2> ...]"
// ============================================================================

fn parse_position(pos: &mut Position, tokens: &[&str]) {
    if tokens.is_empty() {
        return;
    }

    let mut idx = 0;

    if tokens[0] == "startpos" {
        pos.set_position(START_POS);
        idx = 1;
    } else if tokens[0] == "fen" {
        // Collect FEN tokens until "moves" or end
        let mut fen_parts = Vec::new();
        idx = 1;
        while idx < tokens.len() && tokens[idx] != "moves" {
            fen_parts.push(tokens[idx]);
            idx += 1;
        }
        let fen = fen_parts.join(" ");
        pos.set_position(&fen);
    }

    // Parse moves
    if idx < tokens.len() && tokens[idx] == "moves" {
        idx += 1;
        while idx < tokens.len() {
            let mv = pos.str_to_move(tokens[idx]);
            if mv != Move::NONE && pos.legal(mv) {
                let mut u = crate::board::position::Undo::new();
                pos.do_move(mv, &mut u);
                // After an irreversible move, reset rep list head
                if pos.rev_moves == 0 {
                    pos.head = 0;
                }
            }
            idx += 1;
        }
    }
}

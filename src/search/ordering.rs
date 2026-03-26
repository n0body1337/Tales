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

// Move ordering.
// Staged move picker + history/killer/refutation heuristics.

use crate::board::moves::*;
use crate::board::position::Position;
use crate::board::types::*;
use crate::movegen::gen;
use crate::movegen::movelist::MoveList;
use crate::movegen::see;

// Move type flags returned by the picker
pub const MV_HASH: i32 = 0;
pub const MV_CAPTURE: i32 = 1;
pub const MV_KILLER: i32 = 2;
pub const MV_NORMAL: i32 = 3;
pub const MV_BADCAPT: i32 = 4;

// ============================================================================
// MovePicker — 9-phase staged move generation
// ============================================================================

pub struct MovePicker {
    phase: i32,
    trans_move: Move,
    ref_move: Move,
    ref_sq: i32,
    killer1: Move,
    killer2: Move,
    list: MoveList,
    next_idx: usize,
    bad: [Move; 32],
    bad_count: usize,
    bad_next: usize,
}

impl MovePicker {
    pub fn new(
        trans_move: Move,
        ref_move: Move,
        ref_sq: i32,
        killer1: Move,
        killer2: Move,
    ) -> Self {
        MovePicker {
            phase: 0,
            trans_move,
            ref_move,
            ref_sq,
            killer1,
            killer2,
            list: MoveList::new(),
            next_idx: 0,
            bad: [Move::NONE; 32],
            bad_count: 0,
            bad_next: 0,
        }
    }

    /// NextMove — main staged move picker (for search). Returns (move, flag) or (NONE, 0).
    pub fn next_move(&mut self, pos: &Position, history: &[[i32; 64]; 13]) -> (Move, i32) {
        loop {
            match self.phase {
                0 => {
                    // Phase 0: hash move
                    let mv = self.trans_move;
                    self.phase = 1;
                    if !mv.is_none() && pos.legal(mv) {
                        return (mv, MV_HASH);
                    }
                }
                1 => {
                    // Phase 1: generate captures
                    self.list.clear();
                    gen::generate_captures(pos, &mut self.list);
                    score_captures(pos, &mut self.list);
                    self.next_idx = 0;
                    self.bad_count = 0;
                    self.phase = 2;
                }
                2 => {
                    // Phase 2: return good captures, defer bad ones
                    while self.next_idx < self.list.count {
                        let mv = self.list.best_move(self.next_idx);
                        self.next_idx += 1;
                        if mv == self.trans_move {
                            continue;
                        }
                        if bad_capture(pos, mv) {
                            if self.bad_count < self.bad.len() {
                                self.bad[self.bad_count] = mv;
                                self.bad_count += 1;
                            }
                            continue;
                        }
                        return (mv, MV_CAPTURE);
                    }
                    self.phase = 3;
                }
                3 => {
                    // Phase 3: killer 1
                    let mv = self.killer1;
                    self.phase = 4;
                    if !mv.is_none()
                        && mv != self.trans_move
                        && pos.pc[mv.to_sq() as usize] == NO_PC
                        && pos.legal(mv)
                    {
                        return (mv, MV_KILLER);
                    }
                }
                4 => {
                    // Phase 4: killer 2
                    let mv = self.killer2;
                    self.phase = 5;
                    if !mv.is_none()
                        && mv != self.trans_move
                        && pos.pc[mv.to_sq() as usize] == NO_PC
                        && pos.legal(mv)
                    {
                        return (mv, MV_KILLER);
                    }
                }
                5 => {
                    // Phase 5: refutation move
                    let mv = self.ref_move;
                    self.phase = 6;
                    if !mv.is_none()
                        && mv != self.trans_move
                        && pos.pc[mv.to_sq() as usize] == NO_PC
                        && mv != self.killer1
                        && mv != self.killer2
                        && pos.legal(mv)
                    {
                        return (mv, MV_NORMAL);
                    }
                }
                6 => {
                    // Phase 6: generate quiet moves
                    self.list.clear();
                    gen::generate_quiet(pos, &mut self.list);
                    score_quiet(pos, &mut self.list, history, self.ref_sq);
                    self.next_idx = 0;
                    self.phase = 7;
                }
                7 => {
                    // Phase 7: return quiet moves
                    while self.next_idx < self.list.count {
                        let mv = self.list.best_move(self.next_idx);
                        self.next_idx += 1;
                        if mv == self.trans_move
                            || mv == self.killer1
                            || mv == self.killer2
                            || mv == self.ref_move
                        {
                            continue;
                        }
                        return (mv, MV_NORMAL);
                    }
                    self.bad_next = 0;
                    self.phase = 8;
                }
                8 => {
                    // Phase 8: return bad captures
                    if self.bad_next < self.bad_count {
                        let mv = self.bad[self.bad_next];
                        self.bad_next += 1;
                        return (mv, MV_BADCAPT);
                    }
                    return (Move::NONE, 0);
                }
                _ => return (Move::NONE, 0),
            }
        }
    }
}

// ============================================================================
// NextSpecialMove — for QuiesceChecks (captures + killers + checking moves)
// ============================================================================

pub struct SpecialPicker {
    phase: i32,
    trans_move: Move,
    killer1: Move,
    killer2: Move,
    list: MoveList,
    next_idx: usize,
}

impl SpecialPicker {
    pub fn new(trans_move: Move, killer1: Move, killer2: Move) -> Self {
        SpecialPicker {
            phase: 0,
            trans_move,
            killer1,
            killer2,
            list: MoveList::new(),
            next_idx: 0,
        }
    }

    pub fn next_move(&mut self, pos: &Position, history: &[[i32; 64]; 13]) -> (Move, i32) {
        loop {
            match self.phase {
                0 => {
                    let mv = self.trans_move;
                    self.phase = 1;
                    if !mv.is_none() && pos.legal(mv) {
                        return (mv, MV_HASH);
                    }
                }
                1 => {
                    self.list.clear();
                    gen::generate_captures(pos, &mut self.list);
                    score_captures(pos, &mut self.list);
                    self.next_idx = 0;
                    self.phase = 2;
                }
                2 => {
                    while self.next_idx < self.list.count {
                        let mv = self.list.best_move(self.next_idx);
                        self.next_idx += 1;
                        if mv == self.trans_move {
                            continue;
                        }
                        if bad_capture(pos, mv) {
                            continue;
                        }
                        return (mv, MV_CAPTURE);
                    }
                    self.phase = 3;
                }
                3 => {
                    let mv = self.killer1;
                    self.phase = 4;
                    if !mv.is_none()
                        && mv != self.trans_move
                        && pos.pc[mv.to_sq() as usize] == NO_PC
                        && pos.legal(mv)
                    {
                        return (mv, MV_KILLER);
                    }
                }
                4 => {
                    let mv = self.killer2;
                    self.phase = 5;
                    if !mv.is_none()
                        && mv != self.trans_move
                        && pos.pc[mv.to_sq() as usize] == NO_PC
                        && pos.legal(mv)
                    {
                        return (mv, MV_KILLER);
                    }
                }
                5 => {
                    // Generate checking moves
                    self.list.clear();
                    gen::generate_special(pos, &mut self.list);
                    score_quiet(pos, &mut self.list, history, -1);
                    self.next_idx = 0;
                    self.phase = 6;
                }
                6 => {
                    while self.next_idx < self.list.count {
                        let mv = self.list.best_move(self.next_idx);
                        self.next_idx += 1;
                        if mv == self.trans_move || mv == self.killer1 || mv == self.killer2 {
                            continue;
                        }
                        return (mv, MV_NORMAL);
                    }
                    return (Move::NONE, 0);
                }
                _ => return (Move::NONE, 0),
            }
        }
    }
}

// ============================================================================
// CapturesPicker — simplified picker for Quiesce (captures only, MVV-LVA)
// ============================================================================

pub struct CapturesPicker {
    list: MoveList,
    next_idx: usize,
}

impl CapturesPicker {
    pub fn new(pos: &Position) -> Self {
        let mut list = MoveList::new();
        gen::generate_captures(pos, &mut list);
        score_captures(pos, &mut list);
        CapturesPicker { list, next_idx: 0 }
    }

    pub fn next(&mut self) -> Move {
        if self.next_idx < self.list.count {
            let mv = self.list.best_move(self.next_idx);
            self.next_idx += 1;
            mv
        } else {
            Move::NONE
        }
    }
}

// ============================================================================
// Scoring functions
// ============================================================================

#[inline]
fn score_captures(pos: &Position, list: &mut MoveList) {
    for i in 0..list.count {
        // SAFETY: i is bounded by list.count which is always <= MAX_MOVES
        let entry = unsafe { list.moves.get_unchecked_mut(i) };
        entry.score = mvv_lva(pos, entry.mv);
    }
}

#[inline]
fn score_quiet(pos: &Position, list: &mut MoveList, history: &[[i32; 64]; 13], ref_sq: i32) {
    for i in 0..list.count {
        // SAFETY: i is bounded by list.count which is always <= MAX_MOVES
        let entry = unsafe { list.moves.get_unchecked_mut(i) };
        let mv = entry.mv;
        let fsq = mv.from_sq();
        let tsq = mv.to_sq();
        // SAFETY: pc index and square are always valid (piece 0-12, sq 0-63)
        let pc_idx = unsafe { *pos.pc.get_unchecked(fsq as usize) }.index();
        let mut sc = unsafe { *history.get_unchecked(pc_idx).get_unchecked(tsq as usize) };
        if fsq == ref_sq {
            sc += 2048;
        }
        entry.score = sc;
    }
}

/// MVV-LVA scoring
#[inline(always)]
pub fn mvv_lva(pos: &Position, mv: Move) -> i32 {
    let tsq = mv.to_sq();
    let fsq = mv.from_sq();

    // SAFETY: tsq and fsq are valid squares (0-63) derived from Move encoding
    if unsafe { *pos.pc.get_unchecked(tsq as usize) } != NO_PC {
        return pos.tp_on_sq(tsq).index() as i32 * 6 + 5 - pos.tp_on_sq(fsq).index() as i32;
    }

    if mv.is_prom() {
        return mv.prom_type().index() as i32 - 5;
    }

    5
}

/// BadCapture — is this capture likely losing material?
#[inline]
pub fn bad_capture(pos: &Position, mv: Move) -> bool {
    let fsq = mv.from_sq();
    let tsq = mv.to_sq();

    // Equal or winning captures are ok
    if TP_VALUE[pos.tp_on_sq(tsq).index()] >= TP_VALUE[pos.tp_on_sq(fsq).index()] {
        return false;
    }

    // En passant is always ok
    if mv.move_type() == EP_CAP {
        return false;
    }

    see::see(pos, fsq, tsq) < 0
}

// ============================================================================
// Searcher — per-thread search state (history, killers, refutation, etc.)
// ============================================================================

pub struct Searcher {
    pub history: [[i32; 64]; 13],     // [Piece][Square]
    pub killer: [[Move; 2]; MAX_PLY], // [ply][0/1]
    pub refutation: [[Move; 64]; 64], // [from][to] → refutation move
    pub root_depth: i32,
    pub dp_completed: i32,
    pub seldepth: usize, // max ply reached (selective depth)
    pub fl_root_choice: bool,
    pub pv_eng: [Move; 2], // engine's best/ponder moves
    pub nodes: u64,
    pub abort_search: bool,
    pub start_time: std::time::Instant,
    pub time_limit_ms: u64,
    pub move_overhead_ms: u64,   // lag safety buffer
    pub game_key: u64,           // random key per game for eval_blur
    pub nodes_limit: u64,        // node limit from "go nodes" (0 = unlimited)
    pub nps_limit: i32,          // NPS limit for strength clamping
    pub avoid_moves: [Move; 65], // moves to skip in SearchRoot (for MultiPV)
    pub avoid_count: usize,      // number of avoid moves
    pub multi_pv: usize,         // number of PV lines to search
}

impl Searcher {
    pub fn new() -> Self {
        Searcher {
            history: [[0; 64]; 13],
            killer: [[Move::NONE; 2]; MAX_PLY],
            refutation: [[Move::NONE; 64]; 64],
            root_depth: 0,
            dp_completed: 0,
            seldepth: 0,
            fl_root_choice: false,
            pv_eng: [Move::NONE; 2],
            nodes: 0,
            abort_search: false,
            start_time: std::time::Instant::now(),
            time_limit_ms: u64::MAX,
            move_overhead_ms: 50,
            game_key: 0,
            nodes_limit: 0,
            nps_limit: 0,
            avoid_moves: [Move::NONE; 65],
            avoid_count: 0,
            multi_pv: 1,
        }
    }

    pub fn clear_all(&mut self) {
        // SAFETY: history is [[i32; 64]; 13] — all-zeros is valid for i32.
        // Move is repr(transparent) u32 wrapper, Move::NONE = Move(0),
        // so zeroing is equivalent to filling with Move::NONE.
        unsafe {
            std::ptr::write_bytes(self.history.as_mut_ptr(), 0, self.history.len());
            std::ptr::write_bytes(self.refutation.as_mut_ptr(), 0, self.refutation.len());
            std::ptr::write_bytes(self.killer.as_mut_ptr(), 0, self.killer.len());
        }
        self.clear_avoid_list();
    }

    pub fn clear_avoid_list(&mut self) {
        // SAFETY: Move::NONE = Move(0), zeroing is equivalent to filling with NONE
        unsafe {
            std::ptr::write_bytes(self.avoid_moves.as_mut_ptr(), 0, self.avoid_moves.len());
        }
        self.avoid_count = 0;
    }

    pub fn set_avoid_move(&mut self, mv: Move) {
        if self.avoid_count < 65 {
            self.avoid_moves[self.avoid_count] = mv;
            self.avoid_count += 1;
        }
    }

    pub fn is_avoid_move(&self, mv: Move) -> bool {
        for i in 0..self.avoid_count {
            if self.avoid_moves[i] == mv {
                return true;
            }
        }
        false
    }

    pub fn age_hist(&mut self) {
        for pc in 0..13 {
            for sq in 0..64 {
                self.history[pc][sq] /= 8;
            }
        }
        self.killer = [[Move::NONE; 2]; MAX_PLY];
    }

    fn trim_hist(&mut self) {
        for pc in 0..13 {
            for sq in 0..64 {
                self.history[pc][sq] /= 2;
            }
        }
    }

    #[inline]
    pub fn update_history(
        &mut self,
        pos: &Position,
        last_move: Move,
        mv: Move,
        depth: i32,
        ply: usize,
    ) {
        let tsq = mv.to_sq();
        let fsq = mv.from_sq();

        // No update on captures/promotions
        if pos.pc[tsq as usize] != NO_PC || mv.is_prom() || mv.move_type() == EP_CAP {
            return;
        }

        let pc_idx = pos.pc[fsq as usize].index();
        self.history[pc_idx][tsq as usize] += 2 * depth * depth;
        if self.history[pc_idx][tsq as usize] > MAX_HIST {
            self.trim_hist();
        }

        // Update refutation table.
        // Skip when last_move is NONE (null move / root) or SENTINEL (quiescence).
        // Only update counter-moves when a valid previous move exists (0 = null move, -1 = skip).
        if !last_move.is_none() && last_move.0 != u16::MAX {
            let lf = last_move.from_sq() as usize;
            let lt = last_move.to_sq() as usize;
            self.refutation[lf][lt] = mv;
        }

        // Update killer moves
        if ply < MAX_PLY && mv != self.killer[ply][0] {
            self.killer[ply][1] = self.killer[ply][0];
            self.killer[ply][0] = mv;
        }
    }

    #[inline]
    pub fn decrease_history(&mut self, pos: &Position, mv: Move, depth: i32) {
        let tsq = mv.to_sq();
        let fsq = mv.from_sq();

        if pos.pc[tsq as usize] != NO_PC || mv.is_prom() || mv.move_type() == EP_CAP {
            return;
        }

        let pc_idx = pos.pc[fsq as usize].index();
        self.history[pc_idx][tsq as usize] -= depth * depth;
        if self.history[pc_idx][tsq as usize] < -MAX_HIST {
            self.trim_hist();
        }
    }

    #[inline(always)]
    pub fn get_refutation(&self, mv: Move) -> Move {
        if mv.is_none() {
            return Move::NONE;
        }
        self.refutation[mv.from_sq() as usize][mv.to_sq() as usize]
    }

    /// Check if we should abort due to time, node limit, or NPS limit.
    #[inline]
    pub fn check_timeout(&mut self) {
        if self.nodes & 16383 == 0 {
            let elapsed = self.start_time.elapsed().as_millis() as u64;

            // Time limit
            if elapsed >= self.time_limit_ms {
                self.abort_search = true;
                return; // short-circuit: skip remaining checks once we're stopping
            }

            // Poll stdin for "stop"/"quit"
            if input_available() {
                let mut cmd = String::new();
                if std::io::stdin().read_line(&mut cmd).is_ok() {
                    let cmd = cmd.trim();
                    if cmd == "stop" || cmd == "quit" {
                        self.abort_search = true;
                    }
                }
            }

            // Node limit (from "go nodes")
            if self.nodes_limit > 0 && self.nodes >= self.nodes_limit {
                self.abort_search = true;
            }

            // NPS slowdown
            if self.nps_limit > 0 && elapsed > 0 {
                let actual_nps = (self.nodes * 1000) / elapsed;
                if actual_nps > self.nps_limit as u64 {
                    let target_ms = (self.nodes * 1000) / self.nps_limit as u64;
                    let sleep_ms = target_ms.saturating_sub(elapsed);
                    if sleep_ms > 0 {
                        std::thread::sleep(std::time::Duration::from_millis(sleep_ms.min(50)));
                    }
                }
            }
        }
    }
}

/// Non-blocking check if stdin has data available.
/// Check for pending stdin input using platform-specific APIs.
#[cfg(windows)]
fn input_available() -> bool {
    use std::os::windows::io::AsRawHandle;

    #[allow(non_snake_case)]
    extern "system" {
        fn PeekNamedPipe(
            hNamedPipe: *mut std::ffi::c_void,
            lpBuffer: *mut std::ffi::c_void,
            nBufferSize: u32,
            lpBytesRead: *mut u32,
            lpTotalBytesAvail: *mut u32,
            lpBytesLeftThisMessage: *mut u32,
        ) -> i32;
        fn GetNumberOfConsoleInputEvents(
            hConsoleInput: *mut std::ffi::c_void,
            lpNumberOfEvents: *mut u32,
        ) -> i32;
    }

    let handle = std::io::stdin().as_raw_handle();
    let mut bytes_available: u32 = 0;
    unsafe {
        if PeekNamedPipe(
            handle as *mut _,
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            &mut bytes_available,
            std::ptr::null_mut(),
        ) == 0
        {
            // PeekNamedPipe failed — might be a console, not a pipe.
            let mut events: u32 = 0;
            if GetNumberOfConsoleInputEvents(handle as *mut _, &mut events) != 0 {
                return events > 1;
            }
            return false;
        }
    }
    bytes_available > 0
}

/// Non-blocking check if stdin has data available (Unix version).
#[cfg(not(windows))]
fn input_available() -> bool {
    // On non-Windows platforms, skip stdin polling for now.
    // The engine will still respond to time limits and node limits.
    false
}

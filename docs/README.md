<p align="center">
  <img src="logo.png" alt="Tales" width="360" />
</p>

<h1 align="center">Tales — UCI Chess Engine</h1>

<p align="center">
  <img src="https://img.shields.io/badge/language-Rust-orange?style=flat-square&logo=rust" />
  <img src="https://img.shields.io/badge/edition-2024-orange?style=flat-square&logo=rust" />
  <img src="https://img.shields.io/badge/protocol-UCI-blue?style=flat-square" />
  <img src="https://img.shields.io/badge/license-GPL--3.0-red?style=flat-square" />
</p>

---

## Overview

Tales is a UCI chess engine written in Rust. It is a ground-up reimplementation inspired by [OpenTal](https://github.com/sTaleksY/OpenTal) — itself a derivative of **Rodent III** by **Pawel Koziol** — rewritten in idiomatic Rust with a focus on speed and on playing in the attacking, sacrificial style of **Mikhail Tal**.

## Architecture

```
┌────────────────────────────────────────────┐
│                 main.rs                    │
│  entry — dispatches UCI / bench / suite    │
└────────────────────────────────────────────┘
                     │
┌────────────────────────────────────────────┐
│                  uci/                      │
│  command loop · options · go · time_mgmt   │
│  bench · epd suite runner                  │
└────────────────────────────────────────────┘
                     │
┌─────────────────────┐     ┌────────────────┐
│       book/         │     │    search/     │
│ polyglot format     │     │ alphabeta      │
│ internal (embedded) │     │ quiesce (3-lyr)│
│ external (disk)     │     │ ordering       │
└─────────────────────┘     │ threads (SMP)  │
                            │ uci_info       │
                            └────────────────┘
                                    │
                            ┌───────────────┐
                            │     tt.rs     │
                            │ 4-bucket, age │
                            └───────────────┘
                                    │
┌────────────────────────────────────────────┐
│                   eval/                    │
│  material · pst · pieces · pawns · passers │
│  patterns · king_safety · threats · endgame│
│  params · eval_data · pawn_hash            │
└────────────────────────────────────────────┘
                     │
┌────────────────────────────────────────────┐
│                  movegen/                  │
│       generate · movelist · see            │
└────────────────────────────────────────────┘
                     │
┌────────────────────────────────────────────┐
│                   board/                   │
│  position · types · moves · bitboard       │
│  attacks · magic · masks · distance        │
│  zobrist                                   │
└────────────────────────────────────────────┘
```

### Source layout

```
src/
├── main.rs          # entry point: UCI / --test / --bench / --suite
├── board/           # position, bitboards, magic, zobrist, move types
├── movegen/         # move generation, move list, SEE
├── book/            # polyglot format + internal (embedded) + external
├── eval/            # static evaluation (material, PST, pawns, king safety, ...)
├── search/          # alpha-beta, quiescence, ordering, Lazy SMP threads
├── tt.rs            # transposition table (4-bucket, age-based replacement)
└── uci/             # UCI protocol, options, time management, bench, EPD
```

## UCI options

| Name | Type | Default | Range | Purpose |
|:---|:---:|:---:|:---:|:---|
| `Hash` | spin | 16 | 1–33554432 | Transposition table size in MB. |
| `Threads` | spin | 1 | 1–1024 | Number of Lazy-SMP search threads sharing the TT. |
| `MultiPV` | spin | 1 | 1–64 | Number of best principal variations to report. |
| `MoveOverhead` | spin | 50 | 0–5000 | Milliseconds subtracted from the allocated time for network/GUI lag. |
| `Clear Hash` | button | — | — | Zero out the transposition table. |
| `Ponder` | check | false | — | Allow background search on the expected opponent reply. |
| `UseBook` | check | false | — | Probe an opening book before starting the search. When true, the engine loads `MainBookFile`; on failure it falls back to the embedded book. |
| `VerboseBook` | check | false | — | Emit `info string` lines for every book probe. |
| `BookFilter` | spin | 20 | 0–100 | Quality gate for book moves, as a percentage of the best candidate's weight. `0` keeps every move, `100` keeps only the top-weighted move(s). |
| `MainBookFile` | string | book.bin | — | Path to an external Polyglot `.bin` book. Reloaded immediately when `UseBook` is on. |
| `TimeBuffer` | spin | 50 | 0–1000 | Extra safety margin (ms) held back by the time manager. |
| `Contempt` | spin | 0 | -100–100 | Draw score bias from the engine's perspective (cp). Positive values avoid draws. |
| `EvalBlur` | spin | 0 | 0–40 | Adds deterministic per-game noise to the evaluation for handicapping. |
| `NpsLimit` | spin | 0 | 0–1000000 | Throttle the search to roughly N nodes per second. `0` means no limit. |
| `UCI_Elo` | spin | 2800 | 800–2800 | Target playing strength when strength limiting is enabled. |
| `UCI_LimitStrength` | check | false | — | Enable Elo-based weakening (combines NPS throttling with `EvalBlur`). |
| `SlowMover` | spin | 100 | 10–200 | Percentage scaling of the time allocated per move. |
| `Selectivity` | spin | 175 | 100–500 | LMR aggressiveness knob — higher values reduce more quiet moves. |

## Building

```bash
cargo build --release
```

Requires Rust 1.95.0 or newer (edition 2024). The opening book is embedded in the binary via `include_bytes!`; no external files are needed to play.

## Credits

Tales builds on a lineage of open-source chess engines. Credit and thanks to:

- **Pawel Koziol** — author of **Rodent III**, whose evaluation architecture, pattern catalogue, and "engine with character" philosophy are the direct ancestor of Tales.
- **sTaleksY** — author of [**OpenTal**](https://github.com/sTaleksY/OpenTal), a Rodent-derived engine tuned for Tal-style attacking play; Tales started as a port of OpenTal before diverging.
- **Pablo Vazquez** — author of **Sungorus 1.4**, the engine from which the Rodent family descends.
- The **Chess Programming Wiki** and the broader engine-development community for the algorithmic knowledge (PVS, LMR, NMP, SEE, magic bitboards, …) that every modern engine relies on.

Rodent III and OpenTal are released under the GNU General Public License, and Tales continues under the same license.

## License

GPL-3.0-or-later. See [LICENSE](../LICENSE).

## Author

Andre MARTINS

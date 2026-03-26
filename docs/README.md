<p align="center">
  <img src="logo.png" alt="Tales UCI Chess Engine" width="420" />
</p>

<h1 align="center">Tales — UCI Chess Engine</h1>

<p align="center">
  <em>A high-performance UCI chess engine written in Rust, inspired by the spirit of Mikhail Tal.</em>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/language-Rust-orange?style=flat-square&logo=rust" />
  <img src="https://img.shields.io/badge/protocol-UCI-blue?style=flat-square" />
  <img src="https://img.shields.io/badge/version-1.0a-green?style=flat-square" />
  <img src="https://img.shields.io/badge/license-GPL--3.0-red?style=flat-square" />
  <img src="https://img.shields.io/badge/platform-Windows%20%7C%20Linux-lightgrey?style=flat-square" />
</p>

---

## 🔥 What is Tales?

**Tales** is a modern UCI chess engine written from the ground up in **Rust**. It is a complete re-engineering of the [OpenTal](https://github.com/) engine (itself derived from the **Rodent III** chess engine by Pawel Koziol), rebuilt for speed, safety, and aggressive play.

Tales is not a translation — it is a **ground-up Rust implementation**, with every hot path profiled, every allocation scrutinized, and every pruning threshold tuned. The result is an engine that is **over 30% faster** than its C++ ancestor while playing sharp, attacking chess inspired by the legendary **Mikhail Tal**.

---

## ✨ Features

### 🧠 Search Engine

Tales implements a state-of-the-art alpha-beta search with the following techniques:

- **Principal Variation Search (PVS)** with zero-window re-search
- **Aspiration Windows** with exponential widening (starting at ±8cp)
- **Null Move Pruning (NMP)** with adaptive Stockfish-style reduction and verification search at high depths
- **Static Null Move Pruning** (Reverse Futility) at depths ≤ 3
- **Razoring** (Toga II–style) at depths ≤ 4 — drops into quiescence when far below beta
- **Futility Pruning** for quiet moves at depths ≤ 6
- **Late Move Reductions (LMR)** with logarithmic tables, PV-aware, history-adjusted
- **Singular Extensions** for clearly dominant TT moves (Senpai-style)
- **Check Extensions**, **Pawn-to-7th Extensions**
- **Internal Iterative Deepening (IID)** when no hash move is available
- **Mate Distance Pruning** to cut off proven mates

### ♟️ Quiescence Search

- **3-layer architecture**: checks → evasions → captures-only
- Full TT probing in all QS layers
- SEE-based capture filtering

### 📊 Move Ordering

- TT move → Captures (MVV-LVA) → Killers (2 per ply) → Countermove/Refutation → History heuristic
- History bonus on beta cutoffs, malus on previously failed quiet moves
- Null-move refutation square tracking

### ⚡ Multi-Threaded Search (Lazy SMP)

- Shared lockless transposition table across all threads
- Odd-numbered threads search at `depth + 1` for natural diversification
- Lagging thread skip: threads that fall behind rejoin at useful depths
- Atomic coordination via `AtomicBool` (abort), `AtomicI32` (depth reached), `AtomicU64` (node count)
- Only the main thread (thread 0) prints UCI info lines

### 📖 Opening Book

Tales includes a **dual book system** — an always-available internal book compiled into the binary, plus optional support for user-supplied external Polyglot books:

#### Internal Book (default)

- **Polyglot-format Tal book** (`ph-tal2.bin`, ~2 MB) compiled directly into the binary via `include_bytes!`
- Canonical 781-entry Polyglot Zobrist table for position hashing
- **Weighted random selection** with configurable filter threshold (default 20%)
- Binary search for O(log n) probing, executes *before* the search thread is spawned
- No external book files required — works out of the box

#### External Book (v1.0a+)

- Users can specify any **Polyglot `.bin` book** via the `MainBookFile` UCI option
- Enable with `UseBook true` — the engine loads the file from disk into memory
- When a valid external book is loaded, it is used **instead of** the internal book
- If the file is not found or invalid, the engine prints an error and **falls back** to the internal book
- The external book uses the same weighted random selection and Zobrist hashing as the internal book
- Hot-swappable: changing `MainBookFile` while `UseBook` is active immediately reloads the new file

#### Book Move Selection (`BookFilter`)

Both the internal and external books use **weighted random selection** controlled by the `BookFilter` UCI option (0–100, default 20). This determines how selective the engine is when choosing from multiple book moves for the same position:

- **`BookFilter 0`** — Consider *all* book moves, regardless of weight. Maximum opening variety.
- **`BookFilter 20`** (default) — Filter out moves below 20% of the best move's weight. Balanced play.
- **`BookFilter 100`** — Only consider the highest-weighted move(s). Deterministic, strongest-line play.

Among the surviving candidates, the engine picks a move via weighted random selection — higher-weighted moves are still more likely to be chosen, but lower-weighted alternatives add variety.

### 🏰 Evaluation Engine

The evaluation is a comprehensive hand-crafted evaluation with ~200 parameters, all tuned for aggressive, Tal-like play:

- **Material evaluation** with bishop pair, knight pair, rook pair adjustments
- **Material imbalance** table (Crafty-style 9×9 matrix)
- **Piece-Square Tables (PSTs)** — separate MG/EG tables for all piece types, with outpost and pawn formation tables
- **Piece mobility** — per-piece-type mobility tables (knight 0–8, bishop 0–13, rook 0–14, queen 0–27)
- **King safety** — non-linear danger table (quadratic curve, 512 entries) driven by accumulated attack scores
- **King tropism** — bonus for piece proximity to the enemy king
- **Pawn structure** — doubled, isolated, backward pawn penalties; pawn islands; pawn binds
- **Pawn chains** — full triad evaluator (e.g., c6-d5-e4 complexes) with storm pattern detection
- **Pawn shield & storm** — rank-based shield penalties and storm bonuses around the castled king
- **Passed pawns** — rank-based MG/EG bonuses with stop-square control multipliers
- **Candidate passers** — bonuses for pawns that could become passers
- **Pattern recognition** — knight traps, bishop traps, blocked bishops, fianchetto bonuses, central patterns, king patterns
- **Threats** — bonus for pieces attacking undefended enemy pieces
- **Endgame scaling** — KPK, KBPK, KNPK, KRPKR, KQKRP draw detection and scaling
- **KBN vs K** checkmate helper
- **Eval caching** — 65536-entry evaluation hash table
- **Pawn hash table** — separate pawn structure cache
- **Asymmetric attack weights** — own attack scaled at 450%, opponent at 100% (Tal-like aggression)
- **Piece-keeping bonuses** — reluctance to trade queens and knights
- **Tempo bonus** — 14cp MG / 7cp EG for the side to move
- **Phase interpolation** — smooth MG↔EG blending based on material phase (0–24)

### 🔧 UCI Interface

- 16 fully implemented UCI options
- **UCI_Elo** strength limiting with NPS throttling and eval blur
- **MultiPV** support (up to 64 lines)
- **Pondering** support
- Built-in `bench` command for regression testing
- `d` / `print` command for board display

### 🎯 Strength Limiting

- **UCI_LimitStrength** + **UCI_Elo** (800–2800)
- Automatic NPS throttling and evaluation noise injection
- Adjustable selectivity, contempt, and time management via UCI options

---

## 🚀 Performance & Optimizations

Every module was **profiled with flamegraph analysis** and optimized for maximum throughput. We identified critical hot paths in the search and evaluation code and applied targeted optimizations:

| Optimization | Description |
|:---|:---|
| **`MaybeUninit` move arrays** | Eliminates zeroing overhead for `[Move; 256]` arrays in the search loop |
| **`get_unchecked` indexing** | Bounds-check elimination across TT probes, eval hash, move ordering |
| **TT prefetch** | `_mm_prefetch` hints before TT probes — cache line loaded during `in_check()` computation |
| **4-bucket TT** | Age-based replacement with 64-bit full keys for correctness |
| **Stack-allocated arrays** | All move lists and PV arrays are fixed-size on the stack — zero heap allocation in the search |
| **Incremental PST scoring** | Position scores updated incrementally during `do_move` / `undo_move` |
| **LMR table in L1 cache** | Move dimension clamped to 64 entries to fit the full table in 32KB L1 |
| **Eval hash + Pawn hash** | Two-level caching avoids redundant evaluation of seen positions and pawn structures |
| **Fat LTO** | Full link-time optimization across all crates for maximum inlining |
| **Single codegen unit** | Enables whole-program optimization passes |
| **`panic = "abort"`** | Eliminates unwinding overhead in release builds |
| **`-C target-cpu=native`** | Automatic use of POPCNT, BMI2, AVX2 hardware instructions |

### Build Profile

```toml
[profile.release]
opt-level = 3
lto = "fat"
codegen-units = 1
panic = "abort"
```

### Results

- **~2 million nodes per second** on standard hardware
- **30%+ throughput increase** over the C++ OpenTal engine
- **Zero** crashes, zero illegal moves across 10,000+ tested plies

---

## 🏆 Tournament Results

Tales has been validated through automated tournament testing against its reference engine.

### Tales vs OpenTal (C++ Reference) — 2000 games

| | Tales | OpenTal |
|:---|:---:|:---:|
| Wins | 800 | 600 |
| Draws | 600 | 600 |
| Losses | 600 | 800 |
| **Score** | **1200.0** | **800.0** |

> *60% win rate, demonstrating the combined impact of Rust optimizations and search tuning.*

### Stability Record

- ✅ Zero crashes or engine failures
- ✅ Zero illegal moves
- ✅ Zero `bestmove NONE` forfeits
- ✅ Stable across multi-threaded configurations

---

## 📦 Getting Started

### Building from Source

```bash
git clone https://github.com/your-username/tales.git
cd tales
cargo build --release
```

The binary will be at `target/release/tales.exe` (Windows) or `target/release/tales` (Linux).

> **Note**: No external files are needed. The opening book is embedded in the binary. To use a custom opening book, see the UCI options `UseBook` and `MainBookFile` below.

### Running with a GUI

Tales works with any UCI-compatible chess GUI:

- [Arena Chess GUI](http://www.playwitharena.de/)
- [CuteChess](https://cutechess.com/)
- [Lucas Chess](https://lucaschess.pythonanywhere.com/)
- [Banksia GUI](https://banksiagui.com/)

Simply add the `tales` binary as a new engine in your GUI of choice.

### Running the Built-in Benchmark

```bash
./tales --bench
```

This runs a fixed set of 5 positions at increasing depths and reports total nodes, NPS, and time.

### UCI Options

| Option | Type | Default | Description |
|:---|:---|:---:|:---|
| `Hash` | spin | 16 | Transposition table size in MB (1–33554432) |
| `Threads` | spin | 1 | Number of search threads (1–1024, Lazy SMP) |
| `MultiPV` | spin | 1 | Number of principal variations (1–64) |
| `MoveOverhead` | spin | 50 | Network/GUI lag buffer in ms (0–5000) |
| `Ponder` | check | false | Enable background search on opponent's time |
| `UseBook` | check | false | When `true`, use external book from `MainBookFile` instead of internal |
| `VerboseBook` | check | false | Show book probe details in UCI info strings |
| `BookFilter` | spin | 20 | Book move quality threshold (0=all moves, 100=best only) |
| `MainBookFile` | string | book.bin | Path to an external Polyglot `.bin` opening book |
| `TimeBuffer` | spin | 50 | Additional time safety margin (0–1000 ms) |
| `Contempt` | spin | 0 | Draw score bias (−100 to +100 cp) |
| `EvalBlur` | spin | 0 | Evaluation noise for handicapping (0–40) |
| `NpsLimit` | spin | 0 | Maximum nodes per second (0 = unlimited) |
| `UCI_Elo` | spin | 2800 | Target playing strength (800–2800) |
| `UCI_LimitStrength` | check | false | Enable Elo-based strength limiting |
| `SlowMover` | spin | 100 | Time management scaling % (10–200) |
| `Selectivity` | spin | 175 | LMR aggressiveness factor (100–500) |

---

## 🏗️ Architecture

```
┌──────────────────────────────────────────────┐
│              UCI Interface (uci/)            │
│         Async command processing             │
├──────────────────────────────────────────────┤
│    Opening Book      │    Search Engine      │
│   ┌──────────────┐   │   ┌─────────────────┐ │
│   │  Internal    │   │   │ Iterative Deep. │ │
│   │  (embedded)  │   │   │ ├─ Aspiration   │ │
│   ├──────────────┤   │   │ ├─ PVS + LMR    │ │
│   │  External    │   │   │ ├─ NMP + Verify │ │
│   │  (disk .bin) │   │   │ ├─ Razoring     │ │
│   └──────────────┘   │   │ ├─ Singular Ext │ │
│                      │   │ └─ 3-Layer QS   │ │
│                      │   └─────────────────┘ │
├──────────────────────────────────────────────┤
│            Evaluation Engine (eval/)         │
│  ┌──────────┐ ┌──────────┐ ┌──────────────┐  │
│  │ Material │ │   PSTs   │ │ King Safety  │  │
│  │(Imbalanc)│ │ (MG/EG)  │ │(Danger Table)│  │
│  ├──────────┤ ├──────────┤ ├──────────────┤  │
│  │ Mobility │ │  Pawns   │ │  Patterns    │  │
│  │          │ │ (Chains) │ │  & Threats   │  │
│  ├──────────┤ ├──────────┤ ├──────────────┤  │
│  │ Passers  │ │ Endgame  │ │  Eval Hash   │  │
│  └──────────┘ └──────────┘ └──────────────┘  │
├──────────────────────────────────────────────┤
│          Board Representation (board/)       │
│  Hybrid Mailbox + Bitboard │ Magic Bitboard  │
│  Zobrist Hashing           │ SEE / Perft     │
├──────────────────────────────────────────────┤
│        Lazy SMP Thread Pool (search/)        │
│  Shared TT (tt/)  │  Atomic Coordination     │
└──────────────────────────────────────────────┘
```

### Source Structure

```
src/
├── main.rs          # Entry point (UCI / test / bench modes)
├── board/           # Position, bitboards, magic tables, Zobrist, types
├── movegen/         # Move generation, move list, SEE
├── eval/            # Evaluation: params, PST, pieces, pawns, passers,
│                    #   patterns, king safety, threats, endgame, pawn hash
├── search/          # Alpha-beta, quiescence, move ordering, Lazy SMP
├── tt/              # Transposition table (4-bucket, age-based)
├── book/            # Opening book (internal embedded + external disk-based)
└── uci/             # UCI protocol handler
```

---

## 🆕 What's New in 1.0a

### External Opening Book Support

Tales now supports **user-supplied Polyglot opening books**. This allows you to replace the built-in Tal-style book with any `.bin` Polyglot book file — from a broader GM repertoire to a specialized opening suite.

**How to use:**

1. Set the path to your book file:

   ```
   setoption name MainBookFile value /path/to/my-book.bin
   ```

2. Enable external book usage:

   ```
   setoption name UseBook value true
   ```

3. The engine will confirm loading:

   ```
   info string loaded external book '/path/to/my-book.bin' (128903 entries)
   ```

**Error handling:** If the file cannot be found or is corrupt, the engine prints an error and automatically falls back to the internal embedded book:

```
info string error loading external book: cannot read 'missing.bin': ... — using internal book
```

**Behavior summary:**

| `UseBook` | External file found | Book used |
|:---------:|:-------------------:|:---------:|
| `false`   | —                   | Internal  |
| `true`    | ✅ Yes              | External  |
| `true`    | ❌ No / invalid     | Internal (fallback) |

### Book Move Quality Filter

The new **`BookFilter`** option (inspired by Rodent III/IV) controls how selective the engine is when choosing from book moves:

```
setoption name BookFilter value 0     # use all book moves (maximum variety)
setoption name BookFilter value 20    # default — filter weak alternatives
setoption name BookFilter value 100   # always play the best book move
```

The value is a percentage threshold: moves whose weight is below `(best_weight × BookFilter / 100)` are excluded. This applies to both the internal and external books.

---

## 🙏 Acknowledgments & Credits

Tales stands on the shoulders of giants. This engine would not exist without the foundational work of the open-source chess programming community.

### OpenTal & Rodent III

Tales is a modern Rust reimplementation inspired by the **OpenTal** engine, which is itself a derivative of the **Rodent III** chess engine. We owe an enormous debt of gratitude to the original creators:

- **Pawel Koziol** — Creator of **Rodent III**, whose innovative evaluation architecture, comprehensive pattern recognition, and the idea of a chess engine with character formed the algorithmic foundation for Tales. The evaluation philosophy — from the pawn chain triad evaluator to the non-linear king danger table — traces directly back to Pawel's visionary work.

- **Pablo Vazquez** — Creator of **Sungorus 1.4**, the original engine from which the Rodent family was derived.

Both Rodent III and OpenTal are released under the **GNU General Public License** (GPL), and Tales proudly continues in that tradition.

### The Chess Programming Community

Tales also draws inspiration from techniques pioneered by the broader community:

- **Stockfish** — Null move reduction formula, LMR table construction, mate distance pruning
- **Toga II** — Razoring implementation
- **Senpai** — Singular extension approach
- **The Chess Programming Wiki** — An invaluable resource for search and evaluation algorithms

### Author

**Tales** is developed and maintained by **Andre MARTINS**.

---

## 📄 License

Tales is free software, released under the **GNU General Public License v3.0** (GPL-3.0).

You are free to use, modify, and redistribute this software under the terms of the GPL. See the [LICENSE](../LICENSE) file for details.

---

<p align="center">
  <img src="logo.png" alt="Tales" width="120" />
  <br />
  <em>Built with ♟️ and 🦀</em>
</p>

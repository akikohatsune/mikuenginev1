#![allow(unused_mut)]
pub mod attacks;
pub mod bitboard;
pub mod board;
pub mod eval;
pub mod history;
pub mod movegen;
pub mod movepick;
pub mod nnue;
pub mod perft;
pub mod search;
pub mod smp;
pub mod time;
pub mod transposition;
pub mod types;
pub mod uci;
pub mod zobrist;

use std::env;
use std::path::PathBuf;
use std::sync::Arc;

fn find_nnue_file() -> Option<PathBuf> {
    // 1. Tìm cạnh file exe
    if let Ok(exe) = env::current_exe() {
        if let Some(dir) = exe.parent() {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().map(|e| e == "nnue").unwrap_or(false) {
                        return Some(path);
                    }
                }
            }
        }
    }
    // 2. Tìm trong thư mục hiện tại (cwd)
    if let Ok(entries) = std::fs::read_dir(".") {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "nnue").unwrap_or(false) {
                return Some(path);
            }
        }
    }
    None
}

fn main() {
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024) // 32MB
        .name("main".to_string())
        .spawn(|| real_main())
        .unwrap()
        .join()
        .unwrap();
}

fn real_main() {
    // Initialize magic bitboard tables before anything else
    attacks::init_magics();

    let args: Vec<String> = env::args().collect();

    // Debug: show NNUE search paths
    if args.len() > 1 && args[1] == "--pwd_nnue" {
        println!("=== NNUE File Search ===");
        if let Ok(exe) = env::current_exe() {
            println!("Exe path: {}", exe.display());
            if let Some(dir) = exe.parent() {
                println!("Exe dir:  {}", dir.display());
            }
        }
        if let Ok(cwd) = env::current_dir() {
            println!("CWD:      {}", cwd.display());
        }
        match find_nnue_file() {
            Some(path) => println!("Found:    {}", path.display()),
            None => println!("Found:    (none)"),
        }
        return;
    }

    let nnue = if let Some(path) = find_nnue_file() {
        eprintln!("info string Loading NNUE: {}", path.display());
        match nnue::NNUE::load(path.to_str().unwrap_or("")) {
            Ok(n) => {
                eprintln!("info string NNUE loaded successfully");
                Arc::new(n)
            }
            Err(e) => {
                eprintln!("info string Failed to load NNUE: {}, using fallback", e);
                Arc::new(nnue::NNUE::new())
            }
        }
    } else {
        eprintln!("info string No .nnue file found, using fallback evaluation");
        Arc::new(nnue::NNUE::new())
    };

    if args.len() > 1 && args[1] == "perft" {
        let depth = if args.len() > 2 {
            args[2].parse().unwrap_or(5)
        } else {
            5
        };
        let mut board = board::Board::new(nnue.clone());
        perft::divide(&mut board, depth);
    } else if args.len() > 1 && args[1] == "bench" {
        // Run bench without GUI interaction. Pass mock commands to uci module.
        // Or simply execute uci bench directly and exit.
        println!("Running benchmark from command line...");
        let stdin = format!("bench\nquit\n");
        // We can just pipe exactly "bench" then "quit" into uci loop?
        // Actually uci_loop reads stdin. It's easier to refactor uci loop slightly or send it through a channel.
        // A simple hack is to spawn a thread that pushes "bench\nquit\n" to uci loop but since we read from stdin, that's tricky.
        // Let's extract the bench logic or just do what Stockfish does - inject it into the uci commands?
        // Since `uci_loop` listens to standard input, testing natively is easiest by setting up a custom function for bench.
        // For simplicity now, let's just instruct users to use "bench" inside UCI or make a dedicated bench fn.
        // Let's make a dedicated run_bench function in uci.rs or just call the logic.
        uci::run_bench(nnue.clone(), &args[2..]);
    } else {
        uci::uci_loop(nnue);
    }
}

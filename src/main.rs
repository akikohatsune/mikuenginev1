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
    } else {
        uci::uci_loop(nnue);
    }
}

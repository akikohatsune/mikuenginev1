use crate::board::Board;
use crate::movegen::{generate_pseudo_legal_moves, MoveList};
use crate::nnue::NNUE;
use crate::search::Search;
use crate::transposition::TranspositionTable;
use crate::types::{Color, PieceType};
use std::io::{self, BufRead};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub fn uci_loop(nnue: Arc<NNUE>) {
    let stdin = io::stdin();
    let mut board = Board::new(nnue.clone());
    let mut tt = Arc::new(TranspositionTable::new(16));
    let stop_flag = Arc::new(AtomicBool::new(false));
    let mut num_threads = 1;

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let tokens: Vec<&str> = line.split_whitespace().collect();
        let cmd = tokens[0];

        match cmd {
            "uci" => {
                println!("id name mikuchess");
                println!("id author komekokomi");
                println!("option name Hash type spin default 16 min 1 max 1024");
                println!("option name Threads type spin default 1 min 1 max 64");
                println!("uciok");
            }
            "isready" => {
                println!("readyok");
            }
            "setoption" => {
                if tokens.len() >= 5 && tokens[1] == "name" {
                    let name = tokens[2].to_lowercase();
                    if name == "threads" && tokens[3] == "value" {
                        if let Ok(t) = tokens[4].parse::<usize>() {
                            num_threads = t.max(1).min(64);
                        }
                    } else if name == "hash" && tokens[3] == "value" {
                        if let Ok(mb) = tokens[4].parse::<usize>() {
                            tt = Arc::new(TranspositionTable::new(mb));
                        }
                    }
                }
            }
            "ucinewgame" => {
                board = Board::new(nnue.clone());
                tt.clear();
            }
            "position" => {
                parse_position(&mut board, &tokens, nnue.clone());
            }
            "go" => {
                stop_flag.store(false, Ordering::Relaxed);
                parse_go(
                    board.clone(),
                    tt.clone(),
                    stop_flag.clone(),
                    num_threads,
                    &tokens,
                );
            }
            "stop" => {
                stop_flag.store(true, Ordering::Relaxed);
            }
            "quit" => {
                stop_flag.store(true, Ordering::Relaxed);
                break;
            }
            _ => {}
        }
    }
}

fn parse_position(board: &mut Board, tokens: &[&str], nnue: Arc<NNUE>) {
    if tokens.len() < 2 {
        return;
    }

    let mut moves_idx = 0;
    if tokens[1] == "startpos" {
        *board = Board::new(nnue.clone());
        moves_idx = 2;
    } else if tokens[1] == "fen" {
        if tokens.len() < 8 {
            return;
        }
        let fen = tokens[2..8].join(" ");
        if let Some(b) = Board::from_fen(&fen, nnue.clone()) {
            *board = b;
        }
        moves_idx = 8;
    }

    if tokens.len() > moves_idx && tokens[moves_idx] == "moves" {
        for i in (moves_idx + 1)..tokens.len() {
            let m_str = tokens[i];
            let mut list = MoveList::new();
            generate_pseudo_legal_moves(board, &mut list);

            if m_str.len() < 4 {
                continue;
            }
            let from_file = m_str.chars().next().unwrap() as u8 - b'a';
            let from_rank = m_str.chars().nth(1).unwrap() as u8 - b'1';
            let to_file = m_str.chars().nth(2).unwrap() as u8 - b'a';
            let to_rank = m_str.chars().nth(3).unwrap() as u8 - b'1';

            let from_sq = from_rank * 8 + from_file;
            let to_sq = to_rank * 8 + to_file;

            let mut promo_type = 0;
            if m_str.len() == 5 {
                let p = m_str.chars().nth(4).unwrap();
                promo_type = match p {
                    'q' => 3,
                    'r' => 2,
                    'b' => 1,
                    'n' => 0,
                    _ => 0,
                };
            }

            for j in 0..list.count {
                let m = list.moves[j];
                if m.from_sq() == from_sq && m.to_sq() == to_sq {
                    let m_promo = (m.0 >> 12) & 0x3;
                    if m_str.len() == 5 {
                        if m_promo == promo_type
                            && ((m.0 >> 12) & 0x3 != 0
                                || (board.piece_on_sq[from_sq as usize].unwrap().piece_type()
                                    == PieceType::Pawn
                                    && (to_rank == 7 || to_rank == 0)))
                        {
                            board.make_move(m);
                            break;
                        }
                    } else {
                        board.make_move(m);
                        break;
                    }
                }
            }
        }
    }
}

fn parse_go(
    board: Board,
    tt: Arc<TranspositionTable>,
    stop: Arc<AtomicBool>,
    num_threads: usize,
    tokens: &[&str],
) {
    let mut depth = 6;
    let mut wtime = 0;
    let mut btime = 0;
    let mut winc = 0;
    let mut binc = 0;
    let mut movetime = 0;

    let mut i = 1;
    while i < tokens.len() {
        match tokens[i] {
            "depth" => {
                depth = tokens[i + 1].parse().unwrap_or(6);
                i += 1;
            }
            "wtime" => {
                wtime = tokens[i + 1].parse().unwrap_or(0);
                i += 1;
            }
            "btime" => {
                btime = tokens[i + 1].parse().unwrap_or(0);
                i += 1;
            }
            "winc" => {
                winc = tokens[i + 1].parse().unwrap_or(0);
                i += 1;
            }
            "binc" => {
                binc = tokens[i + 1].parse().unwrap_or(0);
                i += 1;
            }
            "movetime" => {
                movetime = tokens[i + 1].parse().unwrap_or(0);
                i += 1;
            }
            _ => (),
        }
        i += 1;
    }

    let mut final_soft_limit = std::time::Duration::from_secs(u64::MAX);
    let mut final_hard_limit = std::time::Duration::from_secs(u64::MAX);

    if movetime > 0 {
        final_soft_limit =
            std::time::Duration::from_millis((movetime as u64).saturating_sub(50).max(1));
        final_hard_limit =
            std::time::Duration::from_millis((movetime as u64).saturating_sub(10).max(1));
        depth = 64;
    } else if (wtime > 0 && board.side_to_move == Color::White)
        || (btime > 0 && board.side_to_move == Color::Black)
    {
        let (time, inc) = if board.side_to_move == Color::White {
            (wtime as u64, winc as u64)
        } else {
            (btime as u64, binc as u64)
        };

        let base_time = time / 30;
        let increment_bonus = inc / 2;
        let move_time = ((base_time + increment_bonus) * 9) / 10;

        let mut allocated = move_time;
        if allocated > time {
            allocated = time / 10;
        }

        final_soft_limit = std::time::Duration::from_millis(allocated.saturating_sub(50).max(10));
        final_hard_limit =
            std::time::Duration::from_millis((allocated * 3).min(time.saturating_sub(50)).max(10));
        depth = 64;
    }

    // Spawn a detached thread so the UCI loop can continue processing "stop" etc.
    let _ = std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .name("uci_supervisor".to_string())
        .spawn(move || {
            let mut workers = vec![];

            for t_id in 0..num_threads {
                // Each thread gets its own Search state and Heuristics, but shares TT and stop.
                let mut search = Box::new(Search::new(tt.clone(), stop.clone(), t_id));
                search.soft_limit = final_soft_limit;
                search.hard_limit = final_hard_limit;
                let mut worker_board = board.clone();

                let handle = std::thread::Builder::new()
                    .stack_size(8 * 1024 * 1024)
                    .name(format!("search_worker_{}", t_id))
                    .spawn(move || {
                        let best = search.iterate(&mut worker_board, depth);
                        (t_id, best)
                    })
                    .unwrap();
                workers.push(handle);
            }

            let mut main_best_move = None;

            for handle in workers {
                if let Ok((id, m)) = handle.join() {
                    if id == 0 {
                        main_best_move = Some(m);
                    }
                }
            }

            if let Some(m) = main_best_move {
                println!("bestmove {:?}", m);
            }
        });
}

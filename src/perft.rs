use crate::board::Board;
use crate::movegen::{generate_pseudo_legal_moves, MoveList};

pub fn perft(board: &mut Board, depth: u8) -> u64 {
    if depth == 0 {
        return 1;
    }

    let mut list = MoveList::new();
    generate_pseudo_legal_moves(board, &mut list);

    let mut nodes = 0;
    
    for i in 0..list.count {
        let m = list.moves[i];
        let undo = board.make_move(m);

        let side = board.side_to_move.flip();
        let king_sq_opt = board.piece_bb(crate::types::PieceType::King) & board.color_occupancy(side);
        
        if king_sq_opt.is_not_empty() {
            let king_sq = crate::types::Square::new(king_sq_opt.lsb());
            // is_square_attacked needs to check if king is attacked by current side_to_move
            if !board.is_square_attacked(king_sq, board.side_to_move) {
                nodes += perft(board, depth - 1);
            }
        }

        board.unmake_move(m, &undo);
    }

    nodes
}

pub fn divide(board: &mut Board, depth: u8) {
    if depth == 0 {
        return;
    }

    println!("--- Perft Divide Depth {} ---", depth);
    let mut list = MoveList::new();
    generate_pseudo_legal_moves(board, &mut list);

    let mut total_nodes = 0;

    for i in 0..list.count {
        let m = list.moves[i];
        let undo = board.make_move(m);

        let side = board.side_to_move.flip();
        let king_sq_opt = board.piece_bb(crate::types::PieceType::King) & board.color_occupancy(side);
        
        if king_sq_opt.is_not_empty() {
            let king_sq = crate::types::Square::new(king_sq_opt.lsb());
            if !board.is_square_attacked(king_sq, board.side_to_move) {
                let nodes = perft(board, depth - 1);
                println!("{:?}: {}", m, nodes);
                total_nodes += nodes;
            }
        }

        board.unmake_move(m, &undo);
    }
    println!("Total nodes: {}", total_nodes);
}

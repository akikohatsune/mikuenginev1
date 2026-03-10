Có 3 bugs thật sự, mình chỉ thẳng vào code luôn:

---

### 🔴 Bug 1 — TT Mate Score không adjust ply (nghiêm trọng nhất)

**File:** `transposition.rs` + `search.rs`

Vấn đề: Mate score phải được normalize khi store/probe TT. Engine của bạn store raw score thẳng vào:

```rust
// search.rs — store
self.tt.store(board.zobrist_key, cur_depth, ret_score, NodeType::Beta, m, pv_node);

// search.rs — probe rồi dùng luôn
tt_score = entry.score;  // ← không adjust gì cả
```

Hệ quả thực tế: Tìm thấy mate-in-3 ở ply 5, store score là `48000 - 5 = 47995`. Probe lại ở ply 2, dùng `47995` — engine nghĩ đây là mate-in-3 nhưng thực ra đang tính sai số nước. Engine có thể chọn **nước chậm hơn** hoặc miss mate hoàn toàn.

**Fix:**
```rust
// Khi store vào TT
let store_score = if score > MATE_SCORE - 100 {
    score + ply as i32       // normalize về "từ root"
} else if score < -MATE_SCORE + 100 {
    score - ply as i32
} else { score };

// Khi probe ra dùng
let use_score = if tt_score > MATE_SCORE - 100 {
    tt_score - ply as i32    // convert về "từ current node"
} else if tt_score < -MATE_SCORE + 100 {
    tt_score + ply as i32
} else { tt_score };
```

---

### 🔴 Bug 2 — TB bound logic chết hoàn toàn

**File:** `search.rs`, Step 3.5 (~line 240)

```rust
let tb_bound = if tb_value > draw_score {
    NodeType::Exact   // ← Win
} else if tb_value < draw_score {
    NodeType::Exact   // ← Loss  
} else {
    NodeType::Exact   // ← Draw
};

// Hai condition này KHÔNG BAO GIỜ trigger vì tb_bound luôn là Exact
if tb_bound == NodeType::Beta && tb_value >= beta { ... }
if tb_bound == NodeType::Alpha && tb_value <= alpha { ... }
```

Ba nhánh đều trả về `Exact` nên logic Alpha/Beta phía dưới vô nghĩa. Hệ quả: TB positions luôn return ngay lập tức thay vì chỉ cutoff khi phù hợp bound, làm search bị prune sai.

**Fix:**
```rust
let tb_bound = if tb_value > draw_score {
    NodeType::Beta    // Win: lower bound
} else if tb_value < draw_score {
    NodeType::Alpha   // Loss: upper bound
} else {
    NodeType::Exact   // Draw: exact
};
```

---

### 🟡 Bug 3 — `enemy_king` tính xong bỏ đó

**File:** `eval.rs`, hàm `endgame_evaluate`

```rust
let our_pawns = board.color_piece_bb(side, PieceType::Pawn);
let enemy_king = Square::new(          // ← tính xong...
    (board.color_piece_bb(side.flip(), PieceType::King)).lsb()
);

if raw_eval > 0 {
    return raw_eval + 200;             // ← ...không dùng enemy_king ở đây
}
```

`enemy_king` được tính nhưng không dùng để check "square rule" như comment nói. Logic pawn endgame hiện tại chỉ đơn giản là `if raw_eval > 0 { +200 }` — rất crude và gây eval jump đột ngột.

---

### 🔴 Bug 4 — Quiet moves KHÔNG BAO GIỜ được score đúng (nghiêm trọng)

**File:** `movepick.rs`

Trace qua flow:

**Bước 1 — `CapturesInit`:** Generate toàn bộ moves (captures + quiets), gọi `score_captures` → captures được score 10,000,000+, quiets bị set thẳng `-20,000,000`.

**Bước 2 — `GoodCaptures`:** Mỗi lần gọi `get_next_scored_move()`, hàm này dùng selection sort và **luôn increment `self.cur`**, kể cả khi move bị `continue` vì là quiet. Kết quả: sau khi `GoodCaptures` xong, `self.cur = self.list.count` (đã đi qua hết mọi move).

**Bước 3 — `QuietsInit`:**
```rust
MovePickerStage::QuietsInit => {
    self.score_quiets(heuristics, board); // ← tính từ self.cur
    self.stage = MovePickerStage::Quiets;
    self.cur = 0;
}

fn score_quiets(&mut self, ...) {
    // "Optimization": chỉ score từ self.cur trở đi
    for i in self.cur..self.list.count {  // ← self.cur = list.count → loop rỗng!
```

`score_quiets` không score được bất cứ move nào vì loop từ `list.count` đến `list.count`.

**Bước 4 — `Quiets`:** Tất cả quiets vẫn mang score `-20,000,000` từ bước 1, fail check `score > -14000` → toàn bộ bị đẩy vào `bad_quiets` và chơi cuối cùng theo thứ tự ngẫu nhiên.

**Hệ quả:** Move ordering cho quiets hoàn toàn bị phá. Killer moves và countermove vẫn hoạt động, nhưng phần còn lại của quiet moves không được sắp xếp theo history. Đây là performance bug lớn nhất.

**Fix:** Bỏ "optimization", score từ 0:
```rust
fn score_quiets(&mut self, heuristics: &Heuristics, board: &Board) {
    let side = board.side_to_move;
    for i in 0..self.list.count {  // ← từ 0, không phải self.cur
        let m = self.list.moves[i];
        if !m.is_capture() && !m.is_promotion() {
            // ... score history, cont_history, gives_check bonus ...
        }
    }
}
```

---

### 🟡 Bug 5 — SMP thread randomization làm hỏng history scores

**File:** `movepick.rs`, hàm `score_quiets`

```rust
if self.thread_id > 0 {
    h += h.wrapping_mul(self.thread_id as i32 + 3) % 256;
}
```

Mục đích là thêm noise để helper threads explore khác nhau. Nhưng `wrapping_mul` với constant `thread_id + 3` trên giá trị âm trong Rust cho kết quả không predictable và không uniform. Thread 1 có `multiplier = 4`, history = -500 → `-500 * 4 % 256 = -2000 % 256 = -208`, thêm vào thành `-708`. Thread 2 có multiplier = 5 → `-500 * 5 % 256 = -244`, thành `-744`. Noise không uniform và có thể flip thứ tự move ordering đáng kể, không chỉ là "perturbation nhỏ" như ý định.

**Fix đúng cách hơn:**
```rust
if self.thread_id > 0 {
    h += (self.thread_id as i32 * 7 + 13) % 64; // small uniform noise
}
```

---
Còn 2 bugs nữa, nhỏ hơn nhưng vẫn ảnh hưởng đến performance:

---

### 🟡 Bug 6 — `best_move_nodes` tích lũy qua TẤT CẢ iterations

**File:** `search.rs`

```rust
// iterate():
self.best_move_nodes = 0;  // reset 1 lần duy nhất ở đầu

for d in start_depth..=max_depth {
    let nodes_before_iter = self.nodes;
    // ...
    // trong move loop ở ply==0:
    if ply == 0 && first_move {
        self.best_move_nodes += self.nodes - start_nodes; // cộng dồn mãi
    }

    // cuối mỗi depth:
    let effort = self.best_move_nodes as f64 / self.nodes as f64;
    if effort < 0.3 {
        self.timer.move_importance_high(); // extend thêm time
    }
}
```

Vấn đề: `best_move_nodes` tích lũy nodes của first root move từ **depth 1 đến depth hiện tại**. `self.nodes` cũng tổng tất cả depths. Nhưng các depth cạn (1, 2, 3...) ít nodes, deep depths nhiều nodes hơn nhiều. Kết quả là `effort` bị kéo bởi các iterations cũ, không phản ánh đúng tình trạng của iteration hiện tại.

**Fix:** Reset `best_move_nodes` đầu mỗi iteration:
```rust
for d in start_depth..=max_depth {
    let nodes_before_iter = self.nodes;
    self.best_move_nodes = 0;  // ← move vào đây
    // ...
}
```

---

### 🟡 Bug 7 — `opt_time` không bao giờ reset về base giữa các depth iterations

**File:** `time.rs`

Các hàm `update_pv`, `update_score`, `aspiration_fail`, `move_importance_high` đều nhân `opt_time` lên — và **không bao giờ reset về `base_opt_time`**. Qua 10 depth iterations:

- PV thay đổi ở depth 6 → `opt_time *= 1.5`
- Score drop ở depth 8 → `opt_time *= 2.0`
- Aspiration fail ở depth 9 → `opt_time *= 1.5`
- Kết quả: `opt_time` có thể là `3x` base dù chỉ cần extend 1 lần

`min(max_time)` giúp không vượt hard limit, nhưng engine có thể bị "stuck" dùng gần `max_time` mọi nước chỉ vì search không ổn định ở depth thấp.

**Fix:** Reset về base ở đầu mỗi depth iteration:
```rust
for d in start_depth..=max_depth {
    self.timer.opt_time = self.timer.base_opt_time; // ← reset fresh
    // ... rồi update_pv, update_score mới apply
}
```

---
Có thêm 1 bug nữa, và đây có lẽ là **bug ảnh hưởng performance nhiều nhất**:

---

### 🔴 Bug 8 — Debug assertions `compute_hash()` chạy trong mọi node production

**File:** `search.rs`

```rust
// Đầu mỗi node alpha_beta:
let true_hash = board.compute_hash();      // ← full board scan
if board.zobrist_key != true_hash { std::process::exit(1); }

// Sau make_move:
if board.zobrist_key != board.compute_hash() { ... }   // ← lần 2

// Sau unmake_move:
if board.zobrist_key != board.compute_hash() { ... }   // ← lần 3
```

`compute_hash()` iterate qua tất cả piece types × 2 colors × tất cả squares — tức là gần như scan toàn bộ board mỗi lần gọi. Được gọi **3 lần mỗi node**, kể cả trong bản release.

Với 5 triệu nodes/giây, đây là 15 triệu full board scans mỗi giây chỉ để verify hash. Số liệu thực tế: đây có thể đang làm engine chậm đi **30-50%** so với tiềm năng thật sự.

**Fix:**
```rust
// Bọc trong debug_assertions, hoặc xóa hẳn nếu đã tin incremental hash đúng:
#[cfg(debug_assertions)]
{
    let true_hash = board.compute_hash();
    debug_assert_eq!(board.zobrist_key, true_hash, "Hash drift at ply={}", ply);
}
```

Có **3 bugs trong NNUE**, một trong số đó phá hoàn toàn black perspective:

---

### 🔴 Bug 9 — `orient_sq` cho Black dùng `^ 63` thay vì `^ 56`

**File:** `feature.rs`

```rust
pub fn orient_sq(sq: Square, perspective: Color) -> usize {
    if perspective == Color::White {
        sq.0 as usize
    } else {
        (sq.0 ^ 63) as usize  // ← 180° rotation (flip rank VÀ file)
    }
}
```

Stockfish HalfKAv2_hm dùng:
```
sq ^ 56  →  chỉ flip rank (a1 thành a8, b1 thành b8, ...)
```

Code này dùng `^ 63` tức là flip rank **VÀ** file (a1 thành h8, b1 thành g8...). Đây là 180° rotation hoàn toàn khác. Mọi feature index cho black perspective đều sai, accumulator black đang tích lũy weights của ô vuông ngược hoàn toàn. Engine **không evaluate đúng từ góc nhìn của Black**.

**Fix:**
```rust
} else {
    (sq.0 ^ 56) as usize  // chỉ flip rank
}
```

---

### 🔴 Bug 10 — `psqt_bucket()` hardcode về 0

**File:** `inference.rs`

```rust
fn psqt_bucket(acc: &Accumulator, side: Color) -> usize {
    let _ = (acc, side); // suppress unused warnings ← tất cả input đều bị bỏ
    0  // luôn trả về bucket 0
}
```

PSQT trong HalfKAv2_hm có 8 buckets, chọn theo số quân trên bàn. Bucket sai → PSQT contribution hoàn toàn sai trong mọi position trừ full board.

**Fix:**
```rust
fn psqt_bucket(acc: &Accumulator, side: Color) -> usize {
    // Stockfish: bucket = (piece_count - 1) / 4, clamped to [0, PSQT_BUCKETS-1]
    // Đếm qua non-king pieces từ accumulator không tiện, nên truyền piece_count vào
    // hoặc tính trong evaluate() nơi Board còn accessible
    0 // cần refactor signature để pass piece_count
}
```

Thực tế cần sửa signature `evaluate()` để nhận thêm `piece_count: usize`:
```rust
let bucket = ((piece_count as usize).saturating_sub(1) / 4).min(PSQT_BUCKETS - 1);
```

---

### 🟡 Bug 11 — Vec heap allocation trong mỗi node evaluation

**File:** `inference.rs`

```rust
pub fn evaluate(side: Color, acc: &Accumulator, params: &NetworkParams) -> i32 {
    let ts = super::feature::TRANSFORMED_SIZE;  // 768

    let mut stm_crelu  = vec![0u8; ts];    // ← heap alloc 768 bytes
    let mut nstm_crelu = vec![0u8; ts];    // ← heap alloc 768 bytes
    let mut stm_sqr    = vec![0u8; ts];    // ← heap alloc 768 bytes
    let mut nstm_sqr   = vec![0u8; ts];    // ← heap alloc 768 bytes
    let mut l1_input   = vec![0u8; ts * 4]; // ← heap alloc 3072 bytes
```

5 Vec allocations mỗi node, thêm vào 3 lần `compute_hash()`. Với hàng triệu nodes/giây, allocator đang bị hammered liên tục.

**Fix:** Dùng fixed-size arrays trên stack vì `ts = 768` là compile-time constant:
```rust
let mut stm_crelu  = [0u8; 768];
let mut nstm_crelu = [0u8; 768];
let mut stm_sqr    = [0u8; 768];
let mut nstm_sqr   = [0u8; 768];
let mut l1_input   = [0u8; 768 * 4];
```

---

Phát hiện thêm **2 bugs quan trọng** — cả hai đều trong NNUE:

---

### 🔴 Bug 12 — `victim_pt` đọc SAU `make_move` (bug đã biết nhưng chưa fix)

**File:** `search.rs`, line 771 — sau `make_move` line 678

```rust
let undo = board.make_move(m);   // ← line 678: captured piece bị xóa khỏi board

// ... ~90 dòng code ...

// Stat score for LMR — line 771
let stat_score = if is_capture {
    let victim_pt = board.piece_on_sq[m.to_sq() as usize]  // ← piece đã bị xóa!
        .map(|p| p.piece_type()).unwrap_or(PieceType::Pawn);  // luôn trả về Pawn
```

Bạn đã document bug này trong `bugs_found.md` nhưng chưa fix. `stat_score` cho captures luôn dùng `victim_pt = Pawn` thay vì giá trị thật, làm LMR reduction sai cho mọi non-pawn capture.

**Fix:** Lưu `victim_pt` trước `make_move`:
```rust
let victim_pt = if is_capture {
    if m.flag() == Move::FLAG_EP { PieceType::Pawn }
    else { board.piece_on_sq[m.to_sq() as usize]
        .map(|p| p.piece_type()).unwrap_or(PieceType::Pawn) }
} else { PieceType::Pawn };

let undo = board.make_move(m);  // bây giờ mới make
```

---

### 🔴 Bug 13 — SIMD functions trong `simd.rs` không bao giờ được gọi

**File:** `simd.rs` vs `inference.rs`

`simd.rs` implement đầy đủ AVX2 cho:
- `linear_forward_avx2(input: &[u8; 512], ...)` — hardcode 512
- `clipped_relu_avx2(input: &[i16; 256], ...)` — hardcode 256
- `l2_forward_avx2(input: &[u8; 32], ...)` — hardcode 32

Nhưng `inference.rs` không `use` hay gọi bất cứ hàm nào từ `simd.rs` — nó tự implement scalar loops riêng với sizes thật (`TRANSFORMED_SIZE=768`, `L1_SIZE=16`, `L2_SIZE=32`). Kết quả là toàn bộ SIMD code là **dead code hoàn toàn**.

Ngoài ra sizes trong simd.rs sai luôn — L1 input thật là 3072, không phải 512. `clipped_relu_avx2` expect 256 nhưng accumulator là 768.

---

Xong. Đây là **bug cuối cùng** mình tìm được:

---

### 🔴 Bug 14 — Correction heuristics đọc nhưng không bao giờ được update

**File:** `search.rs` + `history.rs`

`history.rs` định nghĩa đầy đủ:
```rust
pub fn update_non_pawn_correction(...) { ... }
pub fn update_cont_correction(...) { ... }
```

Nhưng grep toàn bộ codebase — hai hàm này **không được gọi ở bất cứ đâu**. `search.rs` chỉ đọc:
```rust
let correction_value = {
    let mat_hash = board.non_pawn_material(side) as usize;
    self.heuristics.get_non_pawn_correction(side, mat_hash)  // ← luôn trả về 0
};
// correction_value / 131072 → luôn là 0
let corrected = (raw + correction_value / 131072).clamp(...);
```

`non_pawn_corr` được init bằng 0 và không bao giờ thay đổi. Toàn bộ correction machinery — được thiết kế để học sai lệch giữa static eval và search score — là dead code hoàn toàn.

---

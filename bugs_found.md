

### 🔴 Bug 19 — `is_pseudo_legal` thiếu occupancy check cho pawn single push

**File:** `src/board.rs`, line 141

```rust
if self.side_to_move == Color::White {
    if to.0 == from.0 + 8 {
        // ← THÂN HÀM RỖNG — không check ô đích có trống không!
    } else if to.0 == from.0 + 16 && from.rank() == 1 {
        if self.piece_on_sq[(from.0 + 8) as usize].is_some() { return false; }
```

Double push có check `piece_on_sq[from+8]` đúng. Nhưng single push `from+8` không check gì cả.

**Kịch bản trigger:** Có một TT move (pawn push e4) được lưu từ lần search trước. Ở position mới, ô e4 bị chiếm bởi quân khác. `is_pseudo_legal` trả về `true` → `make_move` được gọi → quân tại e4 bị overwrite (coi như captured) nhưng `captured_piece` trong undo sẽ ghi nhận, tuy nhiên `is_capture()` flag không được set → halfmove clock sai, zobrist sai, board state hỏng.

**Fix:**
```rust
if to.0 == from.0 + 8 {
    if self.piece_on_sq[to.0 as usize].is_some() { return false; }
}
// tương tự cho Black: to.0 == from.0 - 8
```

---

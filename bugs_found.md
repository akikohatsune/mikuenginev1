

### 🔴 Bug 15 — `feature_index_for_perspective` cho Black king dùng `^63` thay vì `^56`

**File:** `src/nnue/feature.rs`, `feature_index_for_perspective`

```rust
} else {
    // For black: flip the king square 180° first, then apply king mirror
    orient_king_sq(Square::new(ksq.0 ^ 63))  // ← SAI
};
```

Bạn đã fix `orient_sq` (Bug 9) từ `^63` → `^56` cho piece squares, nhưng **quên fix cùng chỗ** trong `feature_index_for_perspective` cho king square. `^63` flip cả rank lẫn file, `^56` chỉ flip rank. Stockfish HalfKAv2_hm dùng rank-flip only trước khi apply horizontal mirror.

**Fix:** `ksq.0 ^ 56`

Hậu quả: toàn bộ Black king bucket bị sai → mọi NNUE refresh cho Black perspective dùng sai bucket → eval không nhất quán giữa incremental và full refresh.

---

### 🟡 Bug 16 — `incremental.rs` là dead code (không phải bug chức năng, nhưng gây confusion)

`board.rs` gọi thẳng `feature_index_for_perspective` trong `make_move`, bỏ qua hoàn toàn các helpers trong `incremental.rs` (`quiet_move_deltas`, `capture_deltas`, `apply_deltas`, v.v...). Toàn bộ file `incremental.rs` không được dùng — giống `simd.rs` cũ. Không làm engine sai, nhưng nên xóa để tránh nhầm lẫn sau này.

---

Bug 15 là cái quan trọng nhất chưa fix — nó âm thầm làm Black NNUE buckets sai ngay cả sau khi fix Bug 9.
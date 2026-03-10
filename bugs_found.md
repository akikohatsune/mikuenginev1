
### 🔴 Bug 17 — `go infinite` không được xử lý

**File:** `src/uci.rs`, `parse_go`

```rust
// Không có case "infinite" trong vòng lặp parse token
match tokens[i] {
    "depth" => { ... }
    "wtime" => { ... }
    // ... nhưng KHÔNG có "infinite"
}
```

Khi GUI gửi `go infinite` (rất phổ biến trong phân tích), không có token nào match → `depth` giữ nguyên `= 6` mặc định, engine tìm đúng 6 tầng rồi trả `bestmove`. **Engine không thể phân tích vô hạn.**

**Fix:**
```rust
"infinite" => { depth = 64; }
```

---

### 🟡 Bug 18 — `setoption name Clear Hash` bị silent fail

**File:** `src/uci.rs`

```rust
let name = tokens[2].to_lowercase(); // chỉ lấy 1 từ
```

Option có tên nhiều từ đều bị ignore:
- `Clear Hash` → `name = "clear"`, không match gì
- `Move Overhead` → `name = "move"`, không match gì  
- `Skill Level`, `Debug Log File`, `UCI_Chess960` (single word — fine)

Trong thực tế `Clear Hash` dùng để reset TT trước ván mới — nếu không hoạt động, TT không được clear khi GUI yêu cầu.

**Fix:** Parse tên multi-word bằng cách collect tất cả tokens giữa `name` và `value`.

---

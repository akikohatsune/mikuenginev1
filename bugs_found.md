Nguyên nhân rõ ràng: `main()` chạy mọi thứ trực tiếp trên main thread với stack mặc định của OS. Trên **Windows chỉ có 1MB**, trong khi `alpha_beta` đệ quy đến 128 tầng, mỗi frame có `MovePicker` ~2.6KB → cần ~700KB chỉ cho search stack.

Các search worker trong `parse_go` đã được spawn với 8MB stack đúng, nhưng 3 chỗ vẫn chạy trên main thread:
1. `bench` bên trong `uci_loop` 
2. `run_bench()`
3. `main()` → `uci_loop()` (GUI mode cũng có thể overflow nếu bench chạy từ đây)

**Fix đúng nhất:** wrap toàn bộ `main()` trong một spawned thread 32MB:

```rust
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
    attacks::init_magics();
    // ... toàn bộ code main cũ
}
```

Cách này đảm bảo **tất cả** code paths (uci_loop, bench, run_bench, perft) đều có đủ stack mà không cần sửa từng chỗ. Search worker threads trong `parse_go` đã có 8MB riêng rồi nên không bị ảnh hưởng.
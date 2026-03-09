# MikuEngine v1

**MikuEngine** là một chess engine nhỏ được mình viết trong thời gian rảnh để thử nghiệm và học cách hoạt động của các chess engine hiện đại. Project này chủ yếu mang tính thử nghiệm, mục tiêu chính là hiểu rõ cách xây dựng một engine từ đầu thay vì chỉ sử dụng các engine có sẵn.

Engine được viết bằng **Rust**, sử dụng **bitboard** để biểu diễn bàn cờ và có tích hợp **NNUE (Efficiently Updatable Neural Network)** cho phần đánh giá vị trí. Ý tưởng và hướng phát triển được lấy cảm hứng từ các engine hiện đại như Stockfish.

---

## Mục đích của project

Project này được làm chủ yếu để:

- học cách hoạt động của chess engine  
- thử nghiệm các thuật toán search  
- tìm hiểu cách hoạt động của NNUE  
- cải thiện dần sức mạnh của engine qua từng phiên bản  

MikuEngine không hướng tới việc cạnh tranh với các engine mạnh hiện nay, mà thiên về việc khám phá và học hỏi cách các engine được xây dựng.

---

## Các thành phần chính

Hiện tại engine có một số thành phần cơ bản:

- **Bitboard board representation** để xử lý move generation nhanh  
- **Search algorithm** dựa trên alpha-beta  
- **Move generation** cho các quân cờ  
- **NNUE evaluation** để đánh giá vị trí  
- **UCI protocol** để có thể chạy với các GUI cờ vua  

Nhờ có UCI nên engine có thể chạy với GUI như **Cute Chess** để test hoặc chạy match.

---

## Tình trạng hiện tại

Đây vẫn là phiên bản **v1** và còn khá nhiều thứ chưa tối ưu. Engine chủ yếu được viết khi có thời gian rảnh nên code đôi khi vẫn còn khá đơn giản hoặc chưa được tối ưu tốt.

Tuy vậy engine đã có thể:

- chơi được các ván cờ hoàn chỉnh  
- chạy match với các engine khác  
- test thông qua GUI  

---

## Kế hoạch trong tương lai

Một số thứ mình muốn thử thêm trong các phiên bản sau:

- thêm **transposition table**  
- cải thiện **move ordering**  
- thử nghiệm các kỹ thuật pruning  
- tối ưu NNUE evaluation  
- cải thiện performance của search  
- tăng độ sâu search  

---

# kome!Diary

Một vài ghi chú nhỏ về quá trình phát triển engine.

## Bắt đầu project

Ban đầu project chỉ được viết để thử xem việc làm một chess engine thực sự khó tới mức nào. Lúc đầu engine chỉ có những thứ rất cơ bản như representation của bàn cờ và move generation.

Sau khi engine có thể tạo ra các nước đi hợp lệ, mình bắt đầu thử viết search algorithm để engine có thể chọn nước đi.

---

## Thử nghiệm search

Search ban đầu khá đơn giản và chỉ nhằm mục đích để engine có thể chơi được một ván cờ hoàn chỉnh. Sau đó mình bắt đầu thử cải thiện search để engine có thể nhìn sâu hơn.

Đây cũng là lúc mình nhận ra phần lớn sức mạnh của chess engine đến từ search và các tối ưu xung quanh nó.

---

## Thêm NNUE

Sau khi engine có thể chơi ổn định, mình thử tích hợp **NNUE** để xem liệu evaluation có thể cải thiện được hay không. Đây cũng là phần thú vị nhất của project vì nó kết hợp giữa chess engine và machine learning.

NNUE trong project hiện tại vẫn còn khá cơ bản và có thể sẽ được cải thiện thêm trong các phiên bản sau.

---

## Hiện tại

Hiện tại engine vẫn đang trong giai đoạn thử nghiệm. Mỗi khi có thời gian rảnh mình sẽ thêm một vài cải tiến nhỏ hoặc thử nghiệm các ý tưởng mới.

Project này chủ yếu là một cách để học hỏi và khám phá cách các chess engine hoạt động ở mức thấp.

---

## Ghi chú

Đây chỉ là một project cá nhân để học hỏi và thử nghiệm.  
Nếu bạn thấy project thú vị hoặc có góp ý thì cứ thoải mái mở issue hoặc pull request.

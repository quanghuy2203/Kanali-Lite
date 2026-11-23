# Kanali Lite

Ứng dụng điều khiển màn hình **Tryx Panorama** (USB `391A:1061`), viết bằng Rust, ưu tiên nhẹ RAM.

- `kanali.exe`: icon khay hệ thống + giao diện. Tiến trình nền không có cửa sổ, giao diện là tiến trình riêng, đóng cửa sổ là giải phóng RAM.

## Tính năng
- Giữ màn hình sáng (heartbeat), tự kết nối lại khi rút/cắm cáp hoặc máy ngủ dậy
- Chọn video đang phát, video khi bật nguồn và khi chờ
- Xem thông tin thiết bị (model, firmware, độ phân giải, serial)
- Tự khởi động cùng Windows
- Tự cập nhật từ artifact GitHub Actions (kiểm tra SHA-256 trước khi thay chương trình)

## Build
Cần Rust (MSVC) trên Windows.

```
cargo build --locked --release --bin kanali
```
File nằm ở `target\release\kanali.exe`.
Hãy đặt `kanali.exe` trong thư mục bạn có quyền ghi (ví dụ `%LOCALAPPDATA%\Kanali`) để tự cập nhật thay được file.

## Build trên GitHub Actions

Mỗi lần đẩy lên `main`, workflow **Windows GUI** chạy kiểm thử và tạo artifact chứa bản giao diện Windows x64. Tải artifact, giải nén và mở chương trình. Khi muốn phân phối bản cập nhật mới, tăng phiên bản trong Cargo.toml và đồng bộ Cargo.lock trước khi đẩy.

Từ v0.2.2, bộ cập nhật tra lượt build thành công của nhánh main qua GitHub API, tải artifact công khai qua nightly.link và đối chiếu SHA-256 của ZIP với digest do GitHub cung cấp. Bản v0.2.1 cần chuyển sang bản Actions bằng tay lần đầu. Thoát ứng dụng từ khay hệ thống rồi thay tệp cũ tại cùng đường dẫn để giữ thiết lập tự khởi động.

## Thư viện PANORAMA

Chức năng nhập và tạo xem trước cần FFmpeg cho Windows có libx264. Đặt `ffmpeg.exe` cạnh Kanali hoặc thêm vào PATH. Chọn **Thêm video / GIF** để chuyển mã thành H.264 cho PANORAMA. Khi tìm thấy bản KANALI gốc đã cài, Lite dùng bộ mã hóa MediaX của KANALI để tạo video toàn màn hình 2240 × 1080 ở 60 fps. Nếu thiếu MediaX hoặc bộ mã hóa gặp lỗi, Lite dùng FFmpeg và ghi tên bộ mã hóa trong thông báo nhập. Nếu clip nhập từ bản cũ khiến tản đứng hình, nhập lại từ nguồn và lưu bản mới. Giữ giao diện mở trong lúc chuyển mã. Khi tệp đã vào hàng chờ, tiến trình nền gửi theo khối qua USB, kiểm tra phản hồi và đọc lại danh sách trên màn hình. Bấm vào video trong thư viện để phát.

### Preset và Customization

Preset có các thẻ xếp lớp. Dùng mũi tên để xem, bấm **Lưu vào màn hình** để áp dụng. Lite lấy ảnh của sáu preset mặc định từ bản KANALI đã cài ở thư mục chuẩn của Windows. Video nhập qua Lite dùng preview tạo từ chính video. Với tệp chỉ có trên thiết bị, có thể ghép bản nguồn trên PC. Thiết bị cung cấp danh sách tệp qua USB.

Trong **Customization**, chọn Full Screen hoặc Screen Splitting. Với màn hình chia đôi, bấm từng ô và chọn media cho bên đó trước khi lưu. Chế độ phát gồm Single, Loop và Shuffle.

Khi bấm **Lưu vào màn hình**, Lite đọc cấu hình hiện tại, cập nhật phần đã chọn, rồi gửi lệnh kích hoạt. Nút **Chép chẩn đoán phát** sao chép tên video, chế độ phát và trạng thái đèn nền mà firmware trả về ở từng bước. Nếu tản đứng hình, lấy chẩn đoán ngay sau lần bấm Lưu đó.

### Thư viện Wallpaper Engine

Trong **Customization**, bấm **Mở thư viện Wallpaper Engine**. Cửa sổ riêng hiển thị ảnh bìa, tên và loại của từng dự án, có ô tìm kiếm và nút nhập. **Mở Wallpaper Engine để tải thêm** mở ứng dụng chính chủ. Khi tải xong, Lite tự cập nhật danh sách. Bật **Tự nhập wallpaper mới tải** để chuyển các mục mới vào hàng chờ USB khi cửa sổ còn mở. Web và Scene được ghi từ cửa sổ Wallpaper Engine chạy phía sau giao diện thành clip 8 giây, Video dùng tệp nguồn. Nếu bộ FFmpeg hiện tại không ghi được Scene, chọn **Cài công cụ ghi Scene**. Lite tải FFmpeg một lần (khoảng 105 MiB), kiểm tra SHA-256 rồi dùng bản này cho các lần nhập sau.

Trong thư viện PANORAMA, bấm **Xóa** trên thẻ media đã thêm rồi xác nhận. Nếu tệp đang phát, Lite chuyển về Cooling delivery và đợi màn hình xác nhận trước khi xóa. Nút **Khôi phục màn hình** bật đèn nền và phát lại preset này ở chế độ toàn màn hình. Bạn cũng có thể xóa preset tải về `download_46` nếu thiết bị cho phép ghi. Khi nhập wallpaper, Lite kiểm tra clip đã chuyển đổi trước khi gửi qua USB. Nếu clip chỉ có khung đen, Lite hủy nhập. Với clip có đoạn mở đầu tối, ảnh xem trước bắt đầu từ khung lấy mẫu có hình.

## Dữ liệu
Cấu hình và trạng thái nằm ở `%APPDATA%\kanali-rs\` (`config.json`, `state.json`). Nếu chương trình lỗi, xem `crash.log` trong thư mục đó.

### Kết nối với ứng dụng Wallpaper Engine

Trong **Customization → Wallpaper Engine**, chọn số màn hình PC (bắt đầu từ 0) rồi bấm **Kết nối / đọc wallpaper đang chạy**. Lite hỏi ứng dụng đang chạy bằng lệnh `getWallpaper`. Sau khi thấy tên wallpaper, bấm **Nhập vào PANORAMA** để lấy nội dung hiện tại. Đổi wallpaper trên desktop rồi bấm đọc lại để cập nhật lựa chọn. Lite tự khởi chạy Wallpaper Engine qua Steam nếu cần.


## Xử lý sự cố đứng hình
- Chuột phải icon Kanali ở khay hệ thống, chọn **Khôi phục màn hình (phát video gốc)** để về lại video mặc định.
- Nhật ký hoạt động ở `%APPDATA%\kanali-rs\kanali.log`. Khi cần hỗ trợ, gửi file này cùng nội dung nút **Chép chẩn đoán phát**.
- Tắt mọi chương trình khác đang giữ màn hình (KANALI gốc, `kanali-rs.exe` cũ chạy ngầm) trước khi dùng.

- Hai file đánh dấu để thử nghiệm trong `%APPDATA%\kanali-rs\`: `simple-play` (chỉ gửi lệnh phát tối giản) và `force-ffmpeg` (nhập video bằng FFmpeg thay vì MediaX).
- Từ v0.2.15, video nhập mới dùng tên ngắn tương thích bộ phát của firmware. Hãy nhập lại từ nguồn; các video tên dài đã chép lên màn hình trước đó không tự được đổi tên.

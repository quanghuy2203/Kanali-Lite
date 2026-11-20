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

Chức năng nhập và tạo xem trước cần FFmpeg cho Windows có libx264. Đặt `ffmpeg.exe` cạnh Kanali hoặc thêm vào PATH. Chọn **Thêm video / GIF** để chuyển mã thành H.264 cho PANORAMA. Giữ giao diện mở trong lúc chuyển mã. Khi tệp đã vào hàng chờ, tiến trình nền gửi theo khối qua USB, kiểm tra phản hồi và đọc lại danh sách trên màn hình. Bấm vào video trong thư viện để phát.

Chọn **Nhập từ Wallpaper Engine** và mở `project.json` trong thư mục dự án hoặc `steamapps/workshop/content/431960/<id>`. Dự án Video dùng tệp nguồn. Dự án Web và Scene cần Wallpaper Engine đang chạy. Kanali mở dự án trong một cửa sổ riêng và ghi 15 giây để phát lặp; hãy giữ cửa sổ này hiển thị trong lúc ghi. Nếu Steam nằm ở thư viện khác, chọn wallpaper64.exe khi được hỏi.

Khi chọn video, giao diện hiện tối đa bốn giây xem trước đã lưu trên máy. Với video có sẵn trên màn hình, chọn **Ghép video nguồn để xem trước** và chỉ tới bản sao trên máy. Thư viện USB hiện chỉ trả về thông tin tệp.

## Dữ liệu
Cấu hình và trạng thái nằm ở `%APPDATA%\kanali-rs\` (`config.json`, `state.json`). Nếu chương trình lỗi, xem `crash.log` trong thư mục đó.

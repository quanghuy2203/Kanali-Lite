# Kanali Lite

Ứng dụng điều khiển màn hình **Tryx Panorama** (USB `391A:1061`), viết bằng Rust, ưu tiên nhẹ RAM.

- `kanali.exe`: icon khay hệ thống + giao diện. Tiến trình nền không có cửa sổ, giao diện là tiến trình riêng, đóng cửa sổ là giải phóng RAM.

## Tính năng
- Giữ màn hình sáng (heartbeat), tự kết nối lại khi rút/cắm cáp hoặc máy ngủ dậy
- Chọn video đang phát, video khi bật nguồn và khi chờ
- Xem thông tin thiết bị (model, firmware, độ phân giải, serial)
- Tự khởi động cùng Windows
- Tự cập nhật từ GitHub Releases (kiểm tra SHA-256 trước khi thay chương trình)

## Build
Cần Rust (MSVC) trên Windows.

```
cargo build --locked --release --bin kanali
```
File nằm ở `target\release\kanali.exe`.
Hãy đặt `kanali.exe` trong thư mục bạn có quyền ghi (ví dụ `%LOCALAPPDATA%\Kanali`) để tự cập nhật thay được file.

## Phát hành và tự cập nhật
1. Sửa `version` trong `Cargo.toml` (ví dụ `0.2.1`) và commit.
2. Tạo tag trùng phiên bản rồi đẩy lên:
   ```
   git tag v0.2.1
   git push origin v0.2.1
   ```
3. GitHub Actions build và tạo bản phát hành gồm `kanali-windows-x64.exe` và file `.sha256`.
4. Ứng dụng đang chạy (bản build từ Actions) sẽ tự thấy bản mới trong vòng 6 giờ, hoặc ngay khi bấm "Kiểm tra cập nhật", rồi cập nhật bằng một nút.

Bản build trên máy bạn không biết kho GitHub nào để kiểm tra, nên ô "Kho GitHub" sẽ hiện trong giao diện để bạn nhập `tên-người-dùng/tên-repo`. Bản tải từ Releases đã gắn sẵn.

## Dữ liệu
Cấu hình và trạng thái nằm ở `%APPDATA%\kanali-rs\` (`config.json`, `state.json`). Nếu chương trình lỗi, xem `crash.log` trong thư mục đó.

# Windows Native Prerequisites For DevNest

## Mục tiêu

Tài liệu này chốt các prerequisite tối thiểu để phần native `src-tauri` của DevNest build được trên Windows.

## Tình trạng môi trường hiện tại

Theo kết quả xác minh gần nhất:

- Node.js: có
- npm: có
- cargo: có
- rustc: có
- MSVC C++ Build Tools: có
- Windows SDK libraries: có
- `PATH` của shell thường vẫn đang trỏ `link.exe` sang `D:\laragon\bin\git\usr\bin\link.exe`
- Repo đã có linker wrapper để `cargo check` vẫn pass dù `PATH` bẩn

## DevNest cần gì

### Bắt buộc

1. Rust toolchain `x86_64-pc-windows-msvc`
2. Visual Studio Build Tools với workload Desktop development with C++
3. Windows 10/11 SDK libraries

### Tối thiểu phải có các thành phần này

- `link.exe` của MSVC
- `kernel32.lib`
- `ntdll.lib`
- `userenv.lib`
- `ws2_32.lib`
- `dbghelp.lib`

## Cách tự kiểm tra

Chạy:

```powershell
npm run check:env
```

Nếu pass đầy đủ, script phải không còn báo `MISSING` cho:

- `MSVC toolchain root`
- `Windows SDK lib root`
- `PATH linker`

## Dấu hiệu môi trường đang sai

### Case 1: PATH dính linker của Git

Ví dụ sai:

```text
D:\laragon\bin\git\usr\bin\link.exe
```

Linker này không dùng được cho Rust target `x86_64-pc-windows-msvc`.

### Case 2: Có Rust nhưng không có SDK libs

Lúc `cargo check` sẽ fail với lỗi kiểu:

- `could not open 'kernel32.lib'`
- `could not open 'ntdll.lib'`

## Repo-level workaround hiện có

Repo đã có:

- `src-tauri/.cargo/config.toml`
- `src-tauri/.cargo/msvc-linker.cmd`

Mục tiêu là:

- bypass `link.exe` của Git/Laragon trong shell thường
- tự tìm `link.exe` của MSVC
- tự set `LIB` tới MSVC và Windows SDK libs trước khi link

Lưu ý:

- Workaround này không thay thế việc thiếu SDK/MSVC thật.
- Nếu máy đã có SDK/MSVC nhưng shell bẩn, wrapper này giúp `cargo check` chạy được ngay.

## Khi nào được coi là native-ready

Môi trường chỉ được coi là sẵn sàng khi:

1. `npm run check:env` báo `OK` cho `MSVC toolchain root`, `Windows SDK lib root`, và `Repo linker wrapper`
2. `cargo check` trong `src-tauri` pass từ shell thường
3. Tauri commands mẫu như `ping` và `get_boot_state` build được

## Gợi ý quy trình sau khi máy đã có build tools

1. Chạy `npm run check:env`
2. Chạy `cargo check` trong `src-tauri`
3. Chạy `npm run build`
4. Nếu cả ba pass, tiếp tục lock phần verify của Phase 00

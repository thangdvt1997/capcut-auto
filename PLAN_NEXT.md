# Kế hoạch tiếp theo — dựa trên `PROMPT_AUDIT.md` (2026-09-08)

Tài liệu này thay thế đánh giá "70-75% xong" trước đó (quá lạc quan) bằng một backlog chi tiết, đúng thực tế, ưu tiên theo rủi ro/kích thước. Nguồn dữ liệu: `PROMPT_AUDIT.md` (audit từng dòng, 25 mục tính năng của `promt.md`).

Nguyên tắc khi thực hiện (không đổi so với suốt dự án): audit trước khi code, không fake tính năng, mọi claim "xong" phải verify thật (compile + test + click-through), i18n song ngữ đầy đủ, không commit khi chưa verify.

---

## Nhóm A — Việc nhỏ, độc lập, có thể làm song song ngay (rủi ro thấp)

Mỗi việc dưới đây là 1 gap cụ thể, không phụ thuộc việc khác, phù hợp dispatch song song.

1. **Wire `<Toast />` vào App.svelte** — component đã xây xong (`Toast.svelte`/`toast.svelte.ts`) nhưng 0 nơi dùng thật. Chỉ cần: mount `<Toast />` trong `App.svelte`, và gọi `toastStore` cho ít nhất vài sự kiện quan trọng thật (vd: export CapCut thành công, lỗi kết nối AI nghiêm trọng) — đúng tinh thần §15 "Error quan trọng mới dùng Toast/Dialog".
2. **Thêm nút "Skip" cho batch job** — hiện chỉ có Retry/Cancel. Cần: 1 action mới cho job đang Queued (bỏ qua, không chạy, chuyển sang trạng thái Cancelled hoặc 1 trạng thái Skipped riêng) + command backend tương ứng nếu cần.
3. **"Open Output Folder" cho batch job** — hiện chỉ có tooltip hiện full path. Thêm 1 nút thật gọi `reveal_in_explorer`-shaped command (đã có pattern y hệt ở CapCut `reveal_capcut_draft_in_explorer`) cho `output_path` của job.
4. **"Clear Completed" cho batch queue** — 1 action đơn giản: lọc bỏ mọi job có status completed khỏi `jobsById`/hiển thị.
5. **Wire Test Voice preview thật** — `VoiceInfo.preview_url` đã có field, ghi rõ "chưa dùng ở đâu". Nếu provider trả về preview_url thật, thêm 1 nút play audio ngay trong danh sách voice ở `VoiceSettingsDialog`.
6. **GPU vào Status Bar** — thêm field GPU vào `LiveSystemStats` (dùng `sysinfo` hoặc info từ `hardware_encoders` đã có), hiển thị ở StatusBar.
7. **Sanitize filename/path đúng nghĩa** — mở rộng `fs_safety.rs::is_safe_path_component` (đã có, hẹp) thành 1 hàm sanitize filename/output-path rõ ràng hơn, dùng ở các điểm nhận input từ user (tên project, tên preset export...).
8. **Speed/Pitch/Volume/Emotion UI cho Voice** — field backend đã có (`VoiceSynthesisSettings`), chỉ thiếu UI. Thêm 4 slider/input vào `VoiceSettingsDialog`'s Generate Voice panel.

## Nhóm B — Việc vừa, cần thiết kế nhỏ trước khi code (rủi ro trung bình)

9. **AppSettings thống nhất** — hiện mỗi feature tự lưu `localStorage` riêng (aiSettings/voiceSettings/capcut/...). Cân nhắc: có thực sự cần gộp thành 1 `AppSettings` type không, hay giữ nguyên kiến trúc phân mảnh hiện tại (đã hoạt động tốt)? — đây là câu hỏi kiến trúc, nên hỏi lại trước khi làm nếu quyết định đầu tư.
10. **Throttle progress event** — đã xác định chính xác đường code (`ffmpeg/command.rs` → `batch/pipeline.rs` → `batch/manager.rs` → `app.emit()`). Thêm rate-limit (vd: tối đa 1 lần/200ms per job) ở điểm `on_progress` trong `manager.rs`, không đổi logic nghiệp vụ.
11. **Media query / breakpoint cho 3 độ phân giải** (1366×768/1920×1080/2560×1440) — kiểm tra thật bằng cách resize cửa sổ, thêm CSS breakpoint nếu phát hiện layout vỡ ở độ phân giải nhỏ nhất.
12. **Model dropdown thật cho AI Settings** — hiện là ô nhập text tự do. Cần: 1 command mới list model thật theo provider (OpenAI/Anthropic có API list models; Ollama có endpoint riêng) + đổi Input thành Select.
13. **Test Connection kết quả có cấu trúc (Connected/Latency/Model/Last Checked)** — mở rộng `AiConnectionTestResult`/`VoiceConnectionTestResult` thêm field `latency_ms`, `last_checked` (thời điểm test), giữ nguyên `message` cho backward-compat.
14. **Batch queue table — bổ sung cột + toolbar còn thiếu** — đây là gap lớn nhất của §4. Cần làm theo 2 bước nhỏ hơn:
    - 14a. Thêm multi-select (checkbox) + Start Selected/Retry Failed (bulk)/Delete/Clear Completed vào `BatchJobsDialog`.
    - 14b. Thêm cột Duration (từ media probe), Language (nếu track được), bỏ qua Thumbnail/Project/Subtitle/Translation/Voice nếu xét thấy không cần thiết cho use-case thật (batch xử lý video, không phải quản lý đa ngôn ngữ song song) — **cần hỏi lại mức độ cần thiết trước khi đầu tư lớn vào việc này**, vì đây là thay đổi UI diện rộng.

## Nhóm C — Việc lớn, cần quyết định phạm vi trước (rủi ro cao, nên hỏi trước khi dispatch)

15. **Multi-project library thật (Tab 3 "Projects")** — hiện app chỉ mở 1 project/lần + MRU list đơn giản. Xây 1 project manager thật (bảng Projects với Created/Videos/Completed/Failed/Output/Preset/Language + Open/Rename/Duplicate/Archive/Delete) là thay đổi kiến trúc lớn — cần quyết định: có thực sự cần đa dự án đồng thời, hay MRU hiện tại đã đủ dùng?
16. **Auto save (autosave định kỳ)** — hiện chỉ save thủ công atomic. Cần thiết kế: chu kỳ autosave bao lâu, lưu vào đâu (file riêng hay ghi đè), có xung đột với crash-recovery (`in_progress_jobs`, Phase D8a) không.
17. **True Job Resume** — đã biết từ trước, cần refactor `run_pipeline` để checkpoint từng stage. Rủi ro cao nhất, đã cố tình hoãn nhiều lần — vẫn giữ nguyên quyết định hoãn cho tới khi có chỉ đạo rõ.
18. **Rewrite Script + Sync Timeline thật** — 2 tính năng AI hoàn toàn mới trong dubbing pipeline, chưa audit sâu backend cần gì. Cần 1 pass audit riêng trước khi ước lượng size.
19. **Drag & drop cho batch queue** — hiện 0% (thậm chí có 1 chỗ chủ động tắt native drag). Thêm real DnD cho việc thêm video vào batch — độ ưu tiên thấp vì "Pick files…" đã hoạt động tốt, DnD chỉ là tiện ích thêm.

## Nhóm D — Cần làm rõ trước khi có thể ước lượng (chưa đủ thông tin)

- **Voice Profiles / Prompt Templates / CapCut Templates như category riêng trong Tab 3** — chưa rõ ý nghĩa thật khác biệt với những gì đã có (Voice Mapping, custom prompt cho translation, Templates ở Workspace). Có thể chỉ là đặt tên khác cho tính năng đã có, cần hỏi lại ý định thật của promt.md ở đây trước khi build trùng lặp.
- **API Mode / Thinking Level cho AI Config** — không rõ ý nghĩa cụ thể trong ngữ cảnh app này (model hiện tại không phải reasoning model kiểu OpenAI o-series/Claude thinking mode được cấu hình riêng) — cần làm rõ trước khi build.

---

## Đề xuất thứ tự triển khai

1. Dispatch song song toàn bộ **Nhóm A** (8 việc, độc lập, an toàn) — có thể chạy 4-8 agent song song ngay.
2. Sau khi Nhóm A xong + verify, dispatch **Nhóm B** (mục 9-14, trừ 9 và phần 14b cần hỏi lại) — làm mục 10, 11, 12, 13, 14a trước.
3. Với **Nhóm C**, hỏi lại quyết định phạm vi trước khi dispatch bất kỳ việc nào (đặc biệt #15, #16 — đều là quyết định kiến trúc thật, không phải kỹ thuật thuần tuý).
4. **Nhóm D** cần làm rõ ý định spec trước khi ước lượng — hỏi lại khi tới lượt.

Mọi việc hoàn thành đều phải: cập nhật `STUDIO_PLAN.md` (thêm phase mới), cập nhật `PROMPT_AUDIT.md` (đổi ❌/⚠️ thành ✅ khi thật sự xong và verify), verify qua WSL (Rust) + pnpm (frontend) + live click-through khi khả thi, rồi mới commit.

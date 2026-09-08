# Audit chi tiết: `promt.md` so với code thật (2026-09-08)

Đây là báo cáo audit nghiêm ngặt, từng dòng, so sánh chính xác từng field/button/cột/enum mà `promt.md` yêu cầu với trạng thái THẬT của code hiện tại — không dựa vào các đánh giá "done" cấp cao trước đó trong `STUDIO_PLAN.md`, mà đọc trực tiếp từng file nguồn.

**Vì sao báo cáo này tồn tại:** đánh giá "Full `promt.md` Audit" trước đó trong `STUDIO_PLAN.md` (và tóm tắt miệng "70-75% xong" đưa ra trong hội thoại) dựa trên kiểm tra cấp cao ("có dialog/tính năng này tồn tại không") chứ chưa đối chiếu từng field cụ thể. Khi kiểm tra tay 3 mục (§4, §7, §8) phát hiện gap thật mà audit trước bỏ sót, nên đã dispatch 4 agent audit lại TOÀN BỘ 25 mục tính năng, mỗi claim đều phải trích dẫn bằng chứng thật (đường dẫn file/dòng/tên field) — REAL / PARTIAL / ABSENT / N-A cho từng mục cụ thể.

**Kết luận tổng quát:** hầu như KHÔNG có mục nào của `promt.md` hoàn thiện 100% ở mức chi tiết literal. Chỉ 4/25 mục tính năng được xác nhận REAL hoàn toàn: **§2, §5, §13, §15, §19** (5 mục). 20 mục còn lại đều có ít nhất 1 gap thật, cụ thể, đã trích dẫn bằng chứng.

---

## §2 — Kiến trúc giao diện chính (3-tab)

**REAL, hoàn thiện.** `src/App.svelte:74-94` — đúng 3 tab (Workspace/Automation & AI/Project-Asset-License), mapping 1:1, nhãn từ i18n thật.

- Button hierarchy (Primary/Secondary/Danger/Ghost): REAL, dùng thật ở 21 file (`Button.svelte:17-45`).
- Dark theme: chỉ có 1 palette tối qua CSS variables, KHÔNG có cơ chế chuyển theme sáng/tối.
- Icon hệ thống nhất quán: ABSENT — dùng glyph unicode rời rạc (`×`, `✓`, `!`), không có `Icon.svelte`/sprite hệ thống.
- Tooltip: REAL (`Tooltip.svelte` tồn tại) nhưng chưa rõ mức độ phủ (nhiều nơi vẫn dùng `title=` native).
- Loading/skeleton state: REAL (`LoadingState.svelte`, spinner trong `Button.svelte`).
- **Toast notification: xây xong nhưng KHÔNG dùng ở đâu cả.** `Toast.svelte` tồn tại đầy đủ, nhưng doc comment của chính nó xác nhận: *"Not currently used anywhere... a later pass mounts one `<Toast />` in App.svelte."* `App.svelte` không có `<Toast />`. `grep toastStore` toàn repo chỉ ra 3 file, không có nơi gọi thật nào ngoài demo.
- Confirm dialog cho destructive action: REAL nhưng là pattern "bấm lần 2 để xác nhận" tự chế lặp lại ở mỗi dialog riêng lẻ (21 file), KHÔNG có `ConfirmDialog` component dùng chung — `Modal.svelte` tự ghi "chưa gắn vào dialog nào".

---

## §4 — Video Queue / Batch Processing

**Gap lớn nhất tìm được.** File thật: `src/components/batch/BatchJobsDialog.svelte`.

### Cột bảng thật vs yêu cầu (14 cột)
| Cột yêu cầu | Trạng thái |
|---|---|
| ID | ❌ ABSENT |
| Thumbnail | ❌ ABSENT |
| Project | ❌ ABSENT |
| Video | ⚠️ PARTIAL (có "Name", không có cột tên "Video") |
| Duration | ❌ ABSENT |
| Subtitle | ❌ ABSENT |
| Translation | ❌ ABSENT |
| Voice | ❌ ABSENT |
| Language | ❌ ABSENT |
| Current Task | ⚠️ PARTIAL (có "Stage", không đúng tên) |
| Progress | ✅ REAL |
| Status | ✅ REAL |
| ETA | ✅ REAL |
| Actions | ✅ REAL |

Chỉ 4/14 cột REAL đúng nghĩa, 2 PARTIAL, 8 ABSENT hoàn toàn.

### Toolbar (12 nút yêu cầu)
Thật tế chỉ có: batch-picker Select, "Refresh", `WorkerPoolWidget`, `WorkerPoolSizeControl`, **"Start New Batch…"** — và per-row Pause/Resume/Cancel/Retry (không phải toolbar toàn cục).

- Add Video: ❌ ABSENT (StartBatchDialog có "Pick files…" chung chung, không phải nút "Add Video" riêng)
- Add Folder: ❌ ABSENT (không có `directory: true` ở đâu cả)
- Import SRT: ❌ ABSENT (đã xác nhận không có code parse SRT nào trong toàn repo)
- Start Selected: ❌ ABSENT (không có checkbox/multi-select trong dialog này)
- Start All: ❌ ABSENT
- Pause/Resume: ⚠️ PARTIAL (chỉ per-row, không có nút toàn cục)
- Retry Failed: ⚠️ PARTIAL (chỉ per-row "Retry", không có bulk "Retry Failed")
- Stop: ❌ ABSENT (chỉ per-row Cancel)
- Delete: ❌ ABSENT (không có action xoá job nào)
- Clear Completed: ❌ ABSENT
- Open Output Folder: ❌ ABSENT (chỉ có tooltip hiện full path, không có nút mở folder)

### Status enum
Thật: `queued/analyzing/transcribing/editing/rendering/paused/completed/failed/cancelled` (9 giá trị, chi tiết hơn).
Yêu cầu: `READY/QUEUED/PROCESSING/PAUSED/FAILED/COMPLETED/CANCELLED` (7 giá trị).
→ READY ABSENT; PROCESSING tách thành 4 giá trị chi tiết hơn (analyzing/transcribing/editing/rendering) thay vì 1 giá trị chung.

### Multi-select / filter / search / drag-drop
- Multi-select: ❌ ABSENT trong dialog này
- Batch operations: ❌ ABSENT (theo multi-select)
- Drag/drop video: ❌ ABSENT (chỉ có ở Media Library, không liên quan queue)
- Search: ❌ ABSENT
- Filter theo Status/Language/Project/Processing-step: ❌ ABSENT hoàn toàn (0 filter control nào)
- Sort: ✅ REAL (DataTable client-side sort theo cột) — nhưng khác bản chất so với "filter theo Status/Language..." mà promt.md yêu cầu

---

## §5 — Concurrency / Slot Processing

**REAL, hoàn thiện.**
- "Max concurrent videos: [N]" — REAL, editable, validated (1-16), persisted, restart-to-apply theo thiết kế có chủ đích (`WorkerPoolSizeControl.svelte`, `batch/settings.rs`).
- Queue tự lấy job tiếp theo khi worker rảnh — REAL (`pop_blocking`/`spawn_worker_pool`, `manager.rs:647-1028`).
- Default pool size: 3.

---

## §6 — Tab 2 sidebar categories (11 category yêu cầu)

Thật tế: `AutomationSettingsTab.svelte` chỉ có 6 card: **AI, CapCut, Automation(rules), Update, Voice, Presets** — bố cục dạng lưới card, KHÔNG phải sidebar.

| Category yêu cầu | Trạng thái |
|---|---|
| AI | ✅ REAL |
| Translation | ❌ ABSENT (tự ghi rõ trong code: chưa có dialog/store nào) |
| Voice | ✅ REAL |
| Subtitle | ❌ ABSENT |
| Video | ❌ ABSENT |
| CapCut | ✅ REAL |
| Browser Automation | 🚫 N/A — xác nhận app này KHÔNG có Selenium/WebDriver/Playwright/Puppeteer nào (grep toàn repo = 0 kết quả) |
| Render | ❌ ABSENT (là card riêng) |
| Performance | ❌ ABSENT (tự ghi rõ trong code) |
| Storage | ❌ ABSENT (tự ghi rõ trong code) |
| Advanced | ❌ ABSENT |

Chỉ 3/11 category khớp tên thật (AI, CapCut, Voice) + 3 category khác app có nhưng promt.md không liệt kê (Automation rules, Update, Presets).

---

## §7 — AI Configuration

Struct thật (`AiProviderSettings`, `commands/ai.rs:69-78`): `provider, base_url, model, temperature, timeout_ms, credential_ref` — hết, không có gì khác.

| Field yêu cầu | Trạng thái |
|---|---|
| Provider | ✅ REAL |
| Model | ⚠️ PARTIAL — ô nhập text tự do, KHÔNG phải dropdown lấy danh sách model thật từ provider |
| Thinking Level (Low/Medium/High) | ❌ ABSENT (0 kết quả grep) |
| API Mode | ❌ ABSENT (0 kết quả grep) |
| Temperature | ✅ REAL |
| Timeout | ✅ REAL |
| Retry | ❌ ABSENT (không có field retry-count cho AI settings) |
| Max concurrent requests | ❌ ABSENT (chỉ có worker-pool cho batch, không phải AI-request concurrency) |
| Test Connection button | ✅ REAL |
| Kết quả Test hiển thị Connected/Latency/Model/Last Checked có cấu trúc | ❌ ABSENT — thật tế chỉ có 1 cặp `{success: bool, message: string}`, không có latency/model/last-checked riêng |

---

## §8 — Translation / Script Settings

Struct thật (`TranslationSettings`, `ai/translate.rs:155-170`): genre, translation_style, character_context, preserve_names, preserve_terminology, profanity_handling, sentence_length_optimization, voice_friendly_rewrite, system_prompt_prefix.

| Field yêu cầu | Trạng thái |
|---|---|
| Source Language | ✅ REAL — tách biệt thật với Target Language (đã sửa lại đánh giá sai trước đó) |
| Target Language | ✅ REAL |
| Genre (8 giá trị) | ✅ REAL — khớp đúng 1:1 tuyệt đối với danh sách promt.md (Drama/Romance, Fantasy/Cultivation, Crime/Detective, Police Bodycam, Prison/Crime, Survival, Documentary, Custom) |
| Translation Style | ✅ REAL |
| Character Context | ✅ REAL |
| Preserve Names | ✅ REAL |
| Preserve Terminology | ✅ REAL |
| Profanity handling | ✅ REAL (3 giá trị tự định nghĩa: Preserve/Soften/Remove) |
| Sentence length optimization | ✅ REAL |
| Voice-friendly rewrite | ✅ REAL |
| Tạo custom prompt | ⚠️ PARTIAL — field `system_prompt_prefix` có ở backend nhưng frontend LUÔN gửi `null`, không có UI nhập nào |
| Prompt Template Editor | ❌ ABSENT hoàn toàn (0 kết quả grep, chính code tự ghi "ngoài phạm vi") |

---

## §9 — Voice / TTS Configuration

| Field yêu cầu | Trạng thái |
|---|---|
| Voice Provider | ✅ REAL (3 loại, chỉ custom_api có backend thật) |
| Server/API URL | ✅ REAL |
| API Key (mask, không log) | ✅ REAL |
| Male/Female/Narrator Voice (field riêng) | ❌ ABSENT theo đúng nghĩa đen — **nhưng là lựa chọn kiến trúc có chủ đích**: dùng role-mapping linh hoạt (`VoiceRoleMapping{role, voiceId, voiceName}`, role là text tự do) thay vì field cứng, đã ghi rõ lý do trong code |
| Speed | ⚠️ PARTIAL — field backend có thật (`VoiceSynthesisSettings.speed`) nhưng **không có UI chỉnh**, hard-code = 1 |
| Pitch | ⚠️ PARTIAL — như trên |
| Volume | ⚠️ PARTIAL — như trên |
| Emotion | ⚠️ PARTIAL — như trên |
| Language (như 1 synthesis setting) | ⚠️ PARTIAL — chỉ hiện per-voice (danh sách), không phải setting để chỉnh |
| Test Voice button | ⚠️ PARTIAL — không có nút tên "Test Voice" đúng nghĩa, nhưng có nút "Generate" tương đương chức năng (D16, Generate Voice) |
| Refresh Voices button | ✅ REAL |
| Check Credits button | ❌ ABSENT hoàn toàn (0 kết quả grep) |
| Voice Mapping (role→voice linh hoạt) | ✅ REAL — chính là thiết kế thật của app |

---

## §10 — CapCut Integration

| Mục yêu cầu | Trạng thái |
|---|---|
| CapCut path | ⚠️ PARTIAL — chỉ có trong SystemInfo (đọc), không phải setting field riêng trong CapCut Settings |
| Project path | ❌ ABSENT |
| Draft path | ✅ REAL |
| Template project | ❌ ABSENT |
| Export path | ⚠️ PARTIAL — có `targetPath` tính toán được, không phải field tự chỉnh |
| Auto create project | ❌ ABSENT |
| Auto import video/audio/subtitle | ❌ ABSENT |
| Auto align timeline | ❌ ABSENT |
| Auto apply effects | ❌ ABSENT |
| Auto save | ❌ ABSENT |
| Auto export | ❌ ABSENT (luôn thao tác thủ công) |
| Detect CapCut | ✅ REAL |
| Open CapCut | ✅ REAL (chỉ mở app, không mở đúng project — đã thử thật và xác nhận CapCut không hỗ trợ) |
| Open Current Project | ❌ ABSENT có chủ đích, thay bằng "Reveal in Explorer" |
| Create Draft | ✅ REAL |
| Sync Draft | ❌ ABSENT (không có sync tăng dần, chỉ ghi đè toàn bộ) |
| Validate Draft | ✅ REAL |
| Export Project | ✅ REAL |
| Kiến trúc 3 service tách biệt (CapCutService/ProjectService/DraftService) | ⚠️ PARTIAL — có tách theo module thật (detect/validate/export/adapter) nhưng không theo đúng 3 tên gọi này |

---

## §11 — Video Processing Settings

| Mục yêu cầu | Trạng thái |
|---|---|
| Remove original voice | ❌ ABSENT hoàn toàn (0 kết quả grep toàn repo) |
| Keep background audio | ❌ ABSENT hoàn toàn (0 kết quả grep toàn repo) |
| Noise reduction | ⚠️ PARTIAL — field backend có thật (`AudioClipSettings.noise_reduction`) nhưng **0 UI nào** dùng nó |
| Normalize audio | ⚠️ PARTIAL — như trên (`AudioClipSettings.normalize`) |
| Background music + volume | ✅ REAL (field template-authoring, áp dụng thật trong pipeline) |
| Zoom | ✅ REAL |
| Pan | ⚠️ PARTIAL — có module backend riêng nhưng chưa xác nhận có UI riêng biệt với Zoom |
| Crop | ⚠️ PARTIAL — có field backend/type thật (`ClipCrop`) nhưng **0 UI Svelte nào** dùng (trừ auto-reframe tự động, khác cơ chế) |
| Aspect Ratio (field riêng) | ⚠️ PARTIAL — ẩn trong preset render, không có dropdown riêng |
| Resolution | ✅ REAL |
| FPS | ✅ REAL |
| Subtitle burn-in (toggle riêng) | ⚠️ PARTIAL — là 1 phần không tách rời của bước Render, không có toggle bật/tắt riêng |
| Intro/Outro | ✅ REAL |
| Watermark | ✅ REAL |
| 5 preset đặt tên (TikTok/YouTube Shorts/YouTube/Facebook Reel/Original) | ✅ REAL — khớp đúng cả 5, có thêm 4 preset khác ngoài yêu cầu |

---

## §12 — Tab 3: Project / Asset / License Management

**Gap kiến trúc lớn.** App này về bản chất là single-document editor (1 project tại 1 thời điểm), KHÔNG phải project manager đa dự án.

| Category yêu cầu (8) | Trạng thái |
|---|---|
| Projects (thư viện đa dự án) | ❌ ABSENT hoàn toàn — không có bảng Projects nào trong Tab 3, kể cả Recent Projects (MRU list) cũng KHÔNG hiển thị ở đây (chỉ ở TopBar File menu) |
| Presets | ⚠️ Nằm ở Tab 2, không phải Tab 3 |
| Voice Profiles | ❌ ABSENT hoàn toàn (0 kết quả grep) |
| Prompt Templates | ❌ ABSENT hoàn toàn (0 kết quả grep) |
| CapCut Templates | ❌ ABSENT như 1 category riêng biệt (có hệ Templates khác, ở Workspace LeftPanel, không phải "CapCut Templates" trong Tab 3) |
| Output History | ⚠️ PARTIAL — hợp nhất chung 1 bảng History, không tách 2 |
| Render History | ⚠️ PARTIAL — như trên, cùng 1 bảng |
| License | ❌ ABSENT có chủ đích (honest gap, đã bạn xác nhận giữ nguyên) |

**Field cho mỗi Project entry (Name/Created At/Videos/Completed/Failed/Output/Preset/Language) và action (Open/Rename/Duplicate/Archive/Delete):** hoàn toàn ABSENT — `RecentProjectEntry` (MRU list) chỉ có 3 field (`path/name/lastOpenedAt`), không có Rename/Duplicate/Archive action nào ở bất kỳ đâu trong codebase.

---

## §13 — Professional Dashboard Header

**REAL, hoàn thiện.** Mọi field (Project/Queue/Workers/Done/Failed/AI Status/Voice Status) đều trace về store/command thật, không fabricate.

---

## §14 — Status Bar

REAL cho CPU/RAM/FFmpeg/CapCut/AI API/Voice API/dòng "Current job".

**GPU: ABSENT hoàn toàn.** Struct `LiveSystemStats` chỉ có 4 field (`cpu_usage_percent, ram_usage_percent, used_memory_bytes, total_memory_bytes`) — không có field GPU nào. `SystemInformation` (dialog riêng) có `hardware_encoders`/`active_encoder_label` nhưng đây là danh sách encoder cứng hỗ trợ, không phải "GPU model/usage" thật, và không được StatusBar sử dụng.

---

## §15 — Log / Activity Panel

**REAL, hoàn thiện.** INFO/WARNING/ERROR, collapsible, filter, timestamp — đúng cơ chế. Nội dung ví dụ cụ thể trong promt.md ("Subtitle extraction completed"...) không tồn tại nghĩa đen vì app chưa có pipeline dubbing, nhưng cơ chế log thật dùng đúng shape (chuyển trạng thái stage thật).

---

## §16 — Error Handling

| Loại lỗi yêu cầu | Trạng thái |
|---|---|
| AI timeout | ⚠️ PARTIAL — gộp vào `RequestFailed` chung, không tách riêng timeout |
| Voice API timeout | ⚠️ PARTIAL — như trên |
| CapCut unavailable | ✅ REAL (`ExecutableNotFound`) |
| FFmpeg failure | ⚠️ PARTIAL/ABSENT — không có `FfmpegError` enum riêng dù có comment gợi ý sẽ có; lỗi ffmpeg lẫn vào các enum khác (`MediaError`, `RenderError`, `BatchError`) dạng string thô |
| Invalid subtitle | ❌ ABSENT như 1 category riêng (có validate cho AI-translation output, khác mục đích) |
| Disk full | ❌ ABSENT — không detect ENOSPC, chỉ có generic `*WriteFailed`/`*IoFailed` |
| Output exists | ❌ ABSENT hoàn toàn |
| Browser/Selenium crash | 🚫 N/A — app này không có component browser-automation nào |
| Retry action | ✅ REAL |
| Cancel action | ✅ REAL (two-step confirm) |
| **Skip action** | ❌ **ABSENT hoàn toàn** — chỉ có Retry/Cancel, không có Skip riêng biệt trong batch UI |
| 1 job fail không dừng queue | ✅ REAL, có test riêng chứng minh (`one_failing_video_template_pair_in_a_multi_template_batch_does_not_abort_the_others`) |

---

## §17 — Job State / Resume

**Không đổi so với đánh giá trước** — chỉ crash *detection* (Phase D8a), KHÔNG phải resume thật. Retry luôn chạy lại từ đầu (Queued/stage 0), vì `run_pipeline` không lưu checkpoint trung gian nào — project state đang xử lý chỉ tồn tại trong biến stack cục bộ, mất hoàn toàn khi crash.

---

## §18 — Preset System

**REAL cho AI/Translation/Voice/Render(ref)/CapCut(tối thiểu).** Xác nhận lại đúng các gap đã ghi trong `STUDIO_PLAN.md`:
- Subtitle config: ❌ ABSENT (xác nhận đọc trực tiếp interface, không có field)
- Video processing config: ❌ ABSENT (xác nhận tương tự)
- Đảm bảo không lộ secret: ✅ REAL, đảm bảo cấu trúc (không chỉ lời hứa) — cả phía TS type lẫn phía Rust export/import (schema-free byte pass-through, backend không hề đọc hiểu JSON).

---

## §19 — UI Design System

**REAL, hoàn thiện.** Toàn bộ 19 component tên trong promt.md đều tồn tại file thật + 3 component thêm (NumberInput/RadioGroup/SuccessBanner).

**ContextMenu: xây xong nhưng 0 nơi dùng thật** (chỉ trong demo).

---

## §20 — Color / Status System

Chỉ 3/11 tên token khớp nghĩa đen (`--primary`, `--surface`, `--border`). 8 token còn lại (success/warning/danger/info/surfaceElevated/textPrimary/textSecondary/textMuted) có tương đương thật nhưng dùng tên khác (`--pos`/`--warn`/`--neg`/`--accent`/`--elevated`/`--foreground`/`--muted`/`--muted-2`) — là lựa chọn có chủ đích, ghi rõ trong code.

---

## §21 — Responsive Desktop Layout

`ResizableSplit` REAL, dùng thật ở Workspace/Timeline — nhưng **KHÔNG dùng trong bảng batch/queue**. **Hoàn toàn KHÔNG có media query nào** trong toàn bộ codebase (0 kết quả cho `@media`, cho cả 3 độ phân giải 1366/1920/2560). Chưa từng test thật ở 3 độ phân giải cụ thể.

---

## §22 — UX Improvements (13 mục, kiểm từng mục)

| Mục | Trạng thái |
|---|---|
| Keyboard shortcuts | ⚠️ PARTIAL — chỉ có ở 4 chỗ (Timeline, WorkspaceSimple, SyncGroupDialog, Modal/ContextMenu Escape), không có registry toàn cục |
| Context menu | ❌ ABSENT (xây xong, chưa gắn — xem §19-20) |
| Drag & drop | ❌ **ABSENT hoàn toàn** — kể cả kết quả "draggable" duy nhất tìm thấy trong Timeline là để TẮT native drag, ngược lại hoàn toàn với yêu cầu |
| Multi-select | ✅ REAL (Timeline clips + Script Editor captions, 2 nơi độc lập) |
| Batch edit | ⚠️ PARTIAL — chỉ có merge/delete hàng loạt, không sửa field tuỳ ý hàng loạt |
| Search | ⚠️ PARTIAL — REAL ở Media Library, ABSENT ở Asset Library/History |
| Filter | ⚠️ PARTIAL — REAL ở Media Library (theo kind), ABSENT ở Asset Library/History (chỉ filter JS nội bộ, không phải control cho user) |
| Sorting | ✅ REAL |
| Auto save | ❌ **ABSENT, tự ghi rõ trong code** — chỉ có save thủ công atomic, không có autosave định kỳ nào |
| Undo/Redo | ✅ REAL, dùng chung 1 hệ thống (không nhân đôi cho Simple mode) |
| Recent projects | ✅ REAL (TopBar File menu) |
| Remember window size | ✅ REAL (`tauri-plugin-window-state`) |
| Remember panel sizes | ✅ REAL (`ResizableSplit` + localStorage) |

---

## §23 — Performance

- Async/background cho FFmpeg/AI/TTS/CapCut: ✅ REAL (`spawn_blocking` ở 10 file backend)
- Không full-table re-render khi 1 job đổi progress: ⚠️ PARTIAL/mặc định framework (Svelte 5 runes cho fine-grained reactivity tự nhiên, không có optimization riêng nào được xây thêm)
- **Throttle progress event: ❌ ABSENT, xác nhận với đường dẫn code cụ thể.** Đã trace toàn bộ chuỗi: ffmpeg progress parser (`ffmpeg/command.rs`) → `batch/pipeline.rs`'s render callback → `batch/manager.rs`'s `on_progress` → `app.emit()` không điều kiện, không rate-limit, không debounce ở bất kỳ điểm nào trong chuỗi.

---

## §24 — Security

- API key không hardcode, lưu Windows Credential Manager thật (`keyring` crate, `CredWriteW`/`CredReadW`/`CredDeleteW`): ✅ REAL
- Mask trên UI: ✅ REAL
- Không log key: ✅ REAL (0 kết quả grep key trong logging)
- **Sanitize filename/path riêng:** ⚠️ PARTIAL — có tìm thấy 1 hàm thật (`fs_safety.rs::is_safe_path_component`, được dùng ở nơi gọi thật) nhưng hẹp hơn yêu cầu literal "sanitize filename" của promt.md — không phải hoàn toàn 0% như đánh giá lần đầu.

---

## §26 — Data Model

**Lần đầu tiên được audit riêng.**

`VideoJob` (yêu cầu) so với `BatchJob` (thật, `batch/types.rs:71-97`):
| Field yêu cầu | Trạng thái |
|---|---|
| id | ✅ REAL |
| projectId | ❌ **MISSING** — job không link ngược về `ProjectV1.id` |
| sourceFile | ⚠️ PARTIAL (`name` là tên hiển thị, không phải full path) |
| outputFile | ✅ REAL (`output_path`) |
| status | ✅ REAL |
| progress | ✅ REAL |
| currentStep | ✅ REAL (`stage`) |
| createdAt | ❌ **MISSING** — bị ghi đè ngay khi job bắt đầu chạy, không giữ lại |
| startedAt | ✅ REAL |
| completedAt | ❌ **MISSING trên `BatchJob`** — chỉ có ở `HistoryEntry` (bản ghi sau khi xong), không có trên job đang chạy |
| error | ✅ REAL |
| retryCount | ❌ **MISSING trên `BatchJob`** — chỉ có ở `HistoryEntry`, không gửi về frontend lúc đang xử lý |

`AppSettings` (thực thể thống nhất): ❌ **ABSENT hoàn toàn** — settings phân mảnh theo từng feature riêng (mỗi store `localStorage` riêng biệt), không có 1 type `AppSettings` chung nào (0 kết quả grep cả backend lẫn frontend).

`ProcessingWorker` (record riêng cho từng worker): ⚠️ PARTIAL — chỉ có snapshot tổng (`WorkerPoolStatus{workers,running,queued}`), không có record cho từng worker riêng lẻ.

---

## §31 — Acceptance Criteria (kiểm lại từng mục)

1. UI chuyên nghiệp hơn — PARTIAL (Design System REAL, nhưng Toast built-unused, không icon system nhất quán)
2. 3 tab kiến trúc rõ — ✅ REAL
3. Video preview + script editor dễ dùng — PARTIAL/REAL (không đánh giá UX chủ quan được từ code tĩnh)
4. Batch queue hoạt động — ✅ REAL
5. Pipeline status hiển thị rõ — ✅ REAL
6. Có concurrency worker/slot — ✅ REAL
7. AI/Voice/CapCut config tổ chức lại — ✅ REAL
8. Có preset — ✅ REAL (nhưng thiếu Subtitle/Video-processing config, xem §18)
9. Có progress/error/retry — ✅ REAL
10. App restart resume được nếu kiến trúc hỗ trợ — ⚠️ PARTIAL, xác nhận có chủ đích chỉ làm detection
11. UI không freeze khi xử lý — ✅ REAL (backend), nhưng rủi ro thật từ progress-event không throttle (§23)
12. Không mất chức năng hiện có — ✅ REAL, xác nhận Advanced mode giữ nguyên byte-for-byte

---

## Tổng kết mức hoàn thiện thật theo section

**100% REAL (5/25):** §2, §5, §13, §15, §19

**PARTIAL — có ít nhất 1 gap thật (20/25):** §4, §6, §7, §8, §9, §10, §11, §12, §14, §16, §17, §18, §20, §21, §22, §23, §24, §26, §31 (và §3, đã audit riêng trước đó — xem `STUDIO_PLAN.md`)

**N/A xác nhận (không phải gap):** phần "Browser/Selenium crash" trong §16 và "Browser Automation" trong §6 — app này không có kiến trúc browser-automation, các mục này không áp dụng.

Đây là dữ liệu nguồn cho kế hoạch chi tiết ở `PLAN_NEXT.md`.

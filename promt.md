# TASK: Nâng cấp Tool Auto Video / CapCut – Workflow + UI/UX Professional

Bạn đang làm việc trên CODEBASE HIỆN TẠI của tool xử lý video.

Mục tiêu:
Nâng cấp tool hiện tại thành một ứng dụng desktop chuyên nghiệp để quản lý workflow:

Import Video
→ Tách Sub / Speech-to-Text
→ Dịch nội dung
→ Chỉnh sửa Script
→ Tạo Voice
→ Đồng bộ Timeline
→ Xử lý Video
→ Render
→ Export

QUAN TRỌNG:
- KHÔNG viết lại toàn bộ project nếu không cần thiết.
- Trước tiên phải đọc và phân tích codebase hiện tại.
- Tận dụng architecture/component/service hiện có.
- Refactor những phần cần thiết.
- Không làm mock UI đơn thuần.
- Các control phải được nối với logic/backend hiện có hoặc thiết kế interface rõ ràng để tích hợp.
- Giữ tương thích với các chức năng hiện tại.
- UI mới phải chuyên nghiệp, dễ sử dụng khi xử lý hàng chục/hàng trăm video.
- Không copy UI trong ảnh một cách máy móc. Ảnh chỉ là reference về chức năng.
- Ưu tiên UX giống một Video Processing Studio / Content Automation Dashboard hiện đại.

==================================================
1. PHÂN TÍCH CODEBASE TRƯỚC KHI CODE
==================================================

Trước khi sửa:

1. Scan toàn bộ project.
2. Xác định:
   - Framework UI đang sử dụng.
   - Entry point.
   - Component structure.
   - State management.
   - Database/local storage.
   - Video processing service.
   - FFmpeg integration.
   - CapCut integration.
   - Subtitle processing.
   - AI integration.
   - Voice/TTS integration.
   - Selenium/browser automation.
   - Render pipeline.
   - Queue/job system.
   - Config management.
3. Xác định chức năng nào đã tồn tại.
4. Xác định chức năng nào thiếu.
5. Không duplicate logic đã có.
6. Tách UI khỏi business logic nếu code hiện tại đang coupling quá nhiều.

Sau khi phân tích mới bắt đầu implementation.

==================================================
2. KIẾN TRÚC GIAO DIỆN CHÍNH
==================================================

Thiết kế lại màn hình chính thành 3 TAB:

TAB 1: VIDEO WORKSPACE
TAB 2: AUTOMATION & AI SETTINGS
TAB 3: PROJECT / ASSET / LICENSE MANAGEMENT

Tên có thể điều chỉnh cho phù hợp với sản phẩm hiện tại.

UI phải mang phong cách:

- Modern Desktop Application
- Dark theme
- Professional
- Compact nhưng không chật chội
- Information hierarchy rõ ràng
- Không sử dụng quá nhiều màu
- Màu accent thống nhất
- Card/Panel có border nhẹ
- Typography rõ ràng
- Button hierarchy:
    Primary
    Secondary
    Danger
    Ghost
- Icon nhất quán
- Tooltip cho action khó hiểu
- Loading/Skeleton khi xử lý
- Toast notification
- Confirmation dialog cho destructive actions

==================================================
3. TAB 1 – VIDEO WORKSPACE
==================================================

Đây là màn hình quan trọng nhất.

Layout đề xuất:

┌────────────────────────────────────────────────────────────┐
│ Project / Toolbar / Global actions                         │
├──────────────────────┬─────────────────────────────────────┤
│                      │                                     │
│ VIDEO PREVIEW        │ SUBTITLE / SCRIPT EDITOR            │
│                      │                                     │
│ Player               │ Timeline Rows                       │
│                      │ Original / Translation              │
│ Controls             │ Voice / Ratio / Duration            │
│                      │                                     │
├──────────────────────┴─────────────────────────────────────┤
│ PROCESS PIPELINE                                            │
├────────────────────────────────────────────────────────────┤
│ VIDEO JOB QUEUE / PROJECT LIST                              │
└────────────────────────────────────────────────────────────┘

------------------------------
3.1 VIDEO PREVIEW
------------------------------

Có:

- Video preview.
- Play/Pause.
- Stop.
- Seek.
- Current time / duration.
- ±5 seconds.
- Volume.
- Playback speed.
- Timeline slider.
- Current subtitle highlight.
- Jump đến subtitle khi click subtitle row.

Nếu framework cho phép:
- keyboard shortcut Space = Play/Pause.
- Left/Right = seek.
- Ctrl+S = save project.

------------------------------
3.2 SUBTITLE / SCRIPT EDITOR
------------------------------

Table editor gồm:

- #
- Start
- End
- Duration
- Speaker/Vocal
- Original
- Translation
- Voice
- Speed/Ratio
- Status
- Actions

Cho phép:

- Edit trực tiếp.
- Multi select.
- Delete.
- Split subtitle.
- Merge subtitle.
- Duplicate.
- Re-translate một dòng.
- Regenerate voice một dòng.
- Preview voice.
- Adjust timing.
- Auto fit voice vào subtitle duration.
- Undo/Redo.

Phía dưới editor có action toolbar:

[Import SRT]
[Extract Subtitle]
[Speech To Text]
[Translate]
[Generate Voice]
[Sync Timeline]
[Save]

Không để button nằm lộn xộn.

------------------------------
3.3 PROCESSING PIPELINE
------------------------------

Thay checkbox list đơn giản bằng Pipeline/Stepper:

1. Extract Subtitle
2. Speech Recognition
3. Translate
4. Rewrite Script
5. Generate Voice
6. Sync Timeline
7. Video Processing
8. Subtitle Burn-in
9. Render
10. Export

Mỗi step có trạng thái:

WAITING
RUNNING
SUCCESS
FAILED
SKIPPED

Cho phép enable/disable từng step.

Ví dụ:

✓ Extract Subtitle
✓ Translate
● Generate Voice       67%
○ Sync Timeline
○ Render

Khi FAILED:
hiển thị error ngắn + nút Retry.

==================================================
4. VIDEO QUEUE / BATCH PROCESSING
==================================================

Đây phải là thành phần quan trọng.

Table:

ID
Thumbnail
Project
Video
Duration
Subtitle
Translation
Voice
Language
Current Task
Progress
Status
ETA
Actions

Status:

READY
QUEUED
PROCESSING
PAUSED
FAILED
COMPLETED
CANCELLED

Progress bar từng video.

Global toolbar:

+ Add Video
+ Add Folder
Import SRT
Start Selected
Start All
Pause
Resume
Retry Failed
Stop
Delete
Clear Completed
Open Output Folder

Hỗ trợ:

- Multi-select.
- Batch operations.
- Drag/drop video.
- Search.
- Filter.
- Sort.

Filter theo:
- Status.
- Language.
- Project.
- Processing step.

==================================================
5. CONCURRENCY / SLOT PROCESSING
==================================================

Tool reference đang có Slot 1 / Slot 2 / Slot 3.

Nâng cấp thành Worker/Processing Slot Manager.

Ví dụ:

Workers: 3
Running: 3
Queue: 17
Completed: 42
Failed: 1

Worker #1
Video A
Generating Voice
72%

Worker #2
Video B
Translation
34%

Worker #3
Video C
Rendering
89%

Cho phép cấu hình:

Max concurrent videos: [3]

Không hard-code số worker.

Queue manager phải tự lấy job tiếp theo khi worker hoàn thành.

==================================================
6. TAB 2 – AUTOMATION & AI SETTINGS
==================================================

Không để tất cả setting trên một màn hình dài.

Chia thành sidebar/category:

AI
Translation
Voice
Subtitle
Video
CapCut
Browser Automation
Render
Performance
Storage
Advanced

==================================================
7. AI CONFIGURATION
==================================================

Fields:

Provider
Model
Thinking Level
API Mode
Temperature nếu supported
Timeout
Retry
Max concurrent requests

Model selector.

Ví dụ:

Provider:
Gemini / OpenAI / Custom

Model:
model dropdown

Thinking:
Low
Medium
High

Có:

[Test Connection]

Hiển thị:

Connected
Latency
Model
Last Checked

==================================================
8. TRANSLATION / SCRIPT SETTINGS
==================================================

Config:

Source Language
Target Language
Movie/Content Genre
Translation Style
Character Context
Preserve Names
Preserve Terminology
Profanity handling
Sentence length optimization
Voice-friendly rewrite

Genre:

Drama / Romance
Fantasy / Cultivation
Crime / Detective
Police Bodycam
Prison / Crime
Survival
Documentary
Custom

Cho phép tạo custom prompt.

Có Prompt Template Editor.

==================================================
9. VOICE / TTS CONFIGURATION
==================================================

Provider abstraction.

Có thể hỗ trợ:

NTSGenAI
GPT-SoVITS
Custom API

UI:

Voice Provider
Server/API URL
API Key
Male Voice
Female Voice
Narrator Voice
Speed
Pitch
Volume
Emotion
Language

Button:

Test Voice
Refresh Voices
Check Credits
Test Connection

API Key:
mask mặc định.

Không log API key.

Cho phép Voice Mapping:

Speaker A → Voice X
Speaker B → Voice Y
Narrator → Voice Z

==================================================
10. CAPCUT INTEGRATION
==================================================

Tạo section riêng:

CapCut Integration

Config:

CapCut path
Project path
Draft path
Template project
Export path
Auto create project
Auto import video
Auto import audio
Auto import subtitle
Auto align timeline
Auto apply effects
Auto save
Auto export

Actions:

Detect CapCut
Open CapCut
Open Current Project
Create Draft
Sync Draft
Validate Draft
Export Project

Nếu tool hiện tại thao tác trực tiếp với CapCut project/draft:
phải giữ compatibility.

Tách CapCut logic thành service riêng, ví dụ:

CapCutService
CapCutProjectService
CapCutDraftService

UI không được thao tác trực tiếp file CapCut.

==================================================
11. VIDEO PROCESSING SETTINGS
==================================================

Các option:

Remove original voice
Keep background audio
Noise reduction
Normalize audio
Background music
Background music volume
Zoom/Pan
Crop
Aspect Ratio
Resolution
FPS
Subtitle burn-in
Intro/Outro
Watermark

Preset:

TikTok 9:16
YouTube Shorts 9:16
YouTube 16:9
Facebook Reel
Original

==================================================
12. TAB 3 – PROJECT / ASSET MANAGEMENT
==================================================

Quản lý:

Projects
Presets
Voice Profiles
Prompt Templates
CapCut Templates
Output History
Render History
License

Project có:

Project Name
Created At
Videos
Completed
Failed
Output
Preset
Language

Cho phép:

Open
Rename
Duplicate
Archive
Delete

==================================================
13. PROFESSIONAL DASHBOARD HEADER
==================================================

Header nên hiển thị:

Project
Queue
Workers
AI Status
Voice API Status

Ví dụ:

AUTO VIDEO STUDIO

Project: Movie-ES-001

Queue  18
Running 3
Done    41
Failed   1

AI      ● Connected
Voice   ● Connected

Không đưa quá nhiều text vào status bar.

==================================================
14. STATUS BAR
==================================================

Bottom status:

CPU
RAM
GPU nếu available
FFmpeg
CapCut
AI API
Voice API

Ví dụ:

CPU 42% | RAM 5.2/16GB | Workers 3/3 | Queue 18

Current:
Generating voice – video_004.mp4 – 72%

==================================================
15. LOG / ACTIVITY PANEL
==================================================

Thêm collapsible panel:

Activity / Logs

Có:

INFO
WARNING
ERROR

Filter log.

Ví dụ:

10:21:03 Subtitle extraction completed
10:21:05 Translation started
10:21:17 Translation completed
10:21:18 Voice generation started

Không spam popup cho mọi event.

Error quan trọng mới dùng Toast/Dialog.

==================================================
16. ERROR HANDLING
==================================================

Mọi pipeline step phải có error handling.

Ví dụ:

AI timeout
Voice API timeout
CapCut unavailable
FFmpeg failure
Invalid subtitle
Disk full
Output exists
Browser/Selenium crash

Có:

Retry
Skip
Cancel

Batch processing:
1 video fail KHÔNG được làm toàn bộ queue dừng.

==================================================
17. JOB STATE / RESUME
==================================================

Rất quan trọng.

Lưu trạng thái processing.

Nếu app crash/restart:

không chạy lại toàn bộ từ đầu.

Ví dụ video đã:

Extract ✓
Translate ✓
Voice ✓
Render ✗

Restart app:

resume từ Render.

Persist:

job state
step state
progress
output
error
retry count

==================================================
18. PRESET SYSTEM
==================================================

Cho phép lưu toàn bộ config thành preset.

Ví dụ:

Spanish Crime Movie
Spanish Romance
English Shorts
TikTok Auto Dub

Preset chứa:

AI config reference
Translation config
Voice config
Subtitle config
Video processing
Render config
CapCut config

KHÔNG lưu plaintext secret vào preset export.

==================================================
19. UI DESIGN SYSTEM
==================================================

Refactor UI để có Design System.

Tạo reusable components:

Button
IconButton
Input
Select
Checkbox
Switch
Slider
Card
Panel
Tabs
Badge
ProgressBar
DataTable
Modal
Toast
Tooltip
ContextMenu
EmptyState
LoadingState
ErrorState

Spacing nhất quán.

Ví dụ:

4
8
12
16
24
32

Border radius thống nhất.

Không để:
- button mỗi nơi một kích thước.
- màu sắc tùy tiện.
- font quá nhỏ.
- table quá sát nhau.
- hàng chục màu status khác nhau.

==================================================
20. COLOR / STATUS SYSTEM
==================================================

Sử dụng semantic colors từ theme/design token.

Không hard-code màu khắp source code.

Các semantic token:

primary
success
warning
danger
info
surface
surfaceElevated
border
textPrimary
textSecondary
textMuted

Status badge phải dễ phân biệt.

==================================================
21. RESPONSIVE DESKTOP LAYOUT
==================================================

Tool chủ yếu desktop nhưng phải hoạt động tốt:

1366x768
1920x1080
2560x1440

Không hard-code layout theo screenshot.

Panel cần resize được nếu framework hỗ trợ.

Subtitle editor và Queue table phải ưu tiên diện tích.

==================================================
22. UX IMPROVEMENTS
==================================================

Thêm:

Keyboard shortcuts
Context menu
Drag & drop
Multi-select
Batch edit
Search
Filter
Sorting
Auto save
Undo/Redo
Recent projects
Remember window size
Remember panel sizes

Destructive action cần confirmation.

==================================================
23. PERFORMANCE
==================================================

Không block UI thread khi:

FFmpeg
AI API
TTS
CapCut
File scan
Video processing
Render

Sử dụng async/background worker phù hợp với framework.

UI phải luôn responsive.

Không render lại toàn bộ table khi chỉ progress của một job thay đổi nếu framework hỗ trợ granular update.

Throttle progress event nếu quá nhiều.

==================================================
24. SECURITY
==================================================

API Key / Token:

- Không hard-code.
- Không commit.
- Mask trên UI.
- Không xuất hiện trong log.
- Không lưu plaintext nếu hệ thống có secure storage.

Sanitize:
filename
project path
output path

==================================================
25. CODE ARCHITECTURE
==================================================

Nếu codebase hiện tại cho phép, hướng tới:

UI
 ↓
Controller / ViewModel
 ↓
Application Services
 ↓
Job / Pipeline Manager
 ↓
Services
 ├── AIService
 ├── TranslationService
 ├── SubtitleService
 ├── VoiceService
 ├── VideoService
 ├── FFmpegService
 ├── CapCutService
 └── StorageService

Pipeline:

VideoJob
  ├── ExtractSubtitleStep
  ├── TranscribeStep
  ├── TranslateStep
  ├── RewriteStep
  ├── VoiceStep
  ├── SyncStep
  ├── VideoProcessStep
  ├── SubtitleStep
  └── RenderStep

Không để một UI class chứa toàn bộ business logic.

==================================================
26. DATA MODEL
==================================================

Chuẩn hóa tối thiểu:

Project
VideoJob
PipelineStep
SubtitleItem
VoiceProfile
Preset
AppSettings
ProcessingWorker
ProcessingResult

VideoJob:

id
projectId
sourceFile
outputFile
status
progress
currentStep
createdAt
startedAt
completedAt
error
retryCount

PipelineStep:

name
enabled
status
progress
startedAt
completedAt
error

==================================================
27. CẢI TIẾN SO VỚI UI REFERENCE
==================================================

Ảnh reference hiện tại có khá nhiều:

- Button nhỏ.
- Text nhỏ.
- Mật độ thông tin cao.
- Nhiều màu.
- Control chưa có hierarchy rõ.
- Configuration bị dồn vào một màn hình.
- Queue chưa thể hiện rõ pipeline.
- Status khó scan nhanh.

KHÔNG copy các nhược điểm này.

Giữ lại functional idea nhưng redesign theo hướng:

Professional
Minimal
Fast
Scalable
Clear
Production-ready

==================================================
28. TARGET UI
==================================================

Concept mong muốn:

┌───────────────────────────────────────────────────────────────┐
│ AUTO VIDEO STUDIO     Project ▼       AI ●   Voice ●   ⚙    │
├───────────────────────────────────────────────────────────────┤
│ Workspace │ Automation │ Projects                             │
├────────────────────┬──────────────────────────────────────────┤
│                    │ Script / Subtitle Editor                 │
│   VIDEO PREVIEW    │                                          │
│                    │ 01  00:01 → 00:04   Original...         │
│   ▶ ━━━━━━━ 03:42  │                         Translation...   │
│                    │ 02  00:04 → 00:07   ...                 │
├────────────────────┴──────────────────────────────────────────┤
│ ✓ Subtitle → ✓ Translate → ● Voice 68% → ○ Sync → ○ Render │
├───────────────────────────────────────────────────────────────┤
│ JOB QUEUE                                                     │
│ □ Video             Task             Progress        Status   │
│ □ movie01.mp4       Voice            ███████ 68%     Running  │
│ □ movie02.mp4       Translate        ████ 42%        Running  │
│ □ movie03.mp4       Waiting          ───────          Queue    │
├───────────────────────────────────────────────────────────────┤
│ CPU 38% | RAM 42% | Workers 2/3 | Queue 12       Logs ↑     │
└───────────────────────────────────────────────────────────────┘

Đây chỉ là layout concept.
Hãy điều chỉnh theo framework và codebase thực tế.

==================================================
29. IMPLEMENTATION STRATEGY
==================================================

Không sửa hàng loạt một cách mù quáng.

Thực hiện theo phase:

PHASE 1
- Audit codebase.
- Map architecture.
- Identify existing features.
- Identify technical debt.

PHASE 2
- Tạo/refactor Design System.
- Main layout.
- Navigation.
- Theme.

PHASE 3
- Workspace.
- Video preview.
- Subtitle editor.
- Pipeline.

PHASE 4
- Job Queue.
- Worker manager.
- Progress/status.

PHASE 5
- Settings.
- AI.
- Voice.
- CapCut.
- Render.

PHASE 6
- Project/preset management.

PHASE 7
- Persistence/resume.
- Error/retry.

PHASE 8
- UX polish.
- Performance.
- Testing.

==================================================
30. YÊU CẦU KHI IMPLEMENT
==================================================

Trước mỗi phase:

- Nêu file/module sẽ sửa.
- Nêu lý do.
- Kiểm tra dependency.

Sau mỗi phase:

- Build project.
- Fix compile/runtime errors.
- Không để TODO giả.
- Không để dead code.
- Không phá feature cũ.

Nếu có test:
chạy test.

Nếu chưa có test:
thêm test cho các business logic quan trọng nếu hợp lý.

==================================================
31. ACCEPTANCE CRITERIA
==================================================

Hoàn thành khi:

1. App có UI professional hơn rõ rệt.
2. 3 tab có information architecture rõ ràng.
3. Video preview + subtitle/script editor dễ thao tác.
4. Batch video queue hoạt động.
5. Pipeline status hiển thị rõ.
6. Có concurrency worker/slot.
7. AI/Voice/CapCut config được tổ chức lại.
8. Có preset.
9. Có progress/error/retry.
10. App restart có thể resume job nếu architecture hiện tại hỗ trợ.
11. UI không freeze khi processing.
12. Không làm mất chức năng hiện có.
13. Không hard-code API secrets.
14. Build thành công.
15. Code structure sạch hơn trước.

==================================================
32. BẮT ĐẦU
==================================================

Bây giờ hãy:

1. Đọc toàn bộ structure project.
2. Tìm entry point và UI hiện tại.
3. Tìm toàn bộ module liên quan:
   video / ffmpeg / capcut / subtitle / AI / TTS /
   selenium / render / queue / config.
4. Viết ngắn gọn architecture hiện tại.
5. Lập danh sách file cần thay đổi.
6. Lập implementation plan.
7. Sau đó BẮT ĐẦU CODE ngay.

Không dừng lại chỉ để đưa recommendation.
Không tạo project demo mới.
Không thay thế codebase bằng mockup.
Update trực tiếp tool hiện tại.
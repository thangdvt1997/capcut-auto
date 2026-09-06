# TASK: Upgrade Auto Video Editor – Batch Automation + CapCut + AI

Hãy phân tích **toàn bộ codebase hiện tại trước khi code** và nâng cấp tool bằng module **Auto Video Editor / AI Video Automation**.

## CONTEXT QUAN TRỌNG

Tool hiện tại đã có:

- Kết nối/tích hợp với **CapCut**.
- Có khả năng tự động thao tác/chạy workflow trên CapCut.
- Đã có kết nối **AI/LLM** trong hệ thống gốc.
- Có backend/frontend/database/job processing hiện tại.

### Yêu cầu bắt buộc

KHÔNG xây dựng lại CapCut integration hoặc AI integration nếu codebase đã có.

Trước tiên phải audit và xác định:

1. CapCut hiện được kết nối bằng cách nào.
2. Các API/service/module/function hiện tại liên quan CapCut.
3. AI provider/service hiện tại.
4. Cách gửi prompt/request tới AI.
5. Job/queue/worker hiện tại.
6. Database/schema hiện tại.
7. Storage/file upload hiện tại.
8. Cơ chế realtime/progress hiện tại.
9. Authentication/permission hiện tại.
10. Các component UI có thể reuse.

Sau đó thiết kế tính năng mới sao cho **reuse tối đa architecture hiện tại**.

Không duplicate logic.

---

# 1. MỤC TIÊU

Xây dựng hệ thống cho phép:

```text
Upload / Select Videos
        ↓
Select Editing Mode
        ↓
Manual / Template / AI
        ↓
Generate Editing Plan
        ↓
Validate Plan
        ↓
Create Batch Jobs
        ↓
CapCut Automation
        ↓
Render / Export
        ↓
Collect Output
```

Tool phải có khả năng tự động xử lý từ **1 → N video**.

Ví dụ:

```text
100 videos
   ↓
AI phân tích
   ↓
chọn/generate editing plan
   ↓
apply template
   ↓
CapCut tự động edit
   ↓
export
   ↓
100 video output
```

---

# 2. EDITING MODES

Cần hỗ trợ ít nhất 3 mode.

## Mode A – Manual Setup

User tự cấu hình cách edit.

Ví dụ:

```text
Trim đầu: 3s
Trim cuối: 5s
Ratio: 9:16
Resolution: 1080x1920

Intro: intro.mp4
Outro: outro.mp4

Watermark:
    file: logo.png
    position: top-right

Subtitle: enabled

Background music:
    music.mp3
    volume: 20%

Transition:
    fade
    duration: 0.5s
```

Sau đó:

```text
Apply to selected videos
```

CapCut automation tự động thực hiện toàn bộ workflow.

---

# 3. TEMPLATE MODE

Cho phép tạo và quản lý nhiều Video Template.

Ví dụ:

```text
TikTok Template
YouTube Shorts Template
Facebook Reel Template
Football Highlight Template
News Template
Podcast Short Template
Custom Template
```

Template có thể chứa:

```yaml
name: football-short

canvas:
  ratio: 9:16
  resolution: 1080x1920

trim:
  start: 2
  end: 3

intro:
  enabled: true
  asset: intro.mp4

outro:
  enabled: true
  asset: outro.mp4

watermark:
  enabled: true
  asset: logo.png
  position: top-right

subtitle:
  enabled: true
  style: football-caption

audio:
  normalize: true
  background_music: bg.mp3
  volume: 0.2

transition:
  type: fade
  duration: 0.5

export:
  format: mp4
  resolution: 1080x1920
  fps: 30
```

Template phải:

- Create
- Edit
- Clone
- Delete
- Enable/disable
- Preview
- Version
- Import/export
- Apply cho một video
- Apply cho nhiều video
- Apply cho Batch Job

---

# 4. VISUAL TEMPLATE BUILDER

Xây dựng UI cho phép user tự tạo workflow.

Ví dụ:

```text
INPUT
  ↓
ANALYZE
  ↓
TRIM
  ↓
CUT
  ↓
MERGE
  ↓
CROP
  ↓
RESIZE
  ↓
INTRO
  ↓
TEXT
  ↓
SUBTITLE
  ↓
WATERMARK
  ↓
TRANSITION
  ↓
AUDIO
  ↓
OUTRO
  ↓
EXPORT
```

User có thể:

- Add step
- Remove step
- Drag/drop reorder
- Enable/disable
- Duplicate
- Edit parameters
- Save as template

Thiết kế data model dạng structured JSON để backend, AI và CapCut Automation cùng hiểu được.

Ví dụ:

```json
{
  "steps": [
    {
      "type": "trim",
      "config": {}
    },
    {
      "type": "subtitle",
      "config": {}
    },
    {
      "type": "watermark",
      "config": {}
    },
    {
      "type": "export",
      "config": {}
    }
  ]
}
```

Không hard-code workflow theo từng template.

---

# 5. AI VIDEO EDITING MODE

Tận dụng **AI integration đang có trong tool**.

User có thể nhập yêu cầu bằng natural language.

Ví dụ:

```text
Cắt video này thành video TikTok khoảng 45 giây.

Giữ những đoạn hấp dẫn nhất.

Bỏ những đoạn không có nội dung.

Format 9:16.

Thêm subtitle.

Highlight các câu quan trọng.

Thêm intro và outro.

Logo góc trên bên phải.
```

AI phải convert yêu cầu thành một **Editing Plan có cấu trúc**.

Ví dụ:

```json
{
  "target_duration": 45,
  "ratio": "9:16",
  "strategy": "highlight",
  "subtitle": true,
  "highlight_text": true,
  "intro": true,
  "outro": true,
  "watermark": {
    "enabled": true,
    "position": "top-right"
  }
}
```

Sau đó Editing Plan được convert thành workflow mà CapCut Automation có thể chạy.

QUAN TRỌNG:

AI không được trực tiếp thực thi các thao tác nguy hiểm/không xác định.

Flow phải là:

```text
User Prompt
     ↓
AI
     ↓
Structured Editing Plan
     ↓
Schema Validation
     ↓
Normalize
     ↓
Preview / Confirmation nếu cần
     ↓
CapCut Execution Plan
     ↓
CapCut Automation
```

---

# 6. AI PHÂN TÍCH VIDEO

Kiểm tra khả năng AI integration hiện tại.

Nếu architecture cho phép, bổ sung Video Analysis Pipeline.

Hệ thống có thể phân tích:

```text
video
 ↓
audio extraction
 ↓
speech/transcript
 ↓
scene detection / metadata
 ↓
AI analysis
 ↓
interesting segments
 ↓
editing plan
```

AI có thể xác định:

- đoạn quan trọng
- đoạn nói thừa
- silence
- đoạn không có nội dung
- highlight
- hook
- câu quan trọng
- scene change
- chủ đề
- caption
- title
- description

Ví dụ video dài 20 phút:

```text
00:01:20 → 00:01:55 HIGH SCORE
00:04:11 → 00:04:58 HIGH SCORE
00:09:20 → 00:10:05 MEDIUM SCORE
```

AI có thể đề xuất:

```text
Short #1
00:01:20 → 00:01:55

Short #2
00:04:11 → 00:04:58
```

User có thể:

```text
Preview
Edit
Approve
Run
```

---

# 7. AI AUTO TEMPLATE

Bổ sung mode:

```text
AUTO TEMPLATE
```

User chỉ cần chọn video.

AI tự phân tích:

```text
Content Type
Video Duration
Aspect Ratio
Speech
Scenes
Important Segments
```

Sau đó AI đề xuất template phù hợp.

Ví dụ:

```text
Detected:
Football Highlight

Recommended:
Football Short V3

Output:
9:16
45 seconds
Dynamic subtitle
Fast transition
Logo
Intro/Outro
```

Cho phép:

```text
Accept
Change Template
Customize
Run
```

---

# 8. AI TEMPLATE GENERATOR

Cho phép user tạo template bằng prompt.

Ví dụ:

```text
Tạo cho tôi template video bóng đá dạng TikTok.

Video khoảng 30-45 giây.

9:16.

Subtitle lớn.

Highlight tên cầu thủ.

Logo góc phải.

Intro 2 giây.

Transition nhanh.
```

AI convert thành:

```text
Template Definition
        ↓
Validate
        ↓
Template Builder
        ↓
Preview
        ↓
Save Template
```

Sau đó template có thể reuse cho hàng nghìn video.

---

# 9. AUTO CUT

Hỗ trợ:

```text
Trim Start
Trim End

Keep Range
Remove Range

Split every X seconds
Split into N parts
```

Ví dụ:

```text
KEEP

00:00:10 → 00:00:35
00:01:15 → 00:01:50
00:03:00 → 00:03:40
```

Sau đó CapCut tự:

```text
Import
↓
Split
↓
Remove unwanted segments
↓
Rearrange
↓
Merge
```

---

# 10. AUTO MERGE

Hỗ trợ:

```text
Intro
+
Video A
+
Video B
+
Video C
+
Outro
```

Hoặc batch:

```text
Intro + Video01 + Outro
Intro + Video02 + Outro
Intro + Video03 + Outro
```

Cho phép random asset:

```text
Random Intro
Random Outro
Random Transition
Random Background Music
```

nhưng random phải theo rule/template và có thể reproduce bằng seed nếu cần debug.

---

# 11. MULTI-TEMPLATE BATCH

Một video có thể chạy nhiều template.

Ví dụ:

```text
video01.mp4
```

Apply:

```text
TikTok
YouTube Shorts
Facebook Reel
Original
```

Output:

```text
video01_tiktok.mp4
video01_youtube.mp4
video01_facebook.mp4
video01_original.mp4
```

Tương tự với N video.

---

# 12. CAPCUT AUTOMATION LAYER

Đây là phần rất quan trọng.

Audit CapCut integration hiện tại và tạo abstraction:

```text
EditingPlan
     ↓
CapCutAdapter
     ↓
CapCut Project
     ↓
Timeline Operations
     ↓
Render
     ↓
Export
```

Không để business logic phụ thuộc trực tiếp vào implementation của CapCut.

Ví dụ interface:

```text
createProject()

importMedia()

addToTimeline()

trim()

split()

deleteSegment()

moveSegment()

addText()

addSubtitle()

addOverlay()

addAudio()

addTransition()

setCanvas()

setRatio()

setResolution()

export()
```

Nếu integration hiện tại đã có abstraction tương tự thì mở rộng nó, KHÔNG tạo duplicate architecture.

---

# 13. CAPCUT EXECUTION PLAN

Tạo intermediate representation giữa Template/AI và CapCut.

Ví dụ:

```json
{
  "project": {
    "name": "video01_tiktok"
  },

  "timeline": [
    {
      "action": "import",
      "source": "video01.mp4"
    },
    {
      "action": "trim",
      "start": 3,
      "end": 45
    },
    {
      "action": "subtitle",
      "source": "auto"
    },
    {
      "action": "overlay",
      "asset": "logo.png",
      "position": "top-right"
    }
  ],

  "export": {
    "resolution": "1080x1920",
    "fps": 30,
    "format": "mp4"
  }
}
```

Flow:

```text
AI Plan
   ↓
Template Engine
   ↓
Execution Plan
   ↓
Validator
   ↓
CapCut Adapter
```

Điều này giúp AI không phụ thuộc trực tiếp vào CapCut.

---

# 14. BATCH JOB SYSTEM

Cần support:

```text
Batch
 ├── Video Job 01
 ├── Video Job 02
 ├── Video Job 03
 └── Video Job N
```

Status:

```text
PENDING
ANALYZING
PLANNING
QUEUED
OPENING_CAPCUT
IMPORTING
EDITING
EXPORTING
UPLOADING
COMPLETED
FAILED
CANCELLED
```

UI hiển thị:

```text
Video             Progress       Status

video01.mp4       100%           Completed
video02.mp4        72%           Exporting
video03.mp4        30%           Editing
video04.mp4         0%           Queued
```

Support:

- Retry
- Retry failed
- Cancel
- Pause
- Resume
- Duplicate job
- Re-run with another template

---

# 15. CAPCUT WORKER / RESOURCE MANAGEMENT

Vì CapCut automation có thể cần GUI/process/resource riêng, cần thiết kế Worker Manager.

Ví dụ:

```text
Job Queue
    ↓
Scheduler
    ↓
CapCut Worker Pool

Worker 01
Worker 02
Worker 03
```

Mỗi worker cần track:

```text
ID
Machine
Status
Current Job
CapCut Status
CPU
RAM
Disk
Last Heartbeat
```

Không cho nhiều job tranh chấp cùng một CapCut instance nếu integration hiện tại không support concurrency.

Có configurable:

```text
max_parallel_jobs
max_jobs_per_worker
job_timeout
export_timeout
retry_count
```

---

# 16. FAILURE RECOVERY

CapCut automation có thể fail ở nhiều bước.

Ví dụ:

```text
CapCut không mở
Import fail
Asset missing
Timeline operation fail
Export fail
CapCut crash
Worker disconnect
Timeout
```

Phải có checkpoint.

Ví dụ:

```text
ANALYSIS_COMPLETE
PLAN_COMPLETE
PROJECT_CREATED
MEDIA_IMPORTED
EDIT_COMPLETE
EXPORT_STARTED
EXPORT_COMPLETE
```

Nếu worker crash:

```text
detect failure
↓
release worker
↓
retry/requeue job
↓
resume từ checkpoint nếu có thể
```

Không chạy lại AI analysis nếu kết quả analysis vẫn hợp lệ.

---

# 17. ASSET LIBRARY

Tạo/quản lý Asset Library cho:

```text
Intro
Outro
Logo
Watermark
Music
Sound Effect
Overlay
Font
Subtitle Style
Transition Preset
Background
```

Cho phép template reference asset bằng ID thay vì hard-code path.

Ví dụ:

```json
{
  "intro_asset_id": "intro_001",
  "logo_asset_id": "logo_football",
  "music_asset_id": "music_fast_01"
}
```

---

# 18. PREVIEW / DRY RUN

Trước khi chạy batch lớn, cho phép:

```text
Dry Run
```

Dry Run không export video mà hiển thị:

```text
Input
Template
AI Decision
Editing Plan
CapCut Execution Plan
Expected Output
```

Có thể chạy:

```text
Test 1 Video
```

trước khi apply cho 100 video.

---

# 19. AI GUARDRAILS

AI output bắt buộc phải qua schema validation.

Không cho AI tự sinh arbitrary command/script để chạy trên server.

Chỉ cho AI sử dụng các operation được whitelist:

```text
TRIM
CUT
SPLIT
MERGE
CROP
RESIZE
TEXT
SUBTITLE
OVERLAY
AUDIO
TRANSITION
INTRO
OUTRO
EXPORT
```

Validate:

```text
timestamp
duration
asset
resolution
ratio
operation
parameters
```

Invalid plan phải reject hoặc repair trước khi gửi CapCut.

---

# 20. TEMPLATE VERSIONING

Template cần version.

Ví dụ:

```text
Football Short
v1
v2
v3
```

Job phải lưu chính xác:

```text
template_id
template_version
```

Không để user sửa template rồi làm thay đổi các job cũ.

---

# 21. HISTORY

Tạo Video Processing History.

Lưu:

```text
Input video
Output video
Template
Template version
AI prompt
AI result
Editing plan
Execution plan
CapCut worker
Start time
End time
Duration
Status
Error
Retry count
```

Cho phép:

```text
View
Download output
Re-run
Clone settings
Run with another template
View logs
```

---

# 22. AI COST / TOKEN CONTROL

Vì tool đã có AI integration, cần tránh gọi AI không cần thiết.

Implement cache:

```text
video_hash
+
analysis_type
+
AI model/version
=
cached analysis
```

Nếu cùng video đã được analyze thì reuse khi phù hợp.

Tách:

```text
Video Analysis
Editing Decision
Template Generation
Metadata Generation
```

để không phải analyze lại toàn bộ video.

Nếu AI provider hiện tại có token/cost metadata thì lưu usage cho từng job.

---

# 23. OBSERVABILITY

Log theo:

```text
batch_id
job_id
video_id
worker_id
template_id
capcut_project_id
```

Có thể trace:

```text
User
 ↓
Batch
 ↓
AI Analysis
 ↓
Editing Plan
 ↓
CapCut Worker
 ↓
Export
 ↓
Output
```

Thêm metrics:

```text
jobs_total
jobs_completed
jobs_failed

average_processing_time
average_export_time

AI processing time
CapCut processing time

worker utilization
queue length
```

---

# 24. DATABASE DESIGN

Phân tích schema hiện tại trước.

Chỉ tạo migration cần thiết.

Có thể cần các entity:

```text
video_assets
video_templates
video_template_versions

video_batches
video_jobs

video_analysis

editing_plans
execution_plans

capcut_workers
capcut_projects

video_outputs
job_logs
```

Không bắt buộc dùng đúng tên trên nếu architecture hiện tại có convention khác.

---

# 25. UI

Thêm menu:

```text
Video Automation
```

Các page:

```text
Dashboard

New Batch

Videos

Templates

AI Editor

Template Builder

Assets

Jobs

Workers

History

Settings
```

### New Batch wizard

Step 1:

```text
Select Videos
```

Step 2:

```text
Editing Mode

○ Manual
○ Template
○ AI Auto
```

Step 3:

```text
Configuration
```

Step 4:

```text
AI / Editing Plan Preview
```

Step 5:

```text
Test 1 Video
```

Step 6:

```text
Run Batch
```

---

# 26. UX CHO BATCH LỚN

Ví dụ user chọn:

```text
500 videos
```

Không render 500 editor instances trên frontend.

Sử dụng:

```text
pagination
virtualized list
server-side filtering
job aggregation
```

Dashboard:

```text
TOTAL       500

DONE        320
PROCESSING   10
QUEUED      160
FAILED       10
```

---

# 27. SMART AUTOMATION

Sau khi core features hoạt động ổn định, bổ sung khả năng tạo Automation Rule.

Ví dụ:

```text
WHEN:
new video added to folder X

IF:
duration > 5 minutes

THEN:
AI analyze
↓
create 3 shorts
↓
apply TikTok template
↓
CapCut edit
↓
export
```

Hoặc:

```text
New Video
↓
AI Detect Content Type
↓
Select Template
↓
CapCut
↓
Export
```

Thiết kế rule engine mở rộng được nhưng không over-engineer nếu codebase hiện tại chưa cần.

---

# 28. RANDOMIZATION / VARIATION

Cho phép template tạo variation có kiểm soát:

```text
Random intro from collection
Random outro
Random music
Random transition
Random subtitle style
```

Support:

```text
seed
```

để cùng seed có thể reproduce chính xác output khi debug.

---

# 29. ARCHITECTURE MỤC TIÊU

Ưu tiên architecture:

```text
                 ┌───────────────┐
                 │      UI       │
                 └───────┬───────┘
                         │
                 ┌───────▼───────┐
                 │ Batch Manager │
                 └───────┬───────┘
                         │
              ┌──────────▼──────────┐
              │ Editing Orchestrator│
              └──────────┬──────────┘
                         │
           ┌─────────────┼─────────────┐
           │             │             │
           ▼             ▼             ▼
       Manual         Template         AI
           │             │             │
           └─────────────┼─────────────┘
                         ▼
                  Editing Plan
                         │
                         ▼
                     Validator
                         │
                         ▼
                  Execution Plan
                         │
                         ▼
                    Job Queue
                         │
                         ▼
                  CapCut Adapter
                         │
                         ▼
                 CapCut Workers
                         │
                         ▼
                      Export
                         │
                         ▼
                     Storage
```

AI là **decision/planning layer**.

CapCut là **execution/rendering layer**.

Không trộn hai layer này với nhau.

---

# 30. QUAN TRỌNG: AUDIT TRƯỚC KHI IMPLEMENT

Không bắt đầu bằng việc tạo hàng loạt file mới.

Đầu tiên hãy scan repository và trả về:

## A. Current Architecture

Liệt kê:

```text
Frontend
Backend
Database
Queue
Worker
Storage
AI integration
CapCut integration
Authentication
Realtime
```

## B. Existing CapCut Flow

Xác định chính xác:

```text
User action
↓
Backend
↓
CapCut service
↓
CapCut automation
↓
Export
```

Chỉ rõ file/module/function đang chịu trách nhiệm.

## C. Existing AI Flow

Xác định:

```text
AI provider
AI service
Prompt system
Structured output support
Streaming
Retry
Token tracking
```

## D. Reusable Components

Liệt kê component/module có thể reuse.

## E. Missing Components

Liệt kê những phần thực sự cần phát triển.

---

# 31. IMPLEMENTATION PLAN

Sau khi audit, tạo implementation plan theo phase.

## Phase 1 – Foundation

```text
Editing Plan schema
Execution Plan schema
Template model
CapCut Adapter improvements
Job model
```

## Phase 2 – Manual Batch

```text
Multi-video upload/select
Manual editor config
Batch jobs
CapCut execution
Progress
Retry
```

## Phase 3 – Template Engine

```text
Template CRUD
Template version
Template Builder
Multi-template
Asset Library
```

## Phase 4 – AI

```text
Natural-language editing
AI Editing Plan
AI template generation
AI template recommendation
Video analysis
```

## Phase 5 – Scale

```text
Worker pool
Scheduler
Concurrency
Checkpoint
Recovery
Monitoring
```

## Phase 6 – Automation

```text
Rules
Watch folder/input
Automatic AI processing
Automatic CapCut processing
```

Mỗi phase phải có migration strategy và không phá tính năng hiện tại.

---

# 32. ACCEPTANCE TEST

Phải test ít nhất các scenario sau.

### Scenario 1

```text
1 video
+
Manual configuration
+
CapCut
+
Export
```

### Scenario 2

```text
10 videos
+
1 template
+
CapCut
+
10 outputs
```

### Scenario 3

```text
10 videos
+
3 templates
=
30 outputs
```

### Scenario 4

```text
Long video
+
AI analyze
+
select highlights
+
create short
+
CapCut
+
export
```

### Scenario 5

```text
User prompt
↓
AI creates template
↓
preview
↓
save
↓
apply to batch
```

### Scenario 6

```text
CapCut crash during job
↓
detect
↓
recover/requeue
↓
continue
```

### Scenario 7

```text
One video fails in batch of 100
```

99 video còn lại phải tiếp tục xử lý.

---

# 33. CODE QUALITY

Yêu cầu:

- Không duplicate existing functionality.
- Reuse CapCut integration hiện tại.
- Reuse AI integration hiện tại.
- Không hard-code path nếu có config system.
- Không hard-code template.
- Không để AI output chạy trực tiếp.
- Structured schema giữa AI và execution.
- Idempotent jobs khi có thể.
- Retry-safe.
- Có timeout.
- Có validation.
- Có logging.
- Có migration rollback strategy.
- Backward compatible với tính năng hiện tại.

---

# 34. CÁCH THỰC HIỆN

Bắt đầu bằng:

```text
STEP 1
Scan toàn bộ repository.

STEP 2
Tìm tất cả code liên quan:
CapCut
AI
video
media
template
job
queue
worker
storage
upload
export

STEP 3
Vẽ lại current flow.

STEP 4
Đề xuất architecture dựa trên code thực tế.

STEP 5
Liệt kê file cần:
CREATE
MODIFY
DELETE (nếu thực sự cần)

STEP 6
Đưa ra implementation plan.

STEP 7
Bắt đầu implement theo từng phase nhỏ.

STEP 8
Sau mỗi phase:
- build
- lint
- typecheck
- test
- fix lỗi

STEP 9
Không dừng ở mock UI.
Phải nối end-to-end với CapCut integration thực tế đang có.

STEP 10
Không thay thế code đang hoạt động nếu chỉ cần extend.
```

# KẾT QUẢ CUỐI CÙNG MONG MUỐN

Mục tiêu cuối cùng là biến tool hiện tại thành một hệ thống:

```text
                 AI VIDEO AUTOMATION

                       INPUT
                         │
              ┌──────────┼──────────┐
              │          │          │
           Manual     Template      AI
              │          │          │
              └──────────┼──────────┘
                         │
                  Editing Plan
                         │
                         ▼
                 Batch Scheduler
                         │
                         ▼
                  CapCut Workers
                         │
              ┌──────────┼──────────┐
              ▼          ▼          ▼
            Edit       Render     Export
              │          │          │
              └──────────┼──────────┘
                         ▼
                       OUTPUT
```

User có thể đi từ:

```text
"Đây là 100 video.
Hãy edit thành short theo template này."
```

hoặc:

```text
"AI tự phân tích 100 video này,
chọn đoạn hay nhất,
tạo short 30-60 giây,
thêm subtitle,
logo,
intro/outro,
sau đó dùng CapCut edit và export."
```

và hệ thống có thể tự động thực hiện pipeline end-to-end.

Ưu tiên cao nhất:

**Reuse AI + CapCut hiện tại → Template Engine → Batch Processing → Automation → Scale.**
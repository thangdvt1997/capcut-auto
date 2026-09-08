# MASTER SPEC — Consolidated (2026-09-08)

File này gộp toàn bộ 3 tài liệu spec gốc của dự án thành 1 file duy nhất để tránh nhầm lẫn giữa nhiều file rời rạc. Thứ tự theo lịch sử phát triển (cũ nhất trước).

- **Phần 1**: `MASTER PROMPT — BUILD AI VIDEO EDITOR FOR WINDOWS.md` — spec gốc, xây toàn bộ ứng dụng lần đầu (13 phase, theo dõi ở `IMPLEMENTATION_PLAN.md`, đã 100% hoàn thành).
- **Phần 2**: `Prompt nâng cấp Auto Video Editor tích hợp CapCut + AI.md` — spec nâng cấp lần 1, batch automation + CapCut + AI (theo dõi ở `UPGRADE_PLAN.md`, phase U1-U4, đã 100% hoàn thành).
- **Phần 3**: `promt.md` — spec nâng cấp lần 2 (mới nhất, lớn nhất), Professional Workflow UI + dubbing pipeline (theo dõi ở `STUDIO_PLAN.md`, đang triển khai — xem `PROMPT_AUDIT.md` cho tình trạng thật từng mục, `PLAN_NEXT.md` cho kế hoạch tiếp theo).

---

# PHẦN 1 — MASTER PROMPT GỐC (đã 100% hoàn thành, xem IMPLEMENTATION_PLAN.md)

# MASTER PROMPT — BUILD AI VIDEO EDITOR FOR WINDOWS

You are acting as a Principal Software Architect, Senior Rust/Tauri Engineer, Senior Python Engineer, Senior Video Processing Engineer, and AI Engineer.

Your task is to inspect, understand, refactor, integrate, and extend these two open-source repositories into ONE production-ready Windows desktop application:

Repository A:
https://github.com/Hommy-master/capcut-mate

Repository B:
https://github.com/cobanov/autocut

Do NOT simply copy the repositories together.

The final goal is to build a unified Windows desktop application for automated/AI-assisted video editing.

Working product name:

AI Video Editor

The application must combine:

- AutoCut-style silence/speech detection
- CapCut/Jianying draft automation
- timeline editing
- subtitles/transcription
- AI semantic editing
- automatic removal of filler words
- highlight detection
- short-video generation
- auto reframing
- auto zoom
- B-roll support
- templates
- batch processing
- local project management
- export/render
- CapCut/Jianying draft generation

The final application must be installable on Windows as a normal desktop application.

Target OS:

- Windows 10 x64
- Windows 11 x64

Primary target:

Windows 11 x64.

Do not build a proof-of-concept.

Build the foundation as if this will become a real desktop product.

---

# 1. FIRST TASK — AUDIT BOTH REPOSITORIES

Before implementing anything, completely inspect BOTH repositories.

Do NOT immediately start rewriting code.

Analyze:

## capcut-mate

Understand:

- architecture
- FastAPI backend
- Python packages
- API routes
- models
- services
- draft generation
- CapCut/Jianying integration
- desktop-client
- video handling
- audio handling
- captions
- effects
- masks
- animations
- keyframes
- timelines
- rendering
- local file handling
- URL-based material handling
- configuration
- Windows-specific code
- tests
- build system
- Docker-related components
- authentication if present
- existing limitations

Determine which components can be reused directly and which need refactoring.

## autocut

Understand:

- Tauri architecture
- Rust backend
- Svelte frontend
- FFmpeg integration
- FFprobe integration
- silence detection
- speech detection
- VAD implementation
- audio extraction
- timeline generation
- preview
- export
- FCPXML
- multi-track support
- Windows support
- IPC between UI and Rust
- build/release configuration
- tests

Determine which components should become reusable engine modules.

Create:

docs/architecture-audit.md

The document must contain:

1. capcut-mate architecture
2. autocut architecture
3. reusable components
4. duplicate functionality
5. incompatible components
6. technical risks
7. licensing considerations
8. integration strategy
9. proposed final architecture

Do this BEFORE major implementation.

---

# 2. PRODUCT ARCHITECTURE

Target architecture:

AI Video Editor Desktop
│
├── Desktop UI
│
│   ├── Project Manager
│   ├── Media Library
│   ├── Video Preview
│   ├── Timeline
│   ├── Transcript Editor
│   ├── Caption Editor
│   ├── AI Editor
│   ├── Export
│   └── Settings
│
├── Tauri
│
├── Rust Core
│   ├── FFmpeg
│   ├── FFprobe
│   ├── Media Analyzer
│   ├── Audio Extractor
│   ├── Silence Detector
│   ├── VAD
│   ├── Timeline Engine
│   ├── Cut Engine
│   ├── Render Engine
│   ├── Waveform Engine
│   └── Project Manager
│
├── AI Engine
│   ├── Transcription
│   ├── Word timestamps
│   ├── Filler detection
│   ├── Semantic analysis
│   ├── Highlight detection
│   ├── Caption generation
│   ├── Title generation
│   └── Edit planning
│
└── CapCut Adapter
    ├── Draft generator
    ├── Video
    ├── Audio
    ├── Caption
    ├── Image
    ├── Sticker
    ├── Effect
    ├── Mask
    ├── Animation
    └── Keyframe

Avoid unnecessary microservices.

This is primarily a LOCAL Windows desktop application.

Prefer local IPC rather than HTTP for internal components.

If capcut-mate functionality currently depends heavily on FastAPI, preserve it initially behind an adapter if necessary, but progressively separate core business logic from HTTP routes.

The application should NOT require Docker.

The application should NOT require users to manually install Python.

The application should NOT require users to manually install FFmpeg.

Everything required for normal operation should be packaged or installed automatically.

---

# 3. TECHNOLOGY DIRECTION

Preferred desktop stack:

Tauri 2
Rust
TypeScript
Svelte 5

Reuse autocut's stack wherever reasonable.

Rust should handle:

- filesystem
- FFmpeg
- FFprobe
- video metadata
- audio extraction
- silence detection
- waveform generation
- timeline operations
- local rendering
- heavy media operations
- process management

Python should only remain where it provides substantial value.

If Python components from capcut-mate are required, create a clean sidecar architecture.

Possible structure:

src-tauri/
sidecars/
    capcut-engine/

The Windows installer must package the required runtime.

The end user should NEVER need to:

pip install
uv install
python install
ffmpeg install

manually.

---

# 4. PROJECT STRUCTURE

Refactor toward something similar to:

ai-video-editor/
│
├── src/
│   ├── components/
│   ├── pages/
│   ├── stores/
│   ├── services/
│   ├── timeline/
│   ├── transcript/
│   ├── editor/
│   └── types/
│
├── src-tauri/
│   ├── src/
│   │   ├── commands/
│   │   ├── media/
│   │   ├── ffmpeg/
│   │   ├── audio/
│   │   ├── vad/
│   │   ├── timeline/
│   │   ├── render/
│   │   ├── project/
│   │   ├── capcut/
│   │   └── ai/
│   └── binaries/
│
├── sidecars/
│   └── capcut-engine/
│
├── models/
│
├── templates/
│
├── assets/
│
├── docs/
│
├── tests/
│
└── scripts/

This is a guideline.

Improve it if a better architecture emerges after repository audit.

---

# 5. UNIFIED PROJECT FORMAT

Do NOT make CapCut draft files the application's primary project format.

Create our own project format.

Example:

project.json

Schema concept:

{
  "version": 1,
  "project": {},
  "canvas": {},
  "media": [],
  "tracks": [],
  "clips": [],
  "captions": [],
  "transcript": [],
  "effects": [],
  "animations": [],
  "keyframes": [],
  "cuts": [],
  "ai": {},
  "export": {}
}

Use stable IDs.

All timeline entities must reference IDs rather than fragile array positions.

Design project schema with migration/versioning support.

Implement:

ProjectV1

and a migration layer so future versions can become:

ProjectV2
ProjectV3

without destroying old projects.

---

# 6. PROJECT MANAGEMENT

Create a Windows-friendly project system.

Default location:

%USERPROFILE%\Videos\AI Video Editor\Projects

Each project:

ProjectName/
├── project.json
├── media/
├── proxy/
├── audio/
├── waveform/
├── transcript/
├── thumbnails/
├── cache/
├── drafts/
└── exports/

Features:

New Project
Open Project
Save
Save As
Auto Save
Recent Projects
Duplicate Project
Delete Project
Archive Project

Autosave should protect against crashes.

Implement atomic writes:

project.json.tmp
→ fsync
→ rename project.json

Keep recovery versions.

---

# 7. MEDIA IMPORT

Support:

MP4
MOV
MKV
AVI
WEBM
M4V
MP3
WAV
AAC
M4A
FLAC
PNG
JPG
JPEG
WEBP

Features:

drag & drop

multi-file import

folder import

file picker

media metadata extraction

thumbnail generation

duration detection

resolution

FPS

codec

bitrate

audio channels

sample rate

rotation metadata

creation timestamp if available

Use FFprobe.

Do not load huge video files completely into memory.

---

# 8. PROXY MEDIA

Large videos must remain responsive.

Implement optional proxy generation.

Example:

4K source
→ 720p editing proxy

Editing uses proxy.

Final render uses original media.

Allow:

Proxy OFF
Proxy Auto
Proxy Always

Show proxy generation progress.

---

# 9. VIDEO PREVIEW

Build a proper preview panel.

Features:

Play
Pause
Stop
Seek
Frame step forward
Frame step backward
Playback speed

0.25x
0.5x
1x
1.5x
2x

Current time

Total duration

Volume

Mute

Fullscreen

Canvas ratios:

16:9
9:16
1:1
4:5
custom

Preview should follow timeline edits.

---

# 10. TIMELINE EDITOR

This is a CRITICAL component.

Implement a non-destructive multi-track timeline.

Track types:

Video
Audio
Caption
Image
Overlay
Effect

Required functionality:

drag clips

resize clips

trim start

trim end

split

delete

duplicate

move

snap

zoom timeline

horizontal scroll

multi-select

lock track

hide video track

mute audio track

solo audio track

undo

redo

copy

paste

keyboard shortcuts

playhead

selection region

timeline ruler

waveform

thumbnail strip

Markers.

Timeline units internally should use microseconds or another high-precision integer representation.

Avoid floating-point drift.

---

# 11. UNDO / REDO

Implement command-based undo/redo.

Examples:

SplitClipCommand
MoveClipCommand
TrimClipCommand
DeleteClipCommand
AddCaptionCommand

Do not implement undo by copying the entire project after every small operation.

Set reasonable history limits.

---

# 12. SILENCE REMOVAL — AUTOCUT ENGINE

Reuse/refactor autocut's strongest implementation.

Create:

Silence Detector

Parameters:

silence threshold

minimum silence duration

minimum speech duration

padding before

padding after

merge nearby speech

audio channel selection

analysis track selection

Display detected regions visually.

Example:

SPEECH
SILENCE
SPEECH
SILENCE

Allow user to preview BEFORE applying.

Actions:

Analyze

Preview Cuts

Apply Cuts

Reset

Do not permanently modify source media.

Generate timeline edits.

---

# 13. VAD

Add proper Voice Activity Detection.

Support at least one strong local VAD implementation.

Architecture:

VadProvider

Possible implementations:

Silero VAD
WebRTC VAD
existing autocut detector

Do NOT tightly couple application logic to one model.

Interface concept:

trait VadProvider {
    analyze(audio) -> Vec<SpeechSegment>
}

Return:

start
end
confidence

Provide sensitivity configuration.

---

# 14. TRANSCRIPTION

Implement local transcription.

Preferred architecture:

TranscriptionProvider

Support:

Whisper / whisper.cpp / faster-whisper

Evaluate which solution packages best on Windows.

Prefer GPU acceleration when available.

Fallback to CPU.

Support:

NVIDIA CUDA if feasible

CPU

Models:

tiny
base
small
medium
large

Do NOT force large model download during application installation.

Create Model Manager.

Model Manager:

Installed models
Available models
Download
Delete
Model size
Language support
Storage location

Transcription result MUST support timestamps.

Prefer word-level timestamps.

Schema:

{
  "text": "...",
  "start": 12.31,
  "end": 12.89,
  "confidence": 0.94
}

---

# 15. TRANSCRIPT-BASED EDITING

Create a transcript editor similar conceptually to modern text-based video editors.

Display:

video timeline

and synchronized transcript.

Click word
→ seek video.

Select sentence
→ select timeline range.

Delete transcript text
→ optionally create timeline cut.

Important:

Never silently delete video when user edits text.

Modes:

Transcript Text Edit

Video Edit Through Transcript

Clearly distinguish them.

---

# 16. FILLER WORD REMOVAL

Detect filler words.

Vietnamese examples:

ờ
ừ
ừm
à
ờm
kiểu như

English:

uh
um
erm
you know
like

Allow custom dictionary.

Detection must use transcript timestamps.

Show candidates first.

User can:

Select all
Deselect
Preview
Apply

Add configurable padding so speech is not cut unnaturally.

---

# 17. AI PROVIDER ARCHITECTURE

Do not lock AI features to a single provider.

Create:

AIProvider

Support adapters for:

OpenAI-compatible APIs
Anthropic
Google Gemini
Ollama
Custom OpenAI-compatible endpoint

Store credentials securely using Windows Credential Manager or equivalent secure storage.

NEVER store API keys plaintext inside project.json.

Settings:

Provider
Base URL
API Key
Model
Temperature
Timeout

Implement connection test.

---

# 18. AI EDIT PLAN

AI must NEVER directly mutate the timeline.

AI produces a structured edit plan.

Example:

{
  "version": 1,
  "operations": [
    {
      "type": "remove",
      "start": 12.3,
      "end": 15.7,
      "reason": "long pause"
    },
    {
      "type": "zoom",
      "start": 32,
      "end": 36,
      "scale": 1.12
    }
  ]
}

Pipeline:

AI
↓
JSON Schema validation
↓
Edit Plan Preview
↓
User Approves
↓
Timeline Engine

Use strict schemas.

Reject malformed AI output.

---

# 19. AI SEMANTIC EDITING

Create:

Smart Edit

AI analyzes transcript and suggests:

repetition removal

false starts

off-topic sections

weak sentences

long pauses

filler words

unnecessary introductions

duplicate ideas

boring sections

Possible actions:

KEEP
REMOVE
SHORTEN
HIGHLIGHT

Every recommendation MUST contain:

time range
transcript
reason
confidence
suggested action

User decides whether to apply.

---

# 20. NATURAL LANGUAGE VIDEO EDITING

Add AI command box.

Examples:

"Remove all silence longer than 800ms."

"Remove filler words."

"Turn this into a 60 second TikTok."

"Find the 5 best highlights."

"Add captions."

"Zoom in when the speaker says something important."

"Remove the intro."

"Make this video faster."

"Create 3 shorts."

Architecture:

Natural language
↓
AI Provider
↓
EditPlan
↓
Schema validation
↓
Preview
↓
Apply

Never let arbitrary LLM output execute shell commands.

---

# 21. HIGHLIGHT DETECTION

Implement AI-assisted highlight detection.

Use:

transcript

speech density

audio energy

scene changes

semantic importance

optional face/speaker information

Return:

start
end
score
title
reason

UI:

Highlight #1
00:03:14 → 00:03:52
Score: 92

Allow:

Preview
Add to timeline
Create new project
Export clip

---

# 22. SHORT VIDEO GENERATOR

Create:

Long Video → Shorts

Input:

video

Target:

TikTok
YouTube Shorts
Instagram Reels

Settings:

duration:

15s
30s
60s
90s
custom

aspect:

9:16
1:1
4:5

number of clips:

1
3
5
10

Pipeline:

Transcription
↓
Highlight Detection
↓
Candidate Ranking
↓
Clip Extraction
↓
Reframe
↓
Captions
↓
Optional Zoom
↓
Export

Each generated short should remain editable.

---

# 23. AUTO REFRAME

Convert landscape video to portrait.

Example:

1920x1080
→
1080x1920

Do NOT simply center crop.

Implement subject tracking architecture.

Possible techniques:

face detection

person detection

motion tracking

active speaker position

Keep provider abstraction:

SubjectTracker

Return normalized target coordinates over time.

Generate smooth position keyframes.

Prevent camera jumping.

Use smoothing/interpolation.

---

# 24. AUTO ZOOM

Create intelligent zoom.

Use keyframes.

Settings:

Off
Low
Medium
High

Example:

1.0
→ 1.08
→ 1.0
→ 1.12

Trigger based on:

important sentence
speaker emphasis
long static scene
manual markers

Avoid excessive zoom.

---

# 25. SCENE DETECTION

Implement scene/cut detection.

Return:

Scene {
  start
  end
  thumbnail
  score
}

Display scene markers on timeline.

Allow:

split at scenes

select scenes

remove scenes

generate highlights from scenes.

---

# 26. CAPTIONS

Generate captions from transcript.

Features:

word-level timing

sentence captions

max words per line

max characters

line wrapping

font

font size

bold

italic

alignment

position

background

outline

shadow

opacity

safe margins

Templates:

Minimal
TikTok
Podcast
News
Gaming
Karaoke

---

# 27. ACTIVE WORD CAPTIONS

Support karaoke-style captions.

Example:

THIS is a VERY important sentence

Current spoken word gets highlighted.

Do NOT generate one UI object per word if that creates severe performance issues.

Design an efficient caption rendering model.

---

# 28. CAPTION CORRECTION

Allow users to edit transcript/caption text without retranscribing.

Maintain timestamps when possible.

Provide:

split caption

merge captions

retime

drag boundaries

find/replace

bulk style.

---

# 29. CAPCUT INTEGRATION

Refactor/reuse capcut-mate.

Create a clean:

CapCutAdapter

The core application must NOT depend directly on capcut-mate HTTP endpoints.

Expose internal functions:

create_draft

add_video

add_audio

add_image

add_caption

add_sticker

add_effect

add_mask

add_animation

add_keyframe

save_draft

export_draft

Map:

Unified Project
↓
CapCutAdapter
↓
CapCut/Jianying Draft

The unified project remains source of truth.

CapCut draft is an EXPORT FORMAT.

---

# 30. CAPCUT DETECTION

On Windows detect installed CapCut/Jianying locations.

Do not hard-code only one path.

Search known installation paths and optionally registry entries.

Settings:

Detected CapCut:

Version
Path
Draft Directory

Allow manual override.

Never overwrite user drafts without confirmation.

---

# 31. EXPORT TO CAPCUT

Button:

Export to CapCut

Options:

Create New Draft
Update Existing Draft

Default must be:

Create New Draft

Export:

timeline

cuts

captions

audio

images

effects where supported

animations where supported

keyframes where supported.

If an application feature cannot map to CapCut:

show warning.

Example:

"3 effects cannot be represented in the selected CapCut version."

Do not silently lose edits.

---

# 32. LOCAL RENDERING

Do NOT rely solely on CapCut for final video.

Implement local FFmpeg rendering.

Export:

MP4 H.264
MP4 H.265
WebM

Presets:

Fast Preview
1080p
1440p
4K
TikTok 1080x1920
YouTube 1080p
YouTube 4K

Settings:

resolution
FPS
codec
bitrate
CRF
audio bitrate
hardware acceleration

---

# 33. HARDWARE ACCELERATION

Detect GPU capabilities.

Support when available:

NVIDIA NVENC
Intel Quick Sync
AMD hardware encoding

Fallback:

libx264
libx265

Do capability detection rather than assuming hardware exists.

Show active encoder.

Example:

Encoder:
NVIDIA NVENC

or:

CPU — libx264

---

# 34. B-ROLL SYSTEM

Implement B-roll architecture.

Sources:

Local media library
User-selected folders
Optional external providers later

AI can suggest:

keyword
start
end
duration
reason

Example:

Transcript:
"Bitcoin reached a new high..."

AI:

{
  "keyword": "bitcoin price chart",
  "start": 32.5,
  "duration": 3
}

Do NOT automatically download copyrighted media from arbitrary websites.

Initially support:

local B-roll search

and provider interfaces for future integrations.

---

# 35. MEDIA SEARCH

Index local media library.

Metadata:

filename
path
duration
resolution
tags
created
type

Optional AI-generated tags.

Search:

football
bitcoin
city
computer
person

Keep indexing database separate from project.json.

SQLite is acceptable.

---

# 36. TEMPLATES

Create reusable project/edit templates.

Directory:

templates/

Built-in templates:

Talking Head
Podcast
TikTok
YouTube Shorts
News
Tutorial
Gaming
Football Highlight

Template contains:

canvas
caption style
zoom settings
silence settings
transition settings
export preset
AI prompt configuration

Allow:

Save as Template
Import Template
Export Template

---

# 37. FOOTBALL / SPORTS TEMPLATE

Add a generic sports-highlight template.

Do NOT depend on proprietary assets.

Features:

16:9 and 9:16

high-energy captions

optional score/title overlay

fast transitions

highlight markers

slow-motion sections

replay markers

music track

logo overlay

This should remain generic.

---

# 38. AUDIO FEATURES

Implement:

volume

mute

fade in

fade out

normalize

noise reduction architecture

ducking

music track

voice track

Audio waveform.

Auto duck:

when speech exists:

music volume ↓

when speech stops:

music volume ↑

Parameters:

duck level
attack
release.

---

# 39. MULTI-TRACK SUPPORT

Preserve/refactor autocut's multi-track concepts.

Example:

Camera 1
Camera 2
Screen
Microphone
Music

When silence cut is applied based on microphone:

all linked tracks should cut together.

Introduce:

ClipGroup

or:

SyncGroup

to keep synchronized media aligned.

---

# 40. MULTI-CAMERA

Architecture should support future multi-camera editing.

Initial functionality:

synchronized tracks

select active camera

manual camera switching

Later AI can detect active speaker.

Do not over-engineer full professional multicam in first implementation, but do not create an architecture that prevents it.

---

# 41. FCPXML

Preserve autocut's FCPXML export.

Support export for workflows such as DaVinci Resolve where practical.

Export:

timeline cuts
source references
timecode

Do not remove existing working functionality unless there is a strong technical reason.

---

# 42. BATCH PROCESSING

Add Batch Jobs.

Example:

100 videos
↓
Remove silence
↓
Generate captions
↓
Apply template
↓
Export

UI:

Jobs

Columns:

Name
Status
Progress
Stage
Elapsed
ETA
Output

States:

Queued
Analyzing
Transcribing
Editing
Rendering
Completed
Failed
Cancelled

Allow:

pause
resume where technically possible
cancel
retry.

---

# 43. JOB SYSTEM

Long operations must NEVER freeze the UI.

Create internal JobManager.

Operations:

proxy generation

transcription

silence analysis

scene detection

AI analysis

rendering

CapCut export

model download

Progress events should flow:

Rust/sidecar
→ Tauri event
→ frontend store
→ UI.

---

# 44. CANCELLATION

Every long-running task should support cancellation where feasible.

Examples:

FFmpeg render

model download

transcription

proxy generation

scene analysis

batch export

When cancelled:

kill child processes cleanly.

Delete incomplete temporary files when safe.

---

# 45. WINDOWS PROCESS MANAGEMENT

Be careful with Windows process trees.

When cancelling FFmpeg/Python sidecars, ensure child processes are terminated correctly.

Avoid zombie/orphan processes.

Do not leave:

ffmpeg.exe
python.exe
sidecar.exe

running after application exits.

---

# 46. APPLICATION SETTINGS

Sections:

General
Editing
AI
Transcription
Performance
Storage
CapCut
Export
Shortcuts
Updates
About

General:

language
theme
autosave
recent project count

Editing:

snap
timeline FPS
default canvas

Performance:

CPU threads
GPU acceleration
proxy
cache size

Storage:

project path
cache path
model path
temporary path

---

# 47. LANGUAGE

Initial UI languages:

English
Vietnamese

Build proper i18n.

Do NOT hardcode every string directly in components.

Example:

locales/
    en.json
    vi.json

Make architecture ready for more languages.

---

# 48. WINDOWS UI/UX

Build a professional dark desktop editing interface.

Layout concept:

┌─────────────────────────────────────────────────────────────┐
│ Menu / Toolbar                                              │
├──────────────┬──────────────────────────┬───────────────────┤
│ Media        │                          │ Inspector         │
│ Transcript   │      Video Preview       │ AI Edit           │
│ Templates    │                          │ Properties        │
│ AI           │                          │                   │
├──────────────┴──────────────────────────┴───────────────────┤
│ Timeline                                                    │
│ V1 ──────────────────────────────────────────────────────── │
│ V2 ──────────────────────────────────────────────────────── │
│ A1 ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~ │
│ CC ──────────────────────────────────────────────────────── │
└─────────────────────────────────────────────────────────────┘

Resizable panels.

Persist layout.

Support Windows scaling:

100%
125%
150%
175%
200%.

---

# 49. KEYBOARD SHORTCUTS

Implement common editing shortcuts.

Space
Play/Pause

Ctrl+S
Save

Ctrl+Z
Undo

Ctrl+Shift+Z
Redo

Ctrl+C
Copy

Ctrl+V
Paste

Delete
Delete selected

S
Split at playhead

Left/Right
Seek

Shift+Left/Right
larger seek

Ctrl++
Timeline zoom in

Ctrl+-
Timeline zoom out

Allow customization later.

---

# 50. PERFORMANCE

Application must remain usable with:

2+ hour video

4K video

thousands of transcript words

thousands of captions

large timelines

Do not render every timeline object continuously.

Use virtualization where necessary.

Debounce expensive UI updates.

Do not send giant binary media through Tauri IPC.

Pass file paths/metadata instead.

Use streaming/process-based operations.

---

# 51. CACHE MANAGEMENT

Cache:

proxies

thumbnails

waveforms

transcription intermediate files

temporary renders

scene thumbnails

AI analysis

Provide:

cache size

clear cache

maximum cache size

automatic cleanup.

Never delete original media.

---

# 52. DATABASE

Use SQLite only where it makes sense.

Good candidates:

media index

recent projects

model registry

job history

application preferences if needed

Do not unnecessarily put entire editable project timeline into SQLite.

project.json remains portable.

---

# 53. SECURITY

Do not execute arbitrary shell commands generated by AI.

Validate file paths.

Prevent path traversal.

Validate downloaded model hashes when possible.

Secure API keys.

Validate sidecar messages.

Restrict localhost services if any remain.

If a local FastAPI service is retained:

bind:

127.0.0.1

NOT:

0.0.0.0

unless explicitly configured.

Use random/ephemeral ports if practical.

---

# 54. CRASH HANDLING

Implement:

structured logs

panic/error capture

recovery project

failed job details

Log directory:

%LOCALAPPDATA%\AI Video Editor\logs

Provide:

Open Logs Folder

Do not display giant stack traces to normal users.

Show:

"Render failed."

Details button:

FFmpeg error...

---

# 55. LOGGING

Use structured logs.

Levels:

ERROR
WARN
INFO
DEBUG
TRACE

Never log:

API keys
tokens
sensitive headers

Allow debug logging from settings.

---

# 56. ERROR MODEL

Create standardized application errors.

Examples:

MediaError
FfmpegError
TranscriptionError
AiProviderError
CapCutError
ProjectError
RenderError
ModelError

Frontend receives:

code
message
details
recoverable
suggested_action

---

# 57. WINDOWS INSTALLER

Produce proper Windows installer.

Preferred:

Tauri MSI and/or NSIS.

Output example:

AI-Video-Editor-Setup-x64.exe

Requirements:

desktop shortcut optional

start menu entry

uninstaller

application icon

version information

publisher metadata placeholder

upgrade support

user data preserved during upgrade.

---

# 58. FIRST-RUN EXPERIENCE

First launch wizard:

Welcome

↓

System Check

↓

FFmpeg

↓

GPU Detection

↓

CapCut Detection

↓

AI Provider optional

↓

Transcription Model optional

↓

Project Folder

↓

Ready

AI configuration must be optional.

The basic editor should work without cloud AI.

---

# 59. FFmpeg PACKAGING

Package appropriate FFmpeg/FFprobe Windows binaries legally and according to their license requirements.

Do not download random binaries at runtime.

Document source/version/license.

Implement:

ffmpeg_version()

ffprobe_version()

Show in About/System Information.

---

# 60. MODEL DOWNLOAD MANAGER

Models can be large.

Support resumable downloads if feasible.

Show:

filename
model
size
downloaded
speed
ETA

Download into temporary partial file:

model.bin.part

Then verify.

Then atomic rename.

Do not treat partially downloaded models as installed.

---

# 61. OFFLINE MODE

Application core must work offline for:

manual editing

silence removal

local transcription if model installed

caption editing

local rendering

CapCut draft generation where technically possible

Cloud AI features may require internet.

Clearly distinguish:

Local
Cloud

features.

---

# 62. AUTO UPDATE

Prepare architecture for application updates.

Use Tauri updater or suitable supported mechanism.

Do not force-update users while they are editing.

Options:

Automatically check
Notify only
Disabled

Never update while rendering.

---

# 63. TESTING

Add meaningful tests.

Rust:

unit tests
timeline tests
silence tests
project serialization
migration tests
FFmpeg argument generation
CapCut mapping tests

Frontend:

component tests where valuable

state/store tests

timeline operation tests

Python:

tests for remaining capcut engine modules

Integration:

import video
analyze
cut
save
reload
render

Test Windows path cases:

spaces

Unicode

Vietnamese filenames

very long paths where possible.

---

# 64. SAMPLE TEST PROJECT

Create automated/sample test media where licensing allows.

Test scenario:

Import

↓

Analyze silence

↓

Apply cuts

↓

Transcribe

↓

Generate captions

↓

Save

↓

Reload

↓

Render

↓

Export CapCut draft

Validate outputs.

---

# 65. CI/CD

Create GitHub Actions.

On pull request:

frontend lint

TypeScript check

Rust fmt

Clippy

Rust tests

Python tests if applicable

build check

On version tag:

Windows build

installer

checksums

release artifacts.

Do not automatically publish unsigned binaries without making that behavior explicit.

---

# 66. CODE QUALITY

Rules:

No giant god files.

No duplicated timeline logic.

No business logic in Svelte components.

No direct FFmpeg string concatenation scattered throughout code.

Create FFmpeg command builders.

No arbitrary unwrap() in important Rust paths.

Use Result properly.

Use typed IPC payloads.

Use strict TypeScript.

Avoid `any`.

Use formatting/linting.

Document non-obvious media/timebase logic.

---

# 67. TIMEBASE

Video editing timebase is critical.

Define one canonical internal representation.

Prefer integer microseconds or rational timestamps.

Document:

source timebase

timeline timebase

frame conversion

FFmpeg conversion

CapCut conversion

FCPXML conversion

Never casually mix:

milliseconds

microseconds

seconds

frames.

Create centralized time conversion utilities.

---

# 68. EDITING MUST BE NON-DESTRUCTIVE

Original files must never be modified.

All edits should be represented as:

timeline operations

project metadata

render instructions.

Source:

D:\Videos\source.mp4

must remain unchanged.

---

# 69. RENDER GRAPH

Create a clean intermediate render representation.

Project
↓
RenderGraph
↓
FFmpeg Plan
↓
FFmpeg

Do not let UI construct FFmpeg commands.

RenderGraph should represent:

inputs

cuts

scale

crop

overlay

audio

captions

effects

output.

This also allows future render backends.

---

# 70. CAPCUT EXPORT GRAPH

Similarly:

Project
↓
CapCutExportGraph
↓
CapCutAdapter
↓
Draft

Do not mix CapCut-specific IDs/structures throughout core timeline code.

---

# 71. FEATURE COMPATIBILITY MATRIX

Create:

docs/feature-matrix.md

Example:

Feature | Internal | FFmpeg | CapCut | FCPXML

Cut | yes | yes | yes | yes
Caption | yes | yes | yes | partial
Zoom | yes | yes | yes | partial
Effect X | yes | yes | no | no

The UI can use this to warn users before exporting.

---

# 72. MIGRATION FROM BOTH ORIGINAL PROJECTS

Keep original repositories available for reference.

Do not destroy history unnecessarily.

Create documentation:

docs/upstream.md

Document:

code originating from autocut

code originating from capcut-mate

modified modules

license requirements

upstream commit hashes

This makes future upstream synchronization possible.

---

# 73. LICENSE COMPLIANCE

Inspect licenses carefully.

Preserve required:

LICENSE
NOTICE
copyright headers
attribution

Do not assume all bundled assets/models/fonts/FFmpeg binaries have the same license as the source code.

Audit each dependency/resource separately.

Create:

THIRD_PARTY_NOTICES.md

---

# 74. MVP PRIORITY

Do NOT attempt all advanced AI features simultaneously if it makes the application unstable.

Implementation priority:

P0

Application starts on Windows
Project create/open/save
Media import
Video preview
Timeline
FFmpeg/FFprobe
Split/trim/delete
Undo/redo
Silence detection
Apply silence cuts
Render MP4

P1

Transcription
Transcript editor
Captions
Filler removal
CapCut export
Proxy media
Waveforms
Scene detection

P2

AI semantic editing
Highlight detection
Short generator
Auto reframe
Auto zoom
Templates
Batch jobs

P3

B-roll AI
advanced multi-camera
additional AI providers
plugin architecture
advanced effects

However:

Design architecture NOW so P2/P3 do not require rewriting the entire application.

---

# 75. DO NOT FAKE FEATURES

This requirement is extremely important.

Do NOT create buttons that do nothing.

Do NOT mark TODO features as completed.

Do NOT return fake progress.

Do NOT fake AI output.

Do NOT fake render completion.

If feature is not implemented:

disable it

or label:

Experimental
Coming Soon

But prioritize actually implementing P0/P1.

---

# 76. DEVELOPMENT EXECUTION PLAN

Work in phases.

PHASE 0

Repository audit.

PHASE 1

Architecture + unified project schema.

PHASE 2

Tauri Windows shell.

PHASE 3

Media engine.

PHASE 4

Timeline.

PHASE 5

AutoCut integration.

PHASE 6

Rendering.

PHASE 7

Transcription.

PHASE 8

Captions.

PHASE 9

CapCut adapter.

PHASE 10

AI edit-plan architecture.

PHASE 11

Short generator.

PHASE 12

Windows packaging.

PHASE 13

Testing/performance/security.

At the end of EVERY phase:

1. compile
2. run tests
3. fix errors
4. update docs
5. commit logical changes if git access is available

Do not continue while the project is fundamentally broken.

---

# 77. BUILD COMMANDS

Provide scripts such as:

scripts/dev.ps1

scripts/test.ps1

scripts/build.ps1

scripts/package.ps1

Desired usage:

.\scripts\dev.ps1

.\scripts\test.ps1

.\scripts\build.ps1

.\scripts\package.ps1

Final package should appear somewhere obvious such as:

dist/windows/

---

# 78. SYSTEM DIAGNOSTICS

Add:

Settings → System Information

Display:

Application version
Windows version
CPU
RAM
GPU
FFmpeg version
FFprobe version
Hardware encoders
CapCut detected version
CapCut path
Transcription backend
Installed models
Cache directory
Project directory

Button:

Copy System Information

Useful for bug reports.

---

# 79. FINAL README

Rewrite README for the resulting product.

README must contain:

What is AI Video Editor?

Screenshots placeholders

Features

Windows requirements

Installation

Development setup

Build instructions

Architecture overview

AI configuration

Transcription models

CapCut integration

FFmpeg information

Troubleshooting

License

Third-party notices

Do not leave README looking like two repositories pasted together.

---

# 80. DOCUMENTATION

At minimum create:

docs/
├── architecture.md
├── architecture-audit.md
├── project-format.md
├── timeline.md
├── render-engine.md
├── autocut-engine.md
├── transcription.md
├── ai-engine.md
├── capcut-integration.md
├── windows-build.md
├── troubleshooting.md
├── feature-matrix.md
└── upstream.md

---

# 81. IMPORTANT IMPLEMENTATION PRINCIPLE

Think of the application as:

SOURCE MEDIA
     │
     ▼
MEDIA ANALYSIS
     │
     ├── FFprobe
     ├── Waveform
     ├── VAD
     ├── Scene Detection
     └── Transcription
     │
     ▼
UNIFIED PROJECT
     │
     ▼
TIMELINE ENGINE
     │
     ├──────────────┐
     │              │
     ▼              ▼
MANUAL EDIT       AI EDIT
                    │
                    ▼
                 EditPlan
                    │
                    ▼
                 Preview
                    │
                    ▼
                  Apply
     │
     ▼
UNIFIED TIMELINE
     │
     ├───────────────┬────────────────┐
     ▼               ▼                ▼
FFmpeg Render    CapCut Export      FCPXML
     │               │                │
     ▼               ▼                ▼
MP4            CapCut Draft      DaVinci/etc.

This separation is mandatory.

---

# 82. AI SAFETY / RELIABILITY PRINCIPLE

LLM output is UNTRUSTED DATA.

Never:

LLM
→ shell

Never:

LLM
→ raw FFmpeg command

Never:

LLM
→ filesystem operation

Instead:

LLM
↓
EditPlan JSON
↓
Schema validation
↓
Business-rule validation
↓
Preview
↓
Timeline command
↓
RenderGraph
↓
FFmpeg builder

---

# 83. USER EXPERIENCE TARGET

Eventually the user should be able to do this:

Open application.

Drag:

podcast.mp4

Then click:

AI Edit

Choose:

Remove Silence
Remove Filler Words
Generate Captions
Create Shorts

Application performs:

audio extraction

VAD

transcription

semantic analysis

highlight analysis

and shows recommendations.

User presses:

Apply

Timeline updates.

Then user can choose:

Export Video

or:

Export to CapCut

or:

Export FCPXML.

This is the core product experience.

---

# 84. EXAMPLE ADVANCED WORKFLOW

Input:

2-hour podcast.mp4

Application:

1. Analyze media
2. Create proxy
3. Generate waveform
4. Detect speech
5. Detect silence
6. Transcribe
7. Detect filler words
8. Detect repeated ideas
9. Detect highlights
10. Detect scenes
11. Generate captions

AI suggests:

Remove:
18m 32s silence

Remove:
43 filler words

Remove:
7 repeated sections

Highlights:

#1 00:32:10 → 00:33:02
#2 01:04:20 → 01:05:14
#3 01:37:51 → 01:38:34

User chooses:

Generate 3 Shorts.

Application creates:

short-01
short-02
short-03

Each:

9:16
auto reframe
captions
optional auto zoom

Then:

Export All

This workflow should guide architectural decisions.

---

# 85. WINDOWS PERFORMANCE TARGET

The application should not freeze during analysis or rendering.

Target normal memory usage should remain reasonable.

Large media must be processed incrementally.

Use bounded concurrency.

Do not start 20 FFmpeg processes because the user imported 20 videos.

Implement configurable worker concurrency.

Default conservatively based on CPU/RAM.

---

# 86. RECOVERY

If application crashes during:

transcription

render

analysis

the original project must remain valid.

Store job state separately.

Temporary artifacts should be identifiable.

At next startup:

"AI Video Editor did not shut down correctly."

Options:

Recover Project
Discard Recovery
Open Logs

---

# 87. PORTABLE PROJECTS

Add later-compatible support for:

Collect Project Files

Example:

Project references:

D:\Videos\a.mp4
E:\Audio\b.wav

User selects:

Collect Project

Application creates:

MyProject/
    project.json
    media/
        a.mp4
        b.wav

Update project paths to relative paths.

Useful for moving projects between PCs.

Design path abstraction now.

---

# 88. PATH HANDLING

Windows path handling must be first-class.

Test:

C:\Users\Alex\Videos\a.mp4

D:\My Videos\Test Video.mp4

C:\Video tiếng Việt\phỏng vấn 01.mp4

UNC paths if feasible.

Never build commands by naïvely concatenating quoted strings.

Use process argument arrays.

---

# 89. FINAL ACCEPTANCE CRITERIA

Do not consider the project successful merely because:

npm run dev

opens a window.

Minimum acceptance test:

1. Install application on clean Windows environment.

2. Launch.

3. Create project.

4. Import MP4.

5. Preview MP4.

6. Generate waveform.

7. Analyze silence.

8. Preview detected cuts.

9. Apply cuts.

10. Split another clip manually.

11. Undo.

12. Redo.

13. Save project.

14. Close application.

15. Reopen project.

16. Timeline remains identical.

17. Render MP4.

18. Output plays correctly.

19. Generate transcript.

20. Generate captions.

21. Export CapCut draft.

22. Application closes without orphan processes.

23. Installer can uninstall cleanly without deleting user projects.

---

# 90. HOW YOU SHOULD WORK

You are authorized to modify/refactor the codebase substantially.

Do not repeatedly ask me trivial implementation questions.

When there is a reasonable engineering choice:

inspect the existing code,

evaluate alternatives,

choose the most maintainable option,

document the decision,

implement it.

Ask me only when a decision materially changes product behavior or requires credentials/licensed external services that cannot be inferred.

Do not stop after analysis.

Analysis is Phase 0.

After architecture audit, proceed with implementation.

Do not give me only code snippets.

Actually create/update files in the repository.

Run commands.

Compile.

Run tests.

Inspect failures.

Fix failures.

Repeat until the current phase works.

---

# 91. WHEN YOU ENCOUNTER EXISTING WORKING CODE

Prefer:

adapt
extract
wrap
refactor

over unnecessary rewrites.

Especially preserve proven logic from autocut for:

FFmpeg
silence detection
multi-track handling
FCPXML

and proven logic from capcut-mate for:

CapCut draft structures
captions
materials
effects
animations
masks
keyframes.

But remove coupling to their original UI/API architecture where necessary.

---

# 92. DO NOT BLINDLY TRUST UPSTREAM

Test everything.

CapCut internal formats may vary between versions.

FFmpeg behavior may vary between builds.

Windows paths behave differently from Unix paths.

GPU encoders may exist but fail at runtime.

AI models may fail to download.

Transcription may run out of VRAM.

Therefore implement:

capability detection

validation

fallbacks

clear errors.

---

# 93. START NOW

Start by:

1. Inspecting both repositories completely.

2. Recording their current git commit hashes.

3. Reading their LICENSE files.

4. Mapping both directory trees.

5. Identifying reusable modules.

6. Identifying the current Windows build paths.

7. Identifying all FFmpeg/FFprobe handling.

8. Identifying all CapCut/Jianying draft logic.

9. Identifying silence/VAD/timeline logic.

10. Creating:

docs/architecture-audit.md

11. Proposing the exact unified repository structure.

12. Creating an implementation checklist:

IMPLEMENTATION_PLAN.md

Use checkboxes:

[ ]
[x]

and maintain this file throughout the project.

13. Then begin P0 implementation.

Do not stop at the plan unless blocked by a real external dependency.

The final objective is:

ONE Windows desktop installer

that provides:

Manual Video Editing
+
AutoCut
+
Transcription
+
AI Editing
+
Caption Editing
+
Short Generator
+
CapCut Draft Automation
+
Local Video Rendering.

Build it as one coherent product, not as two open-source projects glued together.
---

# PHẦN 2 — NÂNG CẤP CAPCUT + AI AUTOMATION (đã 100% hoàn thành, xem UPGRADE_PLAN.md)

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
---

# PHẦN 3 — PROFESSIONAL WORKFLOW UI + DUBBING PIPELINE (đang triển khai, xem STUDIO_PLAN.md + PROMPT_AUDIT.md)

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
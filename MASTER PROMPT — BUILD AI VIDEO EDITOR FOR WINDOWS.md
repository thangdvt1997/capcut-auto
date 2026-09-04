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
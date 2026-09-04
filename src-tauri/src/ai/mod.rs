//! `AIProvider` trait + adapters (OpenAI-compatible, Anthropic, Gemini,
//! Ollama, custom endpoint), `EditPlan` schema validation. The AI layer
//! only ever produces a validated `EditPlan` JSON document — it never
//! mutates the timeline directly; only `timeline`'s command layer does that
//! (master prompt §18/§82).
//!
//! Not implemented yet — this module is an intentionally empty, honest
//! placeholder. Lands in Phase 10 (`IMPLEMENTATION_PLAN.md`).

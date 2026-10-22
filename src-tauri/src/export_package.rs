//! Export a recording as a package (docs/EXPORT.md): a folder or a .zip with
//! the recording as Markdown and as a printable web page, the screens and
//! photos the user picked, the transcript, and the review guide and
//! flashcards if they were made.
//!
//! - `build`: the package itself, from plain data (pure, tested)
//! - `ai`: the export's AI tools (highlights, screen captions, picking
//!   screens): prompts and answer parsing (pure, tested)
//! - `commands`: what the Export sheet calls
//!
//! Nothing here stores anything: AI captions and highlights live in the sheet
//! until they're written into the user's file, so Delete and Strike have
//! nothing new to purge (docs/REDACTION.md). Text always comes through
//! `redaction::render_plain`, so stricken text exports as the marker.
//! Exporting is free (data portability, docs/PRO.md); the AI tools go
//! through `ai::client::complete`, which gates Pro and consent.

pub mod ai;
pub mod build;
pub mod commands;

#[cfg(test)]
mod tests;

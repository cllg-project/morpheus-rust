//! `morpheus edit` — local web UI for browsing, editing and adding lemmas.
//! Edits land in an overlay directory (stemlib source format, beta-code);
//! the upstream stemlib stays untouched.

pub mod metadata;
pub mod raw_index;
pub mod server;

pub use server::run_edit_server;

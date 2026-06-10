//! Debug probe: dump stem-dict entries for a normalized stem and ending entries
//! for a normalized ending. Usage:
//!   cargo run --example probe -- <morphlib> stem <norm>
//!   cargo run --example probe -- <morphlib> end <norm>
//!   cargo run --example probe -- <morphlib> lemma <lemma>
use morpheus_core::stemlib::{Language, StemlibIndex};
use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let lib = StemlibIndex::load(Path::new(&args[1]), Language::Greek).unwrap();
    let kind = args[2].as_str();
    let query = args[3].as_str();
    match kind {
        "stem" => {
            for e in lib.stem_dict.get_by_stem(query) {
                println!("{} | {} | {} | {:?}", e.lemma, e.stem, e.key_str, e.kind);
            }
        }
        "end" => {
            for e in lib.end_index.get_by_ending(query) {
                println!("{} | {} | {}", e.ending, e.stem_type_name, e.ending_norm);
            }
        }
        "lemma" => {
            for e in lib.stem_dict.get_by_lemma(query) {
                println!("{} | {} | {} | {:?}", e.stem_norm, e.stem, e.key_str, e.kind);
            }
        }
        _ => eprintln!("unknown kind"),
    }
}

//! Time the stemlib load phases. Usage: cargo run --release --example bench_load -- <morphlib>
use morpheus_core::stemlib::{Language, StemlibIndex};
use std::path::Path;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = Path::new(&args[1]);
    let t0 = Instant::now();
    let lib = StemlibIndex::load(path, Language::Greek).unwrap();
    println!("total load: {:?}", t0.elapsed());
    println!("stems: {}", lib.stem_dict.len());
    println!(
        "endings: {}",
        lib.end_index.by_ending.values().map(Vec::len).sum::<usize>()
    );

    let words = ["λόγος", "ἀνθρώπων", "ἐποιήσατο", "καταλαμβάνουσι"];
    let opts = morpheus_core::analysis::AnalysisOptions::default();
    let t1 = Instant::now();
    let mut n = 0;
    for _ in 0..250 {
        for w in words {
            n += morpheus_core::analysis::check_string(w, &lib, &opts).len();
        }
    }
    println!("1000 analyses: {:?} ({} results)", t1.elapsed(), n);
}

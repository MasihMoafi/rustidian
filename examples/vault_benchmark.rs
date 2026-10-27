use rustidian::{editor::Markdown, vault::Vault};
use std::{path::Path, time::Instant};

fn main() {
    let root = std::env::args()
        .nth(1)
        .expect("Pass a disposable benchmark vault");
    let mut samples = Vec::new();
    for _ in 0..5 {
        let started = Instant::now();
        let vault = Vault::open(Path::new(&root)).unwrap();
        let index_us = started.elapsed().as_micros();
        let mut searches = Vec::new();
        for query in ["connected", "note 04999", "你好", "absent_query_zzzz"] {
            let started = Instant::now();
            let hits = std::hint::black_box(vault.query(query).unwrap()).len();
            searches.push(serde_json::json!({"query": query, "hits": hits, "us": started.elapsed().as_micros()}));
        }
        let source = "# Heading\n\n**Bold** text with [[Home]] and `code`.\n\n".repeat(2000);
        let started = Instant::now();
        let spans = std::hint::black_box(Markdown::parse(&source)).spans.len();
        samples.push(serde_json::json!({"notes": vault.notes.len(), "index_us": index_us, "searches": searches, "parse_bytes": source.len(), "parse_us": started.elapsed().as_micros(), "spans": spans}));
    }
    println!("{}", serde_json::to_string_pretty(&samples).unwrap());
}

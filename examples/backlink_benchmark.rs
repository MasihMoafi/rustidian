use rust_notes::vault::Vault;
use std::{fs, path::Path, time::Instant};

fn main() {
    let temporary = tempfile::tempdir().unwrap();
    let count = 2000;
    let mut bytes = 0;
    for i in 0..count {
        let mut text = format!("# Note{i:05}\n\n");
        for j in 0..8 {
            text.push_str(&format!("[[Note{:05}]] ", (i * 17 + j * 251) % count));
        }
        text.push('\n');
        bytes += text.len();
        fs::write(temporary.path().join(format!("Note{i:05}.md")), text).unwrap();
    }
    let mut samples = Vec::new();
    for _ in 0..3 {
        let started = Instant::now();
        let vault = Vault::open(temporary.path()).unwrap();
        let index_us = started.elapsed().as_micros();
        let mut queries = Vec::new();
        for repetition in 0..2 {
            for target in [0, 997, 1999] {
                let name = format!("Note{target:05}.md");
                let started = Instant::now();
                let paths: Vec<_> = vault
                    .backlinks(Path::new(&name))
                    .into_iter()
                    .map(|note| note.path.display().to_string())
                    .collect();
                let elapsed_us = started.elapsed().as_micros();
                std::hint::black_box(&paths);
                queries.push(serde_json::json!({"target": name, "repetition": repetition, "elapsed_us": elapsed_us, "paths": paths}));
            }
        }
        samples.push(serde_json::json!({"index_us": index_us, "queries": queries}));
    }
    println!("{}", serde_json::to_string_pretty(&serde_json::json!({
        "corpus": {"notes": count, "links_per_note": 8, "utf8_bytes": bytes, "recipe": "target = (i * 17 + j * 251) % 2000; examples/backlink_benchmark.rs"},
        "samples": samples
    })).unwrap());
}

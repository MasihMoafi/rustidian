use std::{
    env,
    fs::{self, File},
    io::{self, BufWriter, Write},
    path::PathBuf,
};

fn main() -> io::Result<()> {
    const FONT_BYTES: u64 = 11_020_136;
    const PART_BYTES: u64 = 512 * 1024;
    let root =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("Cargo sets the package root"));
    let directory = root.join("assets/fonts/noto-color-emoji-parts");
    println!("cargo:rerun-if-changed={}", directory.display());
    let mut parts = fs::read_dir(directory)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<io::Result<Vec<_>>>()?;
    parts.sort();
    if parts.len() as u64 != FONT_BYTES.div_ceil(PART_BYTES) {
        return Err(io::Error::other("Incomplete bundled emoji font parts"));
    }
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo sets the build output directory"));
    let mut output = BufWriter::new(File::create(out.join("NotoColorEmoji.ttf"))?);
    for (index, part) in parts.iter().enumerate() {
        let expected_name = format!("{index:03}.part");
        if part.file_name().and_then(|name| name.to_str()) != Some(expected_name.as_str()) {
            return Err(io::Error::other("Unexpected bundled emoji font part name"));
        }
        println!("cargo:rerun-if-changed={}", part.display());
        let copied = io::copy(&mut File::open(part)?, &mut output)?;
        let expected_bytes = PART_BYTES.min(FONT_BYTES - index as u64 * PART_BYTES);
        if copied != expected_bytes {
            return Err(io::Error::other("Incorrect bundled emoji font part size"));
        }
    }
    output.flush()
}

//! Print a BYML file (optionally zstd-compressed) as JSON.
//!
//! cargo run -p mkwtrack --example bymldump -- <file> [--typed] [top-level key...]
//!
//! Plain JSON by default; --typed keeps the BYML node types ($f32, $u64, ...) so the output
//! round-trips through typedjson::from_json.

use mkwtrack::{byml, byml::Byml, io, typedjson};

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let typed = args.iter().any(|a| a == "--typed");
    args.retain(|a| a != "--typed");
    let Some(path) = args.first().cloned() else {
        eprintln!("usage: bymldump <file> [--typed] [top-level key...]");
        std::process::exit(2);
    };
    let data = io::read_file(std::path::Path::new(&path)).unwrap_or_else(|e| {
        eprintln!("{path}: {e}");
        std::process::exit(1);
    });
    let (root, _version) = byml::from_bytes(&data).unwrap_or_else(|e| {
        eprintln!("{path}: {e:?}");
        std::process::exit(1);
    });
    let show = |b: &Byml| if typed { typedjson::to_json(b) } else { typedjson::to_plain(b) };
    let out = if args.len() > 1 {
        let mut m = serde_json::Map::new();
        for k in &args[1..] {
            m.insert(k.clone(), root.get(k).map(show).unwrap_or(serde_json::Value::Null));
        }
        serde_json::Value::Object(m)
    } else {
        show(&root)
    };
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
}

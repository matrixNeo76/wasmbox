//! Helper degli scenari: compila ogni `*.wat` della cartella passata come
//! argomento in un `.wasm` affianco (stessa base name). Uso:
//!
//! ```sh
//! cargo run -q -p wat-compile-scenarios -- target/scenario-hostile
//! ```
//!
//! Il crate `wat` è già dev-dependency autorizzata dal blueprint per i test;
//! qui serve all'infrastruttura degli scenari (esempi, non runtime di
//! produzione) per non introdurre tool esterni (wabt/wat2wasm).

fn main() {
    let args: Vec<String> = std::env::args().collect();
    // arg 1 = cartella con i .wat sorgente (es. examples/scenario-hostile-guest)
    // arg 2 = cartella di output dei .wasm (default: stessa del sorgente)
    let src_dir = args
        .get(1)
        .unwrap_or_else(|| panic!("uso: wat-compile-scenarios <cartella-wat> [dest]"));
    let dest = args
        .get(2)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| src_dir.into());
    let entries = std::fs::read_dir(src_dir)
        .unwrap_or_else(|e| panic!("cartella {src_dir} illeggibile: {e}"));

    let mut compiled = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("wat") {
            continue;
        }
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("lettura {}: {e}", path.display()));
        let bytes = wat::parse_str(&source)
            .unwrap_or_else(|e| panic!("parse {} fallito: {e}", path.display()));
        let out = dest.join(
            path.file_stem()
                .unwrap_or_else(|| panic!("nome file illlegibile per {}", path.display()))
                .to_string_lossy()
                .to_string()
                + ".wasm",
        );
        std::fs::write(&out, &bytes).unwrap_or_else(|e| panic!("scrittura {}: {e}", out.display()));
        println!(
            "{} → {} ({} byte)",
            path.display(),
            out.display(),
            bytes.len()
        );
        compiled += 1;
    }
    if compiled == 0 {
        panic!("nessun .wat trovato in {src_dir}");
    }
}

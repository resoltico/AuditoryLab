//! Embed catalog files in natural order.
use std::{env, fs, path::PathBuf};
fn key(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut run = String::new();
    let mut digit = false;
    for c in s.chars() {
        if c.is_ascii_digit() != digit && !run.is_empty() {
            out.push(if digit {
                format!("{:0>20}", run)
            } else {
                run.clone()
            });
            run.clear();
        }
        digit = c.is_ascii_digit();
        run.push(c);
    }
    if !run.is_empty() {
        out.push(if digit { format!("{:0>20}", run) } else { run });
    }
    out
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=catalog");
    let mut ids = Vec::new();
    for entry in fs::read_dir("catalog")? {
        let path = entry?.path();
        if path.extension().is_some_and(|x| x == "toml") {
            let id = path
                .file_stem()
                .ok_or("missing stem")?
                .to_str()
                .ok_or("non-UTF8 stem")?
                .to_owned();
            if id.is_empty()
                || !id
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
            {
                return Err(format!("invalid catalog ID {id}").into());
            }
            ids.push(id);
        }
    }
    ids.sort_by_key(|s| key(s));
    let mut text = String::from(
        "/// Built-in recipes in natural ID order.\npub const ENTRIES: &[Entry] = &[\n",
    );
    for id in ids {
        let path = fs::canonicalize(format!("catalog/{id}.toml"))?;
        text.push_str(&format!(
            "Entry {{ id: {id:?}, text: include_str!({:?}) }},\n",
            path
        ));
    }
    text.push_str("];\n");
    fs::write(PathBuf::from(env::var("OUT_DIR")?).join("catalog.rs"), text)?;
    Ok(())
}

//! Embed catalog files in natural order.
use std::{env, fs, path::PathBuf};
fn number(run: &str) -> String {
    let digits = run.trim_start_matches('0');
    format!("{:020}{digits}", digits.len())
}
fn key(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut run = String::new();
    let mut digit = false;
    for c in s.chars() {
        if c.is_ascii_digit() != digit && !run.is_empty() {
            out.push(if digit { number(&run) } else { run.clone() });
            run.clear();
        }
        digit = c.is_ascii_digit();
        run.push(c);
    }
    if !run.is_empty() {
        out.push(if digit { number(&run) } else { run });
    }
    out
}
/// Resolve one condition into a standalone current-schema recipe.
pub(crate) fn generated_recipe(
    condition: &str,
    read: impl FnOnce(&str) -> Result<String, Box<dyn std::error::Error>>,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut document: toml::Table = toml::from_str(condition)?;
    let research = document
        .get_mut("about")
        .and_then(toml::Value::as_table_mut)
        .and_then(|a| a.get_mut("research"))
        .and_then(toml::Value::as_table_mut)
        .ok_or("missing condition research")?;
    let source = research.remove("source").ok_or("missing study source")?;
    let source = source.as_str().ok_or("study source must be text")?;
    if source.is_empty()
        || !source
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
    {
        return Err("invalid study source ID".into());
    }
    let study: toml::Table = toml::from_str(&read(source)?)?;
    if study.len() != 2 || !study.contains_key("citation") || !study.contains_key("source_url") {
        return Err("study must contain only citation and source_url".into());
    }
    for (name, value) in study {
        if research.insert(name, value).is_some() {
            return Err("condition duplicates study citation".into());
        }
    }
    Ok(toml::to_string(&document)?)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=catalog");
    println!("cargo:rerun-if-changed=studies");
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
    ids.sort_by(|a, b| key(a).cmp(&key(b)).then_with(|| a.cmp(b)));
    let mut text = String::from(
        "/// Built-in recipes in natural ID order.\npub const ENTRIES: &[Entry] = &[\n",
    );
    for id in ids {
        let recipe = generated_recipe(
            &fs::read_to_string(format!("catalog/{id}.toml"))?,
            |source| Ok(fs::read_to_string(format!("studies/{source}.toml"))?),
        )?;
        text.push_str(&format!("Entry {{ id: {id:?}, text: {recipe:?} }},\n"));
    }
    text.push_str("];\n");
    fs::write(PathBuf::from(env::var("OUT_DIR")?).join("catalog.rs"), text)?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::key;
    #[test]
    fn numeric_runs_compare_by_value_beyond_machine_integer_width() {
        assert!(key("a-8-hz") < key("a-40-hz"));
        assert!(key("a-99999999999999999999-hz") < key("a-100000000000000000000-hz"));
        assert_eq!(key("a-0008-hz"), key("a-8-hz"));
    }
}

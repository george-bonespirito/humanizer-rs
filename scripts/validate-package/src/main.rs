// Port of humanizer's scripts/validate-package.py (release lint, no external deps).
// Usage: validate-package [ROOT]  (ROOT defaults to ".")

use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::exit;

fn die(msg: &str) -> ! {
    eprintln!("{msg}");
    exit(1);
}

fn read_package_file(root: &Path, rel: &str) -> String {
    let path = root.join(rel);
    match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => die(&format!("Cannot read {rel}: {e}")),
    }
}

fn is_version(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    parts.len() == 3 && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

fn extract_yaml_metadata(skill: &str) -> String {
    if !skill.starts_with("---\n") {
        die("SKILL.md must begin with YAML metadata");
    }
    let rest = &skill[4..];
    match rest.find("\n---\n") {
        Some(idx) => rest[..idx].to_string(),
        None => die("SKILL.md must begin with YAML metadata"),
    }
}

fn skill_version_from_yaml(yaml: &str) -> String {
    for line in yaml.lines() {
        if line == line.trim_start() {
            continue; // needs leading whitespace, like the Python `\s+version:` anchor
        }
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("version:") {
            let value = rest.trim();
            let value = value
                .strip_prefix('"').unwrap_or(value)
                .strip_suffix('"').unwrap_or(value.strip_prefix('"').unwrap_or(value));
            let value = value.trim_matches(|c| c == '"' || c == '\'').trim();
            if is_version(value) {
                return value.to_string();
            }
        }
    }
    die("Add metadata.version to SKILL.md as a three-part version");
}

fn readme_version_from(readme: &str) -> String {
    for line in readme.lines() {
        if let Some(rest) = line.strip_prefix("- **") {
            if let Some(end) = rest.find("**") {
                let candidate = &rest[..end];
                if is_version(candidate) {
                    return candidate.to_string();
                }
            }
        }
    }
    die("Add a version entry to README.md");
}

// Minimal targeted JSON field readers -- plugin.json's shape is fixed and small,
// so a full parser would be dead weight.
fn json_string_field(json: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let key_pos = json.find(&needle)?;
    let after_key = &json[key_pos + needle.len()..];
    let colon = after_key.find(':')?;
    let after_colon = after_key[colon + 1..].trim_start();
    let quote = after_colon.strip_prefix('"')?;
    let end = quote.find('"')?;
    Some(quote[..end].to_string())
}

fn json_string_array_field(json: &str, key: &str) -> Option<Vec<String>> {
    let needle = format!("\"{key}\"");
    let key_pos = json.find(&needle)?;
    let after_key = &json[key_pos + needle.len()..];
    let colon = after_key.find(':')?;
    let after_colon = after_key[colon + 1..].trim_start();
    let open = after_colon.strip_prefix('[')?;
    let close = open.find(']')?;
    let body = &open[..close];
    Some(
        body.split(',')
            .map(|s| s.trim().trim_matches('"').to_string())
            .filter(|s| !s.is_empty())
            .collect(),
    )
}

fn rglob_skill_md(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    fn walk(dir: &Path, root: &Path, found: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, root, found);
            } else if path.file_name().and_then(|n| n.to_str()) == Some("SKILL.md") {
                if let Ok(rel) = path.strip_prefix(root) {
                    found.push(rel.to_path_buf());
                }
            }
        }
    }
    walk(root, root, &mut found);
    found
}

fn main() {
    let root = env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));

    let skill_path = root.join("SKILL.md");
    let skill = read_package_file(&root, "SKILL.md");
    let readme = read_package_file(&root, "README.md");
    let plugin_json = read_package_file(&root, ".claude-plugin/plugin.json");

    let yaml_metadata = extract_yaml_metadata(&skill);

    for unsupported in ["version:", "compatibility:", "allowed-tools:"] {
        if yaml_metadata.lines().any(|l| l.starts_with(unsupported)) {
            let field = &unsupported[..unsupported.len() - 1];
            die(&format!("Remove unsupported YAML field: {field}"));
        }
    }

    let skill_version = skill_version_from_yaml(&yaml_metadata);
    let readme_version = readme_version_from(&readme);
    let plugin_version = json_string_field(&plugin_json, "version").unwrap_or_default();

    let package_versions: HashSet<&str> = vec![
        skill_version.as_str(),
        readme_version.as_str(),
        plugin_version.as_str(),
    ]
    .into_iter()
    .collect();
    if package_versions.len() != 1 {
        let mut sorted: Vec<&str> = package_versions.into_iter().collect();
        sorted.sort();
        die(&format!("Use one package version in all files: {sorted:?}"));
    }

    let skill_files = rglob_skill_md(&root);
    let is_symlink = fs::symlink_metadata(&skill_path)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false);
    let only_root_skill = skill_files.len() == 1 && skill_files[0] == Path::new("SKILL.md");
    if is_symlink || !only_root_skill {
        die("Keep one regular SKILL.md at the repo root");
    }

    let plugin_skills = json_string_array_field(&plugin_json, "skills").unwrap_or_default();
    if plugin_skills != vec!["./".to_string()] {
        die("Point the Claude plugin skill loader at the repo root");
    }

    let mut pattern_numbers: Vec<i64> = Vec::new();
    for line in skill.lines() {
        if let Some(rest) = line.strip_prefix("### ") {
            let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            if !digits.is_empty() && rest[digits.len()..].starts_with(". ") {
                pattern_numbers.push(digits.parse().unwrap());
            }
        }
    }
    let pattern_count = pattern_numbers.len() as i64;
    let expected: Vec<i64> = (1..=pattern_count).collect();
    if pattern_count == 0 || pattern_numbers != expected {
        die(&format!(
            "Number SKILL.md patterns from 1 upward without gaps: {pattern_numbers:?}"
        ));
    }

    let mut readme_numbers: Vec<i64> = Vec::new();
    for line in readme.lines() {
        if let Some(rest) = line.strip_prefix("| ") {
            let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            if !digits.is_empty() && rest[digits.len()..].starts_with(" |") {
                readme_numbers.push(digits.parse().unwrap());
            }
        }
    }
    let mut sorted_readme_numbers = readme_numbers.clone();
    sorted_readme_numbers.sort();
    if sorted_readme_numbers != pattern_numbers {
        let mut want = sorted_readme_numbers.clone();
        want.sort();
        die(&format!(
            "List patterns 1 through {pattern_count} once each in the README tables: {want:?}"
        ));
    }

    let title = format!("## The {pattern_count} patterns");
    if !readme.contains(&title) {
        die(&format!("Title the README pattern section '{title}'"));
    }

    if skill.lines().count() > 400 {
        die("Keep SKILL.md at 400 lines or fewer");
    }

    println!("Humanizer package v{skill_version} is valid");
}

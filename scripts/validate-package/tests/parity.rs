// Regression tests for validate-package, run against fixture trees built from
// this fork's real SKILL.md / README.md / plugin.json (not a copy of the
// original Python script -- see the map's "Mecanismo de sincronização"
// ticket: upstream's validate-package.py is diffed at sync time, never
// vendored, so the expected messages below are hardcoded instead).

use std::fs;
use std::path::Path;
use std::process::Command;

fn repo_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

fn write_fixture(dir: &Path, skill: &str, readme: &str, plugin_json: &str) {
    fs::create_dir_all(dir.join(".claude-plugin")).unwrap();
    fs::write(dir.join("SKILL.md"), skill).unwrap();
    fs::write(dir.join("README.md"), readme).unwrap();
    fs::write(dir.join(".claude-plugin/plugin.json"), plugin_json).unwrap();
}

fn base_fixture() -> (String, String, String) {
    let root = repo_root();
    (
        fs::read_to_string(root.join("SKILL.md")).expect("fixture: read real SKILL.md"),
        fs::read_to_string(root.join("README.md")).expect("fixture: read real README.md"),
        fs::read_to_string(root.join(".claude-plugin/plugin.json"))
            .expect("fixture: read real plugin.json"),
    )
}

fn run(dir: &Path) -> (bool, String, String) {
    let bin = env!("CARGO_BIN_EXE_validate-package");
    let output = Command::new(bin).arg(dir).output().expect("run validate-package");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

#[test]
fn valid_package_passes() {
    let (skill, readme, plugin_json) = base_fixture();
    let dir = tempdir();
    write_fixture(dir.path(), &skill, &readme, &plugin_json);

    let (ok, stdout, _stderr) = run(dir.path());
    assert!(ok, "expected success on the fork's real, unmodified files");
    assert!(stdout.contains("is valid"), "stdout: {stdout}");
}

#[test]
fn version_mismatch_fails() {
    let (skill, readme, plugin_json) = base_fixture();
    let dir = tempdir();
    let mutated_skill = skill.replacen("3.0.0", "9.9.9", 1);
    assert_ne!(mutated_skill, skill, "fixture setup: version 3.0.0 not found in SKILL.md");
    write_fixture(dir.path(), &mutated_skill, &readme, &plugin_json);

    let (ok, _stdout, stderr) = run(dir.path());
    assert!(!ok);
    assert!(
        stderr.contains("Use one package version in all files"),
        "stderr: {stderr}"
    );
}

#[test]
fn skill_too_long_fails() {
    let (skill, readme, plugin_json) = base_fixture();
    let dir = tempdir();
    let mut padded = skill;
    padded.push_str(&"\nx".repeat(401));
    write_fixture(dir.path(), &padded, &readme, &plugin_json);

    let (ok, _stdout, stderr) = run(dir.path());
    assert!(!ok);
    assert!(
        stderr.contains("Keep SKILL.md at 400 lines or fewer"),
        "stderr: {stderr}"
    );
}

#[test]
fn missing_yaml_frontmatter_fails() {
    let (_skill, readme, plugin_json) = base_fixture();
    let dir = tempdir();
    write_fixture(dir.path(), "# No frontmatter here\n", &readme, &plugin_json);

    let (ok, _stdout, stderr) = run(dir.path());
    assert!(!ok);
    assert!(
        stderr.contains("SKILL.md must begin with YAML metadata"),
        "stderr: {stderr}"
    );
}

#[test]
fn missing_readme_pattern_title_fails() {
    let (skill, readme, plugin_json) = base_fixture();
    let dir = tempdir();
    let mutated_readme = readme.replacen("## The ", "## Something else ", 1);
    assert_ne!(
        mutated_readme, readme,
        "fixture setup: '## The ' title not found in README.md"
    );
    write_fixture(dir.path(), &skill, &mutated_readme, &plugin_json);

    let (ok, _stdout, stderr) = run(dir.path());
    assert!(!ok);
    assert!(
        stderr.contains("Title the README pattern section"),
        "stderr: {stderr}"
    );
}

/// Minimal tempdir helper (no external crate): a unique subdirectory under
/// the target dir, removed on drop.
struct TempDir(std::path::PathBuf);

impl TempDir {
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn tempdir() -> TempDir {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("validate-package-test-{pid}-{n}"));
    fs::create_dir_all(&dir).unwrap();
    TempDir(dir)
}

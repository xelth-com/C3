//! (wave 27, R13 D5 / 27c D13, D15) `c3 consult --explain coordinate|consult|providers` - the
//! plugin's `codex-consult.ps1 -Explain`: a host without skills reads the plugin's skills through
//! the bridge itself. Prints one line naming the file and the plugin directory, a blank line, then
//! the skill's `SKILL.md` without its front matter, every `${CLAUDE_PLUGIN_ROOT}` in it replaced by
//! that plugin directory (runnable as written on a host that substitutes nothing). UTF-8 bytes on
//! stdout; nothing else is read or written; exit 0. A refusal is exit 1 with the plugin's texts.
//!
//! The plugin directory is [`crate::panel::roles::plugin_root`]: `CLAUDE_PLUGIN_ROOT`, else the
//! parent of `CODEX_CONSULT_SCRIPTS_DIR`, else C3's `plugin/` beside the binary. The skill
//! directories are the plugin's (`coordinate`, `consult-codex`, `setup-providers`); C3's own plugin
//! names the consult skill `consult`, which is read when `consult-codex` is not there.

use std::io::Write;
use std::path::{Path, PathBuf};

const TOOL: &str = "codex-consult";

/// The `-Explain` names and the skill directories each may read (first that exists).
fn skill_dirs(key: &str) -> Option<&'static [&'static str]> {
    match key {
        "coordinate" => Some(&["coordinate"]),
        "consult" => Some(&["consult-codex", "consult"]),
        "providers" => Some(&["setup-providers"]),
        _ => None,
    }
}

/// `--some-flag` -> `-SomeFlag` (how the plugin names the parameter in its refusal).
fn pascal_flag(flag: &str) -> String {
    let body = flag.trim_start_matches('-');
    let body = body.split('=').next().unwrap_or(body);
    let mut out = String::from("-");
    for part in body.split('-') {
        let mut cs = part.chars();
        if let Some(c) = cs.next() {
            out.extend(c.to_uppercase());
            out.push_str(cs.as_str());
        }
    }
    out
}

/// The flags given on the command line besides `--explain` (for the "takes no other parameter"
/// refusal), from the raw `c3 consult ...` arguments.
pub fn other_flags(raw_args: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for a in raw_args {
        if !a.starts_with("--") || a == "--" {
            continue;
        }
        let name = a.split('=').next().unwrap_or(a);
        if name == "--explain" {
            continue;
        }
        let shown = pascal_flag(name);
        if !out.contains(&shown) {
            out.push(shown);
        }
    }
    out
}

/// Strip a leading `---` front matter block (`\A---\r?\n[\s\S]*?\r?\n---\r?\n`).
fn strip_front_matter(body: &str) -> &str {
    let rest = if let Some(r) = body.strip_prefix("---\r\n") {
        r
    } else if let Some(r) = body.strip_prefix("---\n") {
        r
    } else {
        return body;
    };
    let offset = body.len() - rest.len();
    let mut pos = 0usize;
    while let Some(i) = rest[pos..].find('\n') {
        let line_end = pos + i;
        let after = line_end + 1;
        for close in ["---\r\n", "---\n"] {
            if rest[after..].starts_with(close) {
                return &body[offset + after + close.len()..];
            }
        }
        pos = after;
    }
    body
}

/// The text `--explain <name>` prints, or the refusal message. `plugin_root` is the plugin
/// directory (`""` when unknown).
pub fn render(name: &str, plugin_root: &str) -> Result<String, String> {
    let key = name.trim().to_lowercase();
    let dirs = skill_dirs(&key).ok_or_else(|| {
        format!("-Explain takes coordinate, consult or providers (got '{name}').")
    })?;
    let root = PathBuf::from(plugin_root);
    let skills = root.join("skills");
    let mut chosen: Option<(String, PathBuf)> = None;
    for d in dirs {
        let p = skills.join(d).join("SKILL.md");
        if !plugin_root.is_empty() && p.is_file() {
            chosen = Some((d.to_string(), p));
            break;
        }
    }
    let (skill, path) = match chosen {
        Some(c) => c,
        None => {
            let shown = if plugin_root.is_empty() {
                Path::new("<plugin root not found>")
                    .join("skills")
                    .join(dirs[0])
                    .join("SKILL.md")
            } else {
                skills.join(dirs[0]).join("SKILL.md")
            };
            return Err(format!(
                "-Explain {key}: '{}' does not exist (an incomplete plugin directory).",
                shown.display()
            ));
        }
    };
    let raw = std::fs::read(&path).map_err(|e| {
        format!(
            "-Explain {key}: '{}' could not be read ({}).",
            path.display(),
            c3_core::one_line(&e.to_string())
        )
    })?;
    let text = String::from_utf8_lossy(&raw);
    let text = text.trim_start_matches('\u{feff}');
    let body = strip_front_matter(text);
    let root_shown = root.display().to_string();
    let head = format!(
        "{TOOL} -Explain {key}: the {skill} skill, {} - ${{CLAUDE_PLUGIN_ROOT}} in it is replaced by the plugin directory {root_shown}",
        path.display()
    );
    let body = body.replace("${CLAUDE_PLUGIN_ROOT}", &root_shown);
    Ok(format!(
        "{head}\n\n{}\n",
        body.trim_start_matches(['\r', '\n']).trim_end()
    ))
}

/// `c3 consult --explain <name>`: print the skill (exit 0) or refuse (exit 1). `others` are the
/// other flags given (refused: `-Explain takes no other parameter`).
pub fn run(name: &str, others: &[String]) -> i32 {
    if !others.is_empty() {
        eprintln!(
            "{TOOL}: -Explain takes no other parameter (got {}).",
            others.join(", ")
        );
        return 1;
    }
    match render(name, &crate::panel::roles::plugin_root()) {
        Ok(text) => {
            // (wave 27c, D15) the bytes as they are (UTF-8), flushed before the exit.
            let mut out = std::io::stdout().lock();
            let _ = out.write_all(text.as_bytes());
            let _ = out.flush();
            0
        }
        Err(msg) => {
            eprintln!("{TOOL}: {msg}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "c3-explain-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn explain_prints_the_skill_with_the_root_filled_in() {
        let root = scratch("ok");
        let dir = root.join("skills").join("coordinate");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("SKILL.md"),
            "---\r\nname: coordinate\r\ndescription: x\r\n---\r\n\r\n# Rules\r\nrun ${CLAUDE_PLUGIN_ROOT}/scripts/x.ps1\r\n",
        )
        .unwrap();
        let r = root.to_string_lossy().to_string();
        let t = render(" Coordinate ", &r).unwrap();
        let first = t.lines().next().unwrap();
        assert!(first.starts_with("codex-consult -Explain coordinate: the coordinate skill, "));
        assert!(first.ends_with(&format!(
            " - ${{CLAUDE_PLUGIN_ROOT}} in it is replaced by the plugin directory {r}"
        )));
        let rest = &t[t.find('\n').unwrap()..];
        assert!(!rest.contains("${CLAUDE_PLUGIN_ROOT}"), "{t}");
        assert!(rest.contains(&format!("run {r}/scripts/x.ps1")), "{t}");
        assert!(
            !rest.contains("name: coordinate"),
            "front matter stripped: {t}"
        );
        assert!(t.ends_with('\n'));
        // consult reads consult-codex, else C3's own `consult`
        let cdir = root.join("skills").join("consult");
        std::fs::create_dir_all(&cdir).unwrap();
        std::fs::write(cdir.join("SKILL.md"), "body\n").unwrap();
        assert!(render("consult", &r)
            .unwrap()
            .starts_with("codex-consult -Explain consult: the consult skill, "));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn explain_refusals() {
        assert_eq!(
            render("nope", "x").unwrap_err(),
            "-Explain takes coordinate, consult or providers (got 'nope')."
        );
        let root = scratch("missing");
        let e = render("providers", &root.to_string_lossy()).unwrap_err();
        assert!(e.starts_with("-Explain providers: '"), "{e}");
        assert!(
            e.ends_with("' does not exist (an incomplete plugin directory)."),
            "{e}"
        );
        assert_eq!(
            other_flags(&[
                "--explain".into(),
                "coordinate".into(),
                "--task".into(),
                "t".into(),
                "--collab-dir=x".into()
            ]),
            vec!["-Task".to_string(), "-CollabDir".to_string()]
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}

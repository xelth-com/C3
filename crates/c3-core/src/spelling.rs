//! How C3's own user-facing lines name the commands (wave 9, D2): the `c3` spelling by default
//! (`c3 consult --task <t> --status --id <id>`, `c3 providers`); the plugin's PowerShell spelling
//! (`codex-consult.ps1 -Task <t> -Status -Id <id>`; and a panel forking by default, D1) only when the `tests/shim` forwarders ask for it
//! with `C3_PLUGIN_COMPAT=1`, so the plugin's harness oracle texts keep matching. Ledger and
//! handoff FILE formats never go through here.

/// The plugin spelling is asked for (the shims set `C3_PLUGIN_COMPAT=1`).
pub fn plugin() -> bool {
    std::env::var("C3_PLUGIN_COMPAT")
        .map(|v| v == "1")
        .unwrap_or(false)
}

/// `run c3 providers` / `codex-providers.ps1` - the command that prints the full picture.
pub fn providers_cmd() -> &'static str {
    if plugin() {
        "codex-providers.ps1"
    } else {
        "c3 providers"
    }
}

/// The status command of a detached run (`id8`), with the optional collab dir.
pub fn status_cmd(task: &str, collab_abs: Option<&str>, id8: &str) -> String {
    if plugin() {
        let c = collab_abs
            .map(|c| format!(" -CollabDir \"{c}\""))
            .unwrap_or_default();
        format!("codex-consult.ps1 -Task {task}{c} -Status -Id {id8}")
    } else {
        let c = collab_abs
            .map(|c| format!(" --collab-dir \"{c}\""))
            .unwrap_or_default();
        format!("c3 consult --task {task}{c} --status --id {id8}")
    }
}

/// The wait flags of a detached run: `-Wait -Id <id8>` / `--wait --id <id8>`.
pub fn wait_flags(id8: &str) -> String {
    if plugin() {
        format!("-Wait -Id {id8}")
    } else {
        format!("--wait --id {id8}")
    }
}

/// The findings command: `codex-findings.ps1 -Task <t> -List -All` / `c3 findings --task <t> --list --all`.
pub fn findings_list_all_cmd(task: &str) -> String {
    if plugin() {
        format!("codex-findings.ps1 -Task {task} -List -All")
    } else {
        format!("c3 findings --task {task} --list --all")
    }
}

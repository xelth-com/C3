//! The engine contract (DESIGN §4): one attempt of one reviewer, and the exact argv the
//! plugin builds for the subprocess engines.
//!
//! An [`Engine`] reports its [`Capabilities`], plans the argv for a [`Request`]
//! ([`Engine::plan`]), and - in M2b - runs a turn and continues a conversation
//! ([`Engine::run`], [`Engine::continue_turn`], left as signatures here). The three
//! subprocess engines wrap the CLIs exactly as `codex-consult.ps1` / `New-AgyArgv` /
//! `New-MuseArgv` do: the prompt on stdin (codex, agy) or a prompt file (muse), every
//! exec-level option BEFORE the `fork`/`resume` subcommand (codex), and `-` for stdin
//! (codex). The `http` engine builds one OpenAI-compatible request from a reviewer pack
//! instead of an argv (M7); its `plan` therefore returns [`EngineError::NoArgv`].
//!
//! Ids (DESIGN §4 "Identity"): [`ConsultationId`] is one brief + one reviewer;
//! [`AttemptId`] is one engine call (a retry is a NEW attempt with the same inputs, no
//! redraw); [`ConversationId`] is the engine thread for CLI engines, or a C3-owned
//! transcript for `http` (whose continuation is replay: the retained pack plus the prior
//! reply). `resume_supported` is reported per engine ([`Capabilities::resume`]).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::ledger::Usage;

/// The four engines (DESIGN §4). `codex` is the default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EngineKind {
    Codex,
    Agy,
    Muse,
    Http,
}

/// How a reviewer receives the reply schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SchemaTransport {
    /// Codex `--output-schema` / muse `--output-schema`.
    OutputSchema,
    /// agy `--json-schema` (a native structured-output request).
    Native,
    /// The schema is described in the prompt only.
    PromptOnly,
}

/// What an engine can do (DESIGN §4: `capabilities()`).
#[derive(Debug, Clone)]
pub struct Capabilities {
    /// The engine keeps conversation threads (a reply can be resumed/forked).
    pub threads: bool,
    /// Forking a thread is supported (codex only).
    pub fork: bool,
    /// Resuming a thread is supported natively.
    pub resume: bool,
    /// The schema transports the engine accepts, in preference order.
    pub schema_transport: Vec<SchemaTransport>,
    /// The effort vocabulary label (`openai`, `model-tier`, `muse`, ...).
    pub effort_vocabulary: &'static str,
    /// The engine runs its reviewer in a read-only sandbox.
    pub sandbox: bool,
    /// The handoff/file prefix for this engine (`codex`, `agy`, `muse`, `http`).
    pub file_prefix: &'static str,
}

/// The capability table for an engine, from `$script:Engines` (and DESIGN §4 for `http`).
pub fn capabilities(kind: EngineKind) -> Capabilities {
    match kind {
        EngineKind::Codex => Capabilities {
            threads: true,
            fork: true,
            resume: true,
            schema_transport: vec![SchemaTransport::OutputSchema, SchemaTransport::PromptOnly],
            effort_vocabulary: "openai",
            sandbox: true,
            file_prefix: "codex",
        },
        EngineKind::Agy => Capabilities {
            threads: true,
            fork: false,
            resume: true,
            schema_transport: vec![SchemaTransport::Native, SchemaTransport::PromptOnly],
            effort_vocabulary: "model-tier",
            // agy --sandbox restricts the terminal only; the bridge's tree check enforces
            // read-only. The capability is "runs read-only" as the bridge guarantees it.
            sandbox: true,
            file_prefix: "agy",
        },
        EngineKind::Muse => Capabilities {
            threads: true,
            fork: false,
            resume: true,
            schema_transport: vec![SchemaTransport::Native, SchemaTransport::PromptOnly],
            effort_vocabulary: "muse",
            sandbox: true,
            file_prefix: "muse",
        },
        EngineKind::Http => Capabilities {
            threads: false,
            fork: false,
            // Continuation is replay (the retained pack + the prior reply), not a native
            // thread resume; reported as unsupported.
            resume: false,
            schema_transport: vec![SchemaTransport::OutputSchema, SchemaTransport::PromptOnly],
            effort_vocabulary: "openai",
            // The http engine never receives tools (DESIGN §3 invariant 2).
            sandbox: false,
            file_prefix: "http",
        },
    }
}

/// The launch mode for a CLI engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    /// A fresh thread.
    New,
    /// Resume a known thread.
    Resume(String),
    /// Fork a known thread (codex only).
    Fork(String),
}

/// One consultation id (one brief, one reviewer). The prompt's last line carries it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsultationId(pub String);

/// One attempt id (one engine call). A retry is a new [`AttemptId`] with the same inputs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttemptId(pub String);

/// One conversation id (the engine thread for CLI engines; a C3 transcript for `http`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationId(pub String);

/// A request for one attempt. Some fields are engine-specific (documented per field).
#[derive(Debug, Clone)]
pub struct Request {
    /// The full prompt text (its last line is `Consultation id: <id>`).
    pub prompt: String,
    /// The brief file, for the record.
    pub brief_path: Option<PathBuf>,
    pub model: String,
    pub provider: String,
    /// The effort value to send (already mapped to the engine's vocabulary), if any.
    pub effort: Option<String>,
    /// The run timeout, in seconds.
    pub timeout_sec: f64,
    pub mode: Mode,
    /// The sandbox label (codex: `read-only`/`workspace-write`).
    pub sandbox: String,
    /// The reply schema file, when a schema transport is used.
    pub schema_path: Option<PathBuf>,
    /// Applied `-CodexConfig` items (`key=value`); codex `-c` args.
    pub extra_config: Vec<String>,
    /// codex `-o <path>`: where the last agent message is written.
    pub output_last_message: Option<PathBuf>,
    /// muse `--prompt-file <path>`: the prompt file.
    pub prompt_file: Option<PathBuf>,
    /// muse `--max-model-steps <n>`.
    pub max_model_steps: Option<u32>,
}

/// The argv the engine will run: the launcher plus its arguments (the launcher is resolved
/// on PATH separately). `args` is everything after the launcher.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Argv {
    pub command: String,
    pub args: Vec<String>,
}

impl Argv {
    /// The whole argv as one display string (`codex exec --sandbox read-only ... -`).
    pub fn to_command_string(&self) -> String {
        let mut s = self.command.clone();
        for a in &self.args {
            s.push(' ');
            s.push_str(a);
        }
        s
    }
}

/// One reply from an attempt.
#[derive(Debug, Clone)]
pub struct Reply {
    /// The verbatim reply text (the verified structured or the salvaged prose).
    pub raw_text: String,
    /// The parsed v1 reply, when the attempt produced a valid structured object.
    pub structured: Option<StructuredReply>,
    /// The raw event stream file (`.events.jsonl`).
    pub events_path: PathBuf,
    /// Token usage, when the engine reports it.
    pub usage: Option<Usage>,
    /// Wall time in seconds.
    pub wall_seconds: f64,
    /// The conversation this reply came on, when known.
    pub conversation: Option<ConversationId>,
}

// --------------------------------------------------------------------------- v1 reply schema

/// The v1 reply object, mirroring `schemas/consult-reply.schema.json` exactly. Unknown
/// fields are rejected (`additionalProperties: false`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StructuredReply {
    /// Always `"1"`.
    pub schema_version: String,
    /// `ACCEPT | HOLD | REJECT | ADVISE`.
    pub verdict: String,
    pub verdict_reason: String,
    pub reply_markdown: String,
    pub findings: Vec<ReplyFinding>,
    pub prior_findings: Vec<ReplyPriorFinding>,
    pub unproven: Vec<String>,
    pub first_run_checklist: Vec<String>,
}

/// One finding in a v1 reply (the reviewer's proposal, before it is assigned an id).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplyFinding {
    /// `blocker | major | minor | note`.
    pub severity: String,
    pub locations: Vec<ReplyLocation>,
    pub claim: String,
    pub trigger: String,
    pub evidence: Vec<ReplyEvidence>,
    pub verification: String,
    pub remedy: String,
    pub supersedes: Vec<String>,
}

/// A location in a v1 reply finding; `line` may be null.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplyLocation {
    pub path: String,
    pub line: Option<i64>,
}

/// A piece of evidence in a v1 reply finding.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplyEvidence {
    /// `read-code | ran-command | inferred | assumed`.
    pub kind: String,
    pub reference: String,
    pub observation: String,
}

/// A prior-finding report in a v1 reply.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplyPriorFinding {
    pub id: String,
    /// `fixed | still-open | not-checked | unknown-id`.
    pub status: String,
    pub note: String,
}

// --------------------------------------------------------------------------- the trait

/// What can go wrong planning or running an attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineError {
    /// This engine builds no argv (the `http` engine sends a request instead).
    NoArgv,
    /// A required field for this engine's argv was missing.
    MissingField(&'static str),
}

impl std::fmt::Display for EngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EngineError::NoArgv => write!(f, "this engine builds no argv (http sends a request)"),
            EngineError::MissingField(name) => write!(f, "missing field for argv: {name}"),
        }
    }
}

impl std::error::Error for EngineError {}

/// One attempt of one reviewer (DESIGN §4).
pub trait Engine {
    /// The engine's capabilities.
    fn capabilities(&self) -> Capabilities;

    /// The exact argv the plugin builds for `request` (subprocess engines only).
    fn plan(&self, request: &Request) -> Result<Argv, EngineError>;

    /// Run one turn. Implemented in M2b (subprocess launch, event reader, reply
    /// ingestion, format repair). Signature only.
    fn run(&self, request: &Request) -> Result<Reply, EngineError>;

    /// Continue a conversation with a follow-up prompt, where supported (codex/agy/muse
    /// resume; `http` replay). Implemented in M2b. Signature only.
    fn continue_turn(
        &self,
        conversation: &ConversationId,
        prompt: &str,
    ) -> Result<Reply, EngineError>;
}

/// A subprocess engine (codex, agy or muse). Its [`Engine::plan`] reproduces the plugin's
/// argv byte for byte; [`Engine::run`]/[`Engine::continue_turn`] land in M2b.
#[derive(Debug, Clone)]
pub struct SubprocessEngine {
    pub kind: EngineKind,
}

impl SubprocessEngine {
    pub fn new(kind: EngineKind) -> Self {
        SubprocessEngine { kind }
    }

    fn plan_codex(&self, r: &Request) -> Result<Argv, EngineError> {
        // codex exec --sandbox <s> --color never --json [-m <model>]
        //   -c model_reasoning_effort="<e>" [-c model_provider="<p>"] [-c <extra>...]
        //   -o <tmp> [--output-schema <schema>] [fork|resume <thread>] -
        // Every exec-level option precedes the fork|resume subcommand; `-` is stdin.
        let mut a: Vec<String> = vec![
            "exec".into(),
            "--sandbox".into(),
            r.sandbox.clone(),
            "--color".into(),
            "never".into(),
            "--json".into(),
        ];
        if !r.model.is_empty() {
            a.push("-m".into());
            a.push(r.model.clone());
        }
        if let Some(e) = &r.effort {
            a.push("-c".into());
            a.push(format!("model_reasoning_effort=\"{e}\""));
        }
        if !r.provider.is_empty() {
            a.push("-c".into());
            a.push(format!("model_provider=\"{}\"", r.provider));
        }
        for item in &r.extra_config {
            a.push("-c".into());
            a.push(item.clone());
        }
        if let Some(o) = &r.output_last_message {
            a.push("-o".into());
            a.push(o.to_string_lossy().into_owned());
        }
        if let Some(s) = &r.schema_path {
            a.push("--output-schema".into());
            a.push(s.to_string_lossy().into_owned());
        }
        match &r.mode {
            Mode::New => {}
            Mode::Resume(t) => {
                a.push("resume".into());
                a.push(t.clone());
            }
            Mode::Fork(_t) => {
                a.push("fork".into());
            }
        }
        a.push("-".into());
        Ok(Argv {
            command: "codex".into(),
            args: a,
        })
    }

    fn plan_agy(&self, r: &Request) -> Result<Argv, EngineError> {
        // -p= --input-format stream-json --output-format stream-json --model <m>
        //   [--json-schema <schema>] --print-timeout 0 --sandbox --disable-slash-commands
        //   [--conversation <thread>] [--effort <e>]
        if r.model.is_empty() {
            return Err(EngineError::MissingField("model"));
        }
        let mut a: Vec<String> = vec![
            "-p=".into(),
            "--input-format".into(),
            "stream-json".into(),
            "--output-format".into(),
            "stream-json".into(),
            "--model".into(),
            r.model.clone(),
        ];
        if let Some(s) = &r.schema_path {
            a.push("--json-schema".into());
            a.push(s.to_string_lossy().into_owned());
        }
        a.push("--print-timeout".into());
        a.push("0".into());
        a.push("--sandbox".into());
        a.push("--disable-slash-commands".into());
        if let Mode::Resume(t) = &r.mode {
            a.push("--conversation".into());
            a.push(t.clone());
        }
        if let Some(e) = &r.effort {
            a.push("--effort".into());
            a.push(e.clone());
        }
        Ok(Argv {
            command: "agy".into(),
            args: a,
        })
    }

    fn plan_muse(&self, r: &Request) -> Result<Argv, EngineError> {
        // exec --json --prompt-file <P> [--output-schema <S>] --model <m>
        //   [--reasoning-effort <e>] --no-foreign-personal-context --disable-web-tools
        //   --disable-write --disable-shell --approval-mode never [--max-model-steps <n>]
        //   [--session-id <thread>]
        let prompt_file = r
            .prompt_file
            .as_ref()
            .ok_or(EngineError::MissingField("prompt_file"))?;
        if r.model.is_empty() {
            return Err(EngineError::MissingField("model"));
        }
        let mut a: Vec<String> = vec![
            "exec".into(),
            "--json".into(),
            "--prompt-file".into(),
            prompt_file.to_string_lossy().into_owned(),
        ];
        if let Some(s) = &r.schema_path {
            a.push("--output-schema".into());
            a.push(s.to_string_lossy().into_owned());
        }
        a.push("--model".into());
        a.push(r.model.clone());
        if let Some(e) = &r.effort {
            if !e.trim().is_empty() {
                a.push("--reasoning-effort".into());
                a.push(e.clone());
            }
        }
        a.push("--no-foreign-personal-context".into());
        a.push("--disable-web-tools".into());
        a.push("--disable-write".into());
        a.push("--disable-shell".into());
        a.push("--approval-mode".into());
        a.push("never".into());
        if let Some(n) = r.max_model_steps {
            if n > 0 {
                a.push("--max-model-steps".into());
                a.push(n.to_string());
            }
        }
        if let Mode::Resume(t) = &r.mode {
            a.push("--session-id".into());
            a.push(t.clone());
        }
        Ok(Argv {
            command: "muse".into(),
            args: a,
        })
    }
}

impl Engine for SubprocessEngine {
    fn capabilities(&self) -> Capabilities {
        capabilities(self.kind)
    }

    fn plan(&self, request: &Request) -> Result<Argv, EngineError> {
        match self.kind {
            EngineKind::Codex => self.plan_codex(request),
            EngineKind::Agy => self.plan_agy(request),
            EngineKind::Muse => self.plan_muse(request),
            EngineKind::Http => Err(EngineError::NoArgv),
        }
    }

    fn run(&self, _request: &Request) -> Result<Reply, EngineError> {
        // M2b: launch the subprocess, read the event stream, ingest the reply, repair a
        // prose reply, kill and continue on timeout. No execution in the contracts task.
        unimplemented!("Engine::run lands in milestone 2b")
    }

    fn continue_turn(
        &self,
        _conversation: &ConversationId,
        _prompt: &str,
    ) -> Result<Reply, EngineError> {
        unimplemented!("Engine::continue_turn lands in milestone 2b")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_request() -> Request {
        Request {
            prompt: "x".into(),
            brief_path: None,
            model: "gpt-6-astra".into(),
            provider: "openai".into(),
            effort: Some("high".into()),
            timeout_sec: 1800.0,
            mode: Mode::New,
            sandbox: "read-only".into(),
            schema_path: Some(PathBuf::from("schema.json")),
            extra_config: vec![],
            output_last_message: Some(PathBuf::from("last.md")),
            prompt_file: None,
            max_model_steps: None,
        }
    }

    #[test]
    fn codex_argv_shape() {
        let e = SubprocessEngine::new(EngineKind::Codex);
        let argv = e.plan(&base_request()).unwrap();
        assert_eq!(
            argv.to_command_string(),
            "codex exec --sandbox read-only --color never --json -m gpt-6-astra \
             -c model_reasoning_effort=\"high\" -c model_provider=\"openai\" \
             -o last.md --output-schema schema.json -"
        );
    }

    #[test]
    fn codex_resume_options_precede_subcommand() {
        let e = SubprocessEngine::new(EngineKind::Codex);
        let mut r = base_request();
        r.mode = Mode::Resume("abc".into());
        let argv = e.plan(&r).unwrap();
        let pos_schema = argv
            .args
            .iter()
            .position(|a| a == "--output-schema")
            .unwrap();
        let pos_resume = argv.args.iter().position(|a| a == "resume").unwrap();
        assert!(pos_schema < pos_resume, "exec options precede resume");
        assert_eq!(argv.args.last().unwrap(), "-");
    }

    #[test]
    fn agy_and_muse_shapes() {
        let agy = SubprocessEngine::new(EngineKind::Agy)
            .plan(&base_request())
            .unwrap();
        assert_eq!(agy.command, "agy");
        assert_eq!(agy.args[0], "-p=");
        assert!(agy.args.contains(&"--json-schema".to_string()));
        assert!(agy.args.contains(&"--effort".to_string()));

        let mut r = base_request();
        r.prompt_file = Some(PathBuf::from("p.txt"));
        r.max_model_steps = Some(3);
        let muse = SubprocessEngine::new(EngineKind::Muse).plan(&r).unwrap();
        assert_eq!(muse.args[0], "exec");
        assert!(muse.args.contains(&"--disable-shell".to_string()));
        assert!(muse.args.contains(&"--max-model-steps".to_string()));
    }

    #[test]
    fn http_has_no_argv() {
        let e = SubprocessEngine::new(EngineKind::Http);
        assert_eq!(e.plan(&base_request()), Err(EngineError::NoArgv));
    }
}

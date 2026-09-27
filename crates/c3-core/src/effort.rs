//! Effort vocabularies and the caps-v1 capability table, ported from
//! `codex-consult-common.ps1` (`$script:EffortCaps`, `$script:EffortVocabularies`,
//! the declared-model lists and `Get-SchemaTransport`).

pub const CAPS_VERSION: &str = "caps-v1";

/// The declared models for an endpoint, or `Any` when the vocabulary applies to
/// every model on the host.
#[derive(Debug, Clone)]
pub enum Models {
    Any,
    List(&'static [&'static str]),
}

/// One caps-v1 row: vocabulary label, declared models, schema transport.
#[derive(Debug, Clone)]
pub struct Caps {
    pub vocabulary: &'static str,
    pub models: Models,
    pub schema_transport: &'static str,
}

const ZAI_MODELS: &[&str] = &[
    "glm-5.3",
    "glm-5.3-flash",
    "glm-5.3-flashx",
    "glm-5.2",
    "glm-5.1",
    "glm-5",
    "glm-5-turbo",
    "glm-4.7",
    "glm-4.6",
    "glm-4.5",
    "glm-4.5-air",
];
const MIMO_MODELS: &[&str] = &[
    "mimo-v2.6-pro",
    "mimo-v2.6-flash",
    "mimo-v2.6-pro-ultraspeed",
    "mimo-v2.5-pro",
    "mimo-v2.5",
];
const ARK_MODELS: &[&str] = &[
    "dola-seed-2.0-pro",
    "dola-seed-2.0-lite",
    "dola-seed-2.0-code",
    "bytedance-seed-code",
    "glm-5.3-flash",
    "glm-5.2",
    "glm-5.1",
    "kimi-k2.5",
    "gpt-oss-120b",
    "deepseek-v4.1-flash",
    "deepseek-v4-flash",
    "deepseek-v4-pro",
];
const KIMI_MODELS: &[&str] = &[
    "k3",
    "k3-256k",
    "kimi-for-coding",
    "kimi-for-coding-highspeed",
];
const ALIBABA_MODELS: &[&str] = &[
    "qwen3.8-max",
    "qwen3.8-flash",
    "qwen3.7-max",
    "qwen3.7-plus",
    "qwen3.6-flash",
    "deepseek-v4.1-flash",
    "deepseek-v4-pro",
    "deepseek-v4-pro-0813",
    "deepseek-v4-flash-0731",
    "glm-5.3",
    "glm-5.2",
];
const MUSE_MODELS: &[&str] = &["muse-spark-1.3", "muse-spark-1.3-contributor"];

/// The declared caps-v1 host keys (excluding `engine:*`), ordinal-sorted — for the "no effort
/// vocabulary declared for <host>" refusal (`codex-consult-common.ps1:2718`). Kept in sorted
/// order literally so it needs no runtime sort.
pub const DECLARED_HOSTS: &[&str] = &[
    "api.kimi.ai",
    "api.xiaomimimo.com",
    "api.z.ai",
    "ark.ap-southeast.bytepluses.com",
    "builtin:openai",
    "open.bigmodel.cn",
    "token-plan-ams.xiaomimimo.com",
    "token-plan-cn.xiaomimimo.com",
    "token-plan.ap-southeast-1.maas.aliyuncs.com",
];

/// The caps-v1 row for a caps key (a host name, or `engine:<name>`), or `None`.
pub fn caps(key: &str) -> Option<Caps> {
    let row = match key {
        "builtin:openai" => Caps {
            vocabulary: "openai",
            models: Models::Any,
            schema_transport: "output-schema",
        },
        "api.z.ai" => Caps {
            vocabulary: "zai",
            models: Models::List(ZAI_MODELS),
            schema_transport: "output-schema",
        },
        "open.bigmodel.cn" => Caps {
            vocabulary: "zai",
            models: Models::List(ZAI_MODELS),
            schema_transport: "output-schema",
        },
        "token-plan-ams.xiaomimimo.com" => Caps {
            vocabulary: "mimo",
            models: Models::List(MIMO_MODELS),
            schema_transport: "prompt-only",
        },
        "token-plan-cn.xiaomimimo.com" => Caps {
            vocabulary: "mimo",
            models: Models::List(MIMO_MODELS),
            schema_transport: "prompt-only",
        },
        "api.xiaomimimo.com" => Caps {
            vocabulary: "mimo",
            models: Models::List(MIMO_MODELS),
            schema_transport: "prompt-only",
        },
        "ark.ap-southeast.bytepluses.com" => Caps {
            vocabulary: "ark",
            models: Models::List(ARK_MODELS),
            schema_transport: "prompt-only",
        },
        "api.kimi.ai" => Caps {
            vocabulary: "kimi",
            models: Models::List(KIMI_MODELS),
            schema_transport: "prompt-only",
        },
        "token-plan.ap-southeast-1.maas.aliyuncs.com" => Caps {
            vocabulary: "alibaba",
            models: Models::List(ALIBABA_MODELS),
            schema_transport: "prompt-only",
        },
        "engine:agy" => Caps {
            vocabulary: "model-tier",
            models: Models::Any,
            schema_transport: "native",
        },
        "engine:muse" => Caps {
            vocabulary: "muse",
            models: Models::List(MUSE_MODELS),
            schema_transport: "native",
        },
        _ => return None,
    };
    Some(row)
}

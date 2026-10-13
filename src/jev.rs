//! Classifies issues into modules with Jev (TypeSafe's decision model) through
//! OpenRouter's alpha decisions API: one `noul` (yes/no) question per module,
//! whose P(yes) becomes that module's probability.

use std::collections::BTreeMap;
use std::fmt;
use std::io::Write;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, anyhow, bail};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::modulemap::Module;
use crate::world::{Classification, Issue};

/// Pinned, not the `~typesafe/jev-latest` alias, so answers stay comparable.
pub const MODEL: &str = "typesafe/jev-1.13";

const URL: &str = "https://openrouter.ai/api/alpha/decisions";
const KEY_VAR: &str = "AGENTGRAPH_OPENROUTER_API_KEY";

/// About 4k tokens of issue body, at roughly 4 characters per token.
const MAX_BODY_CHARS: usize = 16_000;

const QUESTION: &str = "Does the work described in the issue belong mainly to `module`?";

/// The Jev collector. Holds only whether the API key is set; the key itself
/// stays in the environment, where curl reads it.
pub struct Jev {
    key_set: bool,
}

/// What one tick of classifying got.
pub struct Classified {
    pub classifications: BTreeMap<u64, Classification>,
    /// Why classifying stopped early, if it did.
    pub failure: Option<Failure>,
}

/// Why classifying stopped. Its `Display` is the error line to show.
#[derive(Debug, PartialEq)]
pub enum Failure {
    KeyNotSet,
    /// A request failed: curl could not reach OpenRouter, it answered with a
    /// non-2xx status, or the body was not a Jev answer.
    Unreachable(String),
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Failure::KeyNotSet => write!(f, "{KEY_VAR} not set"),
            Failure::Unreachable(detail) => write!(f, "OpenRouter unreachable: {detail}"),
        }
    }
}

impl Jev {
    pub fn from_env() -> Jev {
        Jev {
            key_set: std::env::var_os(KEY_VAR).is_some_and(|k| !k.is_empty()),
        }
    }

    /// A collector with no key, for tests.
    #[cfg(test)]
    pub fn without_key() -> Jev {
        Jev { key_set: false }
    }

    /// Classifies each issue with one Jev request. A missing key or a failed
    /// request is never fatal; the first failure stops the tick, so an
    /// offline tick costs one timeout rather than one per issue. Without a
    /// key it fails even with no issues, so the error line shows while every
    /// placement comes from the cache.
    pub fn classify(&self, issues: &[&Issue], modules: &[Module]) -> Classified {
        let mut classified = Classified {
            classifications: BTreeMap::new(),
            failure: None,
        };
        if !self.key_set {
            classified.failure = Some(Failure::KeyNotSet);
            return classified;
        }
        if modules.is_empty() {
            return classified;
        }
        for issue in issues {
            match request_classification(&request_body(issue, modules), modules) {
                Ok(c) => {
                    classified.classifications.insert(issue.number, c);
                }
                Err(e) => {
                    classified.failure = Some(Failure::Unreachable(format!("{e:#}")));
                    break;
                }
            }
        }
        classified
    }
}

/// Sends one request through `curl` and reads Jev's answer to it. The key
/// reaches curl through its environment (`--variable %VAR`), never argv.
fn request_classification(body: &Value, modules: &[Module]) -> Result<Classification> {
    let mut child = Command::new("curl")
        .args(["-sS", "--max-time", "30", "-X", "POST", URL])
        .args(["-H", "Content-Type: application/json"])
        .args(["--variable", &format!("%{KEY_VAR}")])
        .args([
            "--expand-header",
            &format!("Authorization: Bearer {{{{{KEY_VAR}}}}}"),
        ])
        .args(["--data-binary", "@-", "-w", "\n%{http_code}"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("running curl (is curl installed?)")?;
    child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(body.to_string().as_bytes())
        .context("writing the request to curl")?;
    let out = child.wait_with_output().context("waiting for curl")?;
    if !out.status.success() {
        bail!("{}", String::from_utf8_lossy(&out.stderr).trim());
    }
    read_response(&String::from_utf8_lossy(&out.stdout), modules)
}

/// Reads curl's output: the body, then the HTTP status on the last line.
fn read_response(stdout: &str, modules: &[Module]) -> Result<Classification> {
    let (json, status) = stdout.rsplit_once('\n').unwrap_or(("", stdout));
    if !status.starts_with('2') {
        bail!("{status} {}", error_message(json));
    }
    parse_answers(json, modules).context("unexpected Jev response")
}

/// The request body for one issue: its title and body as the state, and one
/// question per module, keyed by module id.
fn request_body(issue: &Issue, modules: &[Module]) -> Value {
    let body: String = issue.body.chars().take(MAX_BODY_CHARS).collect();
    let questions: serde_json::Map<String, Value> = modules
        .iter()
        .map(|m| {
            let module = match &m.description {
                Some(d) => json!({"label": m.label, "description": d}),
                None => json!({"label": m.label, "paths": m.paths}),
            };
            let question = json!({
                "type": "noul",
                "instructions": {"module": module, "question": QUESTION},
            });
            (m.id.clone(), question)
        })
        .collect();
    json!({
        "model": MODEL,
        "state": {"issue_title": issue.title, "issue_body": body},
        "questions": questions,
    })
}

/// Reads P(yes) per module from a successful response.
fn parse_answers(json: &str, modules: &[Module]) -> Result<Classification> {
    #[derive(Deserialize)]
    struct Resp {
        answers: BTreeMap<String, Answer>,
    }
    #[derive(Deserialize)]
    struct Answer {
        noul: f64,
    }
    let resp: Resp = serde_json::from_str(json)?;
    modules
        .iter()
        .map(|m| {
            let a = resp
                .answers
                .get(&m.id)
                .ok_or_else(|| anyhow!("no answer for module `{}`", m.id))?;
            Ok((m.id.clone(), a.noul as f32))
        })
        .collect()
}

/// The `error.message` of an OpenRouter error body, or the raw body.
fn error_message(json: &str) -> String {
    serde_json::from_str::<Value>(json)
        .ok()
        .and_then(|v| v["error"]["message"].as_str().map(str::to_owned))
        .unwrap_or_else(|| json.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modulemap::module;
    use serde_json::json;

    fn issue(title: &str, body: &str) -> Issue {
        Issue {
            number: 102,
            title: title.into(),
            body: body.into(),
            updated_at: "2026-10-01T10:00:00Z".into(),
            parent: None,
        }
    }

    #[test]
    fn asks_the_pinned_model_one_noul_question_per_module() {
        let modules = [
            module("search", "Search", Some("Query parsing and ranking."), &[]),
            module("server", "Server", Some("The MCP server."), &[]),
        ];
        let body = request_body(&issue("Floor scores", "Drop hits below 2."), &modules);

        assert_eq!(body["model"], MODEL);
        assert_eq!(
            body["state"],
            json!({"issue_title": "Floor scores", "issue_body": "Drop hits below 2."})
        );
        let questions = body["questions"].as_object().unwrap();
        assert_eq!(
            questions.keys().collect::<Vec<_>>(),
            ["search", "server"],
            "one question per module, keyed by id"
        );
        let search = &questions["search"];
        assert_eq!(search["type"], "noul");
        assert_eq!(
            search["instructions"]["module"],
            json!({"label": "Search", "description": "Query parsing and ranking."})
        );
        assert!(search["instructions"]["question"].is_string());
    }

    #[test]
    fn falls_back_to_label_and_paths_without_a_description() {
        let modules = [module("server", "Server", None, &["src/server/**"])];
        let body = request_body(&issue("t", "b"), &modules);
        assert_eq!(
            body["questions"]["server"]["instructions"]["module"],
            json!({"label": "Server", "paths": ["src/server/**"]})
        );
    }

    #[test]
    fn truncates_a_long_body_on_a_char_boundary() {
        let long = "é".repeat(MAX_BODY_CHARS + 10);
        let body = request_body(&issue("t", &long), &[]);
        let sent = body["state"]["issue_body"].as_str().unwrap();
        assert_eq!(sent.chars().count(), MAX_BODY_CHARS);
    }

    #[test]
    fn reads_p_yes_per_module_from_the_answers() {
        let modules = [
            module("search", "Search", None, &[]),
            module("server", "Server", None, &[]),
        ];
        let json = r#"{
            "id": "gen-dec-1", "model": "typesafe/jev-1.13-20260917",
            "answers": {
                "search": {"type": "noul", "noul": 0.82},
                "server": {"type": "noul", "noul": 0.31}
            },
            "usage": {"input_tokens": 476, "output_tokens": 70}
        }"#;
        let c = parse_answers(json, &modules).unwrap();
        assert_eq!(c["search"], 0.82);
        assert_eq!(c["server"], 0.31);
    }

    #[test]
    fn a_module_missing_from_the_answers_is_an_error_not_zero() {
        let modules = [
            module("search", "Search", None, &[]),
            module("server", "Server", None, &[]),
        ];
        let json = r#"{"answers": {"search": {"type": "noul", "noul": 0.82}}}"#;
        let err = parse_answers(json, &modules).unwrap_err();
        assert!(format!("{err:#}").contains("server"), "{err:#}");
    }

    #[test]
    fn a_non_2xx_response_reads_as_its_status_and_message() {
        let modules = [module("search", "Search", None, &[])];
        let stdout = "{\"error\":{\"message\":\"User not found\",\"code\":401}}\n401";
        let err = read_response(stdout, &modules).unwrap_err();
        assert_eq!(format!("{err:#}"), "401 User not found");
    }

    #[test]
    fn a_malformed_2xx_body_is_an_error() {
        let modules = [module("search", "Search", None, &[])];
        let err = read_response("<html>oops</html>\n200", &modules).unwrap_err();
        assert!(
            format!("{err:#}").contains("unexpected Jev response"),
            "{err:#}"
        );
    }

    #[test]
    fn a_2xx_response_reads_as_the_classification() {
        let modules = [module("search", "Search", None, &[])];
        let stdout = "{\"answers\": {\"search\": {\"type\": \"noul\", \"noul\": 0.82}}}\n200";
        assert_eq!(read_response(stdout, &modules).unwrap()["search"], 0.82);
    }

    #[test]
    fn every_failed_request_shows_as_openrouter_unreachable() {
        let failure = Failure::Unreachable("401 User not found".into());
        assert_eq!(
            failure.to_string(),
            "OpenRouter unreachable: 401 User not found"
        );
    }

    #[test]
    fn without_a_key_nothing_is_sent_and_the_failure_says_so() {
        let jev = Jev { key_set: false };
        let modules = [module("search", "Search", None, &[])];
        let issue = issue("t", "b");
        let classified = jev.classify(&[&issue], &modules);
        assert!(classified.classifications.is_empty());
        assert_eq!(
            classified.failure.map(|f| f.to_string()).as_deref(),
            Some("AGENTGRAPH_OPENROUTER_API_KEY not set")
        );
    }

    #[test]
    fn error_message_reads_openrouter_errors_and_falls_back_to_the_body() {
        let json = r#"{"error":{"message":"Missing Authentication header","code":401}}"#;
        assert_eq!(error_message(json), "Missing Authentication header");
        assert_eq!(
            error_message("<html>Bad gateway</html>"),
            "<html>Bad gateway</html>"
        );
    }
}

//! Classifies issues into modules with Jev (TypeSafe's decision model) through
//! OpenRouter's alpha decisions API: one `noul` (yes/no) question per module,
//! whose P(yes) becomes that module's probability.

use std::collections::BTreeMap;
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
const KEY_VAR: &str = "OPENROUTER_API_KEY";

/// About 4k tokens of issue body, at roughly 4 characters per token.
const MAX_BODY_CHARS: usize = 16_000;

const QUESTION: &str = "Does the work described in the issue belong mainly to `module`?";

/// Classifies each issue with one Jev request. Returns the classifications it
/// got and, when something failed, the error line to show. A missing key or a
/// failed request is never fatal; the first failure stops the tick, so an
/// offline tick costs one timeout rather than one per issue.
pub fn classify(
    issues: &[&Issue],
    modules: &[Module],
) -> (BTreeMap<u64, Classification>, Option<String>) {
    let mut classified = BTreeMap::new();
    if std::env::var_os(KEY_VAR).is_none_or(|k| k.is_empty()) {
        return (classified, Some(format!("{KEY_VAR} not set")));
    }
    if modules.is_empty() {
        return (classified, None);
    }
    for issue in issues {
        match decide(&request_body(issue, modules), modules) {
            Ok(c) => {
                classified.insert(issue.number, c);
            }
            Err(e) => return (classified, Some(format!("{e:#}"))),
        }
    }
    (classified, None)
}

/// Sends one request through `curl`. The key reaches curl through its
/// environment (`--variable %VAR`), never argv.
fn decide(body: &Value, modules: &[Module]) -> Result<Classification> {
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
        .context("OpenRouter unreachable: running curl (is curl installed?)")?;
    child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(body.to_string().as_bytes())
        .context("OpenRouter unreachable: writing the request to curl")?;
    let out = child
        .wait_with_output()
        .context("OpenRouter unreachable: waiting for curl")?;
    if !out.status.success() {
        bail!(
            "OpenRouter unreachable: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }

    let stdout = String::from_utf8_lossy(&out.stdout);
    let (json, status) = stdout.rsplit_once('\n').unwrap_or(("", &stdout));
    if !status.starts_with('2') {
        bail!("OpenRouter error: {status} {}", error_message(json));
    }
    parse_answers(json, modules).context("OpenRouter: unexpected Jev response")
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
    use serde_json::json;

    fn module(id: &str, label: &str, description: Option<&str>, paths: &[&str]) -> Module {
        Module {
            id: id.into(),
            label: label.into(),
            description: description.map(Into::into),
            paths: paths.iter().map(|p| p.to_string()).collect(),
        }
    }

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
    fn error_message_reads_openrouter_errors_and_falls_back_to_the_body() {
        let json = r#"{"error":{"message":"Missing Authentication header","code":401}}"#;
        assert_eq!(error_message(json), "Missing Authentication header");
        assert_eq!(
            error_message("<html>Bad gateway</html>"),
            "<html>Bad gateway</html>"
        );
    }
}

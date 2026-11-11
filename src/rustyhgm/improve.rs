use std::fs;
use std::path::{Path, PathBuf};

use chrono::Local;
use rand::seq::IndexedRandom;
use rand::{Rng, RngExt};
use rustyhgm_bench::swe::{Step, key_mount, message, run_logged, run_step, service_flags};
use rustyhgm_coding_agent::llm::{
    Client, Service, extract_json_between_markers, get_response_from_llm,
};
use rustyhgm_coding_agent::testrepo::{TestRepo, get_test_description};
use rustyhgm_core::log::Log;
use rustyhgm_core::metadata::{MODEL_PATCH_FILE, model_patch_paths, read_overall_performance};
use rustyhgm_core::prompts::{
    DiagnoseInput, SOLVE_CONTEXTLENGTH, SOLVE_EMPTY_PATCHES, SOLVE_STOCHASTICITY,
    any_exceeding_context_length, get_diagnose_prompt_polyglot, get_diagnose_prompt_swe,
    get_problem_description_prompt,
};
use rustyhgm_core::pyjson::PyValue;
use rustyhgm_sandbox::{Container, Docker, ExecOptions, RunOptions};
use serde_json::Value;

use crate::FAILED;
use crate::agent_build::{AGENT_BINARY, INSTALLED_AGENT, compile, copy_binary, patch_steps};
use crate::node::{Metadata, put, save_metadata};

pub const DIAGNOSE_ATTEMPTS: usize = 3;
pub const CHAT_HISTORY: &str = "/rustyhgm/self_evo.md";

pub trait Improver: Sync {
    fn sample_child(&self, parent_commit: &str) -> String;
}

fn pick<R: Rng + ?Sized>(ids: &[&String], rng: &mut R) -> Option<String> {
    ids.choose(rng).map(|id| (*id).clone())
}

pub fn choose_entry<R: Rng + ?Sized>(
    output_dir: &Path,
    parent_commit: &str,
    polyglot: bool,
    rng: &mut R,
) -> Result<String, String> {
    let overall = read_overall_performance(&output_dir.join(parent_commit))
        .map_err(|e| format!("{parent_commit} not eligible for being a parent: {e}"))?;
    let empty = &overall.total_emptypatch_ids;
    let resolved = &overall.total_resolved_ids;
    let unresolved = &overall.total_unresolved_ids;
    let stuck: Vec<&String> = empty.iter().chain(unresolved).collect();
    let all: Vec<&String> = resolved.iter().chain(stuck.iter().copied()).collect();
    let entry = if polyglot {
        pick(if stuck.is_empty() { &all } else { &stuck }, rng)
    } else if empty.len() as f64 >= 0.1 * all.len() as f64 && rng.random::<f64>() < 0.25 {
        Some(SOLVE_EMPTY_PATCHES.to_owned())
    } else if rng.random::<f64>() < 0.25 {
        Some(SOLVE_STOCHASTICITY.to_owned())
    } else if any_exceeding_context_length(output_dir, parent_commit, &stuck)
        .map_err(|e| e.to_string())?
        && rng.random::<f64>() < 0.25
    {
        Some(SOLVE_CONTEXTLENGTH.to_owned())
    } else if unresolved.is_empty() {
        pick(&all, rng)
    } else {
        pick(&unresolved.iter().collect::<Vec<_>>(), rng)
    };
    entry.ok_or_else(|| {
        format!("Failed to choose an entry for self-improvement based on {parent_commit}.")
    })
}

pub fn diagnose_problem<R: Rng + ?Sized>(
    input: &DiagnoseInput,
    polyglot: bool,
    client: &Client,
    rng: &mut R,
    log: &Log,
) -> Result<Option<String>, String> {
    for _ in 0..DIAGNOSE_ATTEMPTS {
        let (system, prompt) = if polyglot {
            get_diagnose_prompt_polyglot(input, rng)?
        } else {
            get_diagnose_prompt_swe(input)?
        };
        let attempt = get_response_from_llm(&prompt, client, &system, &[], 0.7)
            .map_err(|e| {
                log.info(format!("Error with get_response_from_llm: {e}"));
                e.to_string()
            })
            .and_then(|(response, _)| {
                extract_json_between_markers(&response)
                    .ok_or_else(|| "empty response json".to_owned())
            })
            .and_then(|json| get_problem_description_prompt(&json, polyglot));
        match attempt {
            Ok(statement) => return Ok(Some(statement)),
            Err(e) => log.info(format!("Error while diagnosing the problem: {e}")),
        }
    }
    Ok(None)
}

pub fn child_performance() -> PyValue {
    let empty = || PyValue::List(Vec::new());
    PyValue::dict([
        ("accuracy_score", PyValue::Float(0.0)),
        ("total_resolved_instances", PyValue::Int(0)),
        ("total_submitted_instances", PyValue::Int(0)),
        ("files", empty()),
        ("total_submitted_ids", empty()),
        ("total_unresolved_ids", empty()),
        ("total_emptypatch_ids", empty()),
        ("total_resolved_ids", empty()),
    ])
}

pub struct SelfImprove {
    pub docker: Docker,
    pub agent_source: PathBuf,
    pub output_dir: PathBuf,
    pub polyglot: bool,
    pub dataset: Result<Vec<Value>, String>,
    pub image_name: String,
    pub diagnose: Client,
    pub self_improve_llm: String,
    pub self_improve_service: Service,
    pub timeout: u64,
}

struct Attempt<'a> {
    parent_commit: &'a str,
    run_id: &'a str,
    run_dir: &'a Path,
    log: &'a Log,
}

impl SelfImprove {
    fn prepare(
        &self,
        container: &Container,
        patches: &[PathBuf],
        log: &Log,
    ) -> Result<String, String> {
        let mut steps = patch_steps(patches);
        steps.extend([
            Step::Quiet(&["git", "init"], "/rustyhgm/"),
            Step::Run(&["git", "add", "--all"], "/rustyhgm/"),
            Step::Quiet(
                &[
                    "git",
                    "-c",
                    "user.name=user",
                    "-c",
                    "user.email=you@example.com",
                    "commit",
                    "-m",
                    "a nonsense commit message",
                ],
                "/rustyhgm/",
            ),
        ]);
        steps
            .iter()
            .try_for_each(|step| run_step(container, step, log))
            .and_then(|()| run_logged(container, &["git", "log"], &ExecOptions::default(), log))
            .map_err(message)
            .and_then(|out| {
                out.lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .map(str::to_owned)
                    .ok_or_else(|| format!("git log printed no commit: {out}"))
            })
    }

    fn improve(
        &self,
        container: &Container,
        statement: &str,
        commit_hash: &str,
        attempt: &Attempt,
    ) -> Result<(), String> {
        let log = attempt.log;
        log.info("Running self-improvement");
        let timeout = self.timeout.to_string();
        let test_description = get_test_description("", TestRepo::Hgm);
        let mut argv = vec![
            "timeout",
            &timeout,
            INSTALLED_AGENT,
            "--problem_statement",
            statement,
            "--git_dir",
            "/rustyhgm/",
            "--chat_history_file",
            CHAT_HISTORY,
            "--base_commit",
            commit_hash,
            "--outdir",
            "/rustyhgm/",
            "--test_description",
            &test_description,
            "--self_improve",
            "true",
            "--polyglot",
            if self.polyglot { "true" } else { "false" },
            "--language",
            "rust",
            "--model",
            &self.self_improve_llm,
            "--timeout",
            &timeout,
        ];
        let service = service_flags(&self.self_improve_service);
        argv.extend(service.iter().map(String::as_str));
        let patch = attempt.run_dir.join(MODEL_PATCH_FILE);
        container
            .exec(&argv, &ExecOptions::workdir("/"))
            .and_then(|r| r.log(log, false))
            .and_then(|()| {
                container.copy_out(CHAT_HISTORY, &attempt.run_dir.join("self_evo.md"), log)
            })
            .and_then(|()| container.copy_out("/rustyhgm/model_patch.diff", &patch, log))
            .map_err(message)
    }

    fn attempt(
        &self,
        attempt: &Attempt,
        metadata: &mut Metadata,
        container: &mut Option<Container>,
    ) -> Result<bool, String> {
        let log = attempt.log;
        let dataset = self.dataset.as_ref().map_err(Clone::clone)?;
        let name = format!("rustyhgm-container-{}", attempt.run_id);
        let started = self
            .docker
            .remove_existing_container(&name, log)
            .and_then(|()| {
                self.docker
                    .ensure_image(&self.agent_source, &self.image_name, false, log)
            })
            .and_then(|()| {
                let opts = RunOptions {
                    read_only_mounts: key_mount(&self.self_improve_service),
                    ..RunOptions::default()
                };
                self.docker
                    .run_detached_as(&self.image_name, &name, &opts, log)
            })
            .map_err(message)?;
        let started = container.insert(started);
        let patch_files = model_patch_paths(&self.output_dir, "", attempt.parent_commit)
            .map_err(|e| e.to_string())?;
        let commit_hash = self.prepare(started, &patch_files, log)?;
        compile(started, log)?;
        log.info("Getting tasks to improve");
        let mut rng = rand::rng();
        let entry = choose_entry(
            &self.output_dir,
            attempt.parent_commit,
            self.polyglot,
            &mut rng,
        )
        .inspect_err(|e| log.info(format!("Error choosing entry: {e}")))?;
        log.info(format!("Task to improve: {entry}"));
        let input = DiagnoseInput {
            entry_id: &entry,
            commit: attempt.parent_commit,
            root_dir: &self.agent_source,
            out_dir: &self.output_dir,
            dataset,
            patch_files: &patch_files,
        };
        let statement = diagnose_problem(&input, self.polyglot, &self.diagnose, &mut rng, log)?;
        log.info(format!(
            "problem_statement: {}",
            statement.as_deref().unwrap_or("None")
        ));
        put(metadata, "entry", PyValue::str(&entry));
        put(
            metadata,
            "problem_statement",
            statement.clone().map_or(PyValue::Null, PyValue::Str),
        );
        let Some(statement) = statement else {
            log.info("Failed to diagnose the problem statement. Exiting.");
            return Ok(false);
        };
        self.improve(started, &statement, &commit_hash, attempt)?;
        put(metadata, "overall_performance", child_performance());
        fs::read_to_string(attempt.run_dir.join(MODEL_PATCH_FILE))
            .ok()
            .filter(|text| !text.trim().is_empty())
            .ok_or_else(|| "Model patch file is empty or does not exist".to_owned())?;
        compile(started, log)
            .and_then(|()| copy_binary(started, &attempt.run_dir.join(AGENT_BINARY), log))
            .map(|()| true)
    }
}

impl Improver for SelfImprove {
    fn sample_child(&self, parent_commit: &str) -> String {
        let run_id = Local::now().format("%Y%m%d_%H%M%S_%6f").to_string();
        let run_dir = self.output_dir.join(&run_id);
        let log = match fs::create_dir_all(&run_dir)
            .and_then(|()| Log::to_file(&run_dir.join("self_improve.log")))
        {
            Ok(log) => log,
            Err(e) => {
                eprintln!("Error while sampling a child: {e}");
                return FAILED.to_owned();
            }
        };
        let mut metadata: Metadata = vec![
            ("run_id".to_owned(), PyValue::str(&run_id)),
            ("parent_commit".to_owned(), PyValue::str(parent_commit)),
        ];
        let mut container = None;
        let attempt = Attempt {
            parent_commit,
            run_id: &run_id,
            run_dir: &run_dir,
            log: &log,
        };
        let outcome = self.attempt(&attempt, &mut metadata, &mut container);
        if let Some(container) = container {
            container.cleanup(&log);
        }
        if let Err(e) = save_metadata(&run_dir, &metadata) {
            log.error(format!("Error saving metadata: {e}"));
        }
        match outcome {
            Ok(true) => run_id,
            Ok(false) => FAILED.to_owned(),
            Err(e) => {
                log.info(format!("Error while sampling a child: {e}"));
                FAILED.to_owned()
            }
        }
    }
}

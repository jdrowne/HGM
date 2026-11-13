use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::Local;
use rustyhgm_coding_harness::llm::Service;
use rustyhgm_coding_harness::testrepo::{TestRepo, get_test_description};
use rustyhgm_core::log::Log;
use rustyhgm_core::pyjson::{PyValue, dumps};
use rustyhgm_sandbox::{Container, Docker, ExecOptions};
use serde_json::Value;

use crate::polyglot_build::Images;
use crate::polyglot_spec::{TestSpec, get_eval_script, make_test_spec, test_cmd, test_commands};
use crate::swe::{
    HARNESS_PATH, Run, Step, container_run_id, finished, key_mount, message, run_logged, run_step,
    salvage, service_flags,
};

pub const DATASET: &str = "polyglot_benchmark_metadata.json";

#[derive(Clone, Debug)]
pub struct Evaluator {
    pub docker: Docker,
    pub root: PathBuf,
    pub dataset: PathBuf,
    pub machine: String,
    pub namespace: String,
    pub llm: String,
    pub service: Service,
    pub timeout: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outcome {
    pub instance_id: String,
    pub success: bool,
    pub eval_result: String,
    pub error: Option<String>,
}

enum Ending {
    Scored(String),
    TimedOut,
}

struct Job<'a> {
    entry: &'a Value,
    out_dname: &'a Path,
    run: &'a Run,
}

fn text(entry: &Value, key: &str) -> String {
    entry[key].as_str().unwrap_or_default().to_owned()
}

enum State<'a> {
    Scored(&'a str),
    Failed(&'a str),
    TimedOut,
}

fn result(job: &Job, model_patch: &str, proposed: PyValue, state: State) -> String {
    let (eval_result, success, timed_out) = match state {
        State::Scored(r) => (r, true, false),
        State::Failed(r) => (r, false, false),
        State::TimedOut => ("incomplete", false, true),
    };
    dumps(
        &PyValue::dict([
            ("instance_id", PyValue::str(text(job.entry, "instance_id"))),
            (
                "model_name_or_path",
                PyValue::str(&job.run.model_name_or_path),
            ),
            ("model_patch", PyValue::str(model_patch)),
            ("proposed_model_patches", proposed),
            ("eval_result", PyValue::str(eval_result)),
            ("success", PyValue::Bool(success)),
            ("timed_out", PyValue::Bool(timed_out)),
        ]),
        Some(4),
    )
}

fn run_steps(container: &Container, steps: &[Step], log: &Log) -> Result<(), String> {
    steps
        .iter()
        .try_for_each(|step| run_step(container, step, log))
        .map_err(message)
}

fn write(path: &Path, contents: impl AsRef<[u8]>) -> Result<(), String> {
    fs::write(path, contents).map_err(|e| e.to_string())
}

impl Evaluator {
    fn spec(&self, entry: &Value) -> TestSpec {
        TestSpec {
            namespace: self.namespace.clone(),
            ..make_test_spec(entry, &self.machine)
        }
    }

    fn setup(&self, job: &Job, container: &Container, log: &Log) -> Result<(), String> {
        let steps = [
            Step::CopyIn(job.run.harness.clone(), HARNESS_PATH),
            Step::Run(&["ls", "-R", "/testbed"], "/"),
        ];
        run_steps(container, &steps, log)
    }

    fn solve(&self, job: &Job, container: &Container, log: &Log) -> Result<bool, String> {
        let instance_id = text(job.entry, "instance_id");
        let language = text(job.entry, "language");
        let chat_history = format!("/rustyhgm/{instance_id}.md");
        let timeout = self.timeout.to_string();
        let problem_statement = text(job.entry, "problem_statement");
        let base_commit = text(job.entry, "base_commit");
        let test_description = get_test_description(&test_cmd(&language), TestRepo::Polyglot);
        log.info("Running the harness");
        let service = service_flags(&self.service);
        let mut argv = vec![
            "timeout",
            &timeout,
            HARNESS_PATH,
            "--problem_statement",
            &problem_statement,
            "--git_dir",
            "/testbed/",
            "--chat_history_file",
            &chat_history,
            "--base_commit",
            &base_commit,
            "--outdir",
            "/rustyhgm/",
            "--test_description",
            &test_description,
            "--polyglot",
            "--language",
            &language,
            "--model",
            &self.llm,
            "--timeout",
            &timeout,
        ];
        argv.extend(service.iter().map(String::as_str));
        finished(run_logged(
            container,
            &argv,
            &ExecOptions::workdir("/testbed/"),
            log,
        ))
    }

    fn collect(&self, job: &Job, container: &Container, log: &Log) -> Result<String, String> {
        let instance_id = text(job.entry, "instance_id");
        log.info("Copying output files back to host");
        let chat_history = format!("/rustyhgm/{instance_id}.md");
        let pattern = format!("{instance_id}_*.md");
        let find = ["find", "/rustyhgm/", "-name", &pattern];
        container
            .copy_out(
                &chat_history,
                &job.out_dname.join(format!("{instance_id}.md")),
                log,
            )
            .and_then(|()| container.exec(&find, &ExecOptions::workdir("/")))
            .and_then(|found| {
                found.text().split_whitespace().try_for_each(|file| {
                    let name = file.rsplit('/').next().unwrap_or(file);
                    container.copy_out(file, &job.out_dname.join(name), log)
                })
            })
            .map_err(message)?;
        log.info("Getting model_patch");
        run_logged(
            container,
            &["cat", "/rustyhgm/model_patch.diff"],
            &ExecOptions::default(),
            log,
        )
        .map_err(message)
    }

    fn evaluate_patch(
        &self,
        job: &Job,
        container: &Container,
        log: &Log,
    ) -> Result<String, String> {
        let instance_id = text(job.entry, "instance_id");
        let language = text(job.entry, "language");
        let solution: Vec<&str> = job.entry["files"]["solution"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        let restore = [
            Step::Command(
                format!("git -C /testbed stash push {}", solution.join(" ")),
                "/",
            ),
            Step::Command(
                format!(
                    "git -C /testbed reset --hard {}",
                    text(job.entry, "test_commit")
                ),
                "/",
            ),
            Step::Run(&["git", "-C", "/testbed", "clean", "-fd"], "/"),
            Step::Run(&["git", "-C", "/testbed", "stash", "pop"], "/"),
            Step::Info("Running the eval"),
        ];
        let eval_file = job.out_dname.join(format!("{instance_id}_eval.sh"));
        let eval_result_file = job.out_dname.join(format!("{instance_id}_eval.md"));
        let prepare = [
            Step::CopyIn(eval_file.clone(), "/testbed/eval.sh"),
            Step::Run(&["ls", "-R", "/testbed"], "/"),
            Step::Run(&["chmod", "+x", "/testbed/eval.sh"], "/"),
        ];
        let eval = run_steps(container, &restore, log)
            .and_then(|()| write(&eval_file, get_eval_script(test_commands(&language))))
            .and_then(|()| run_steps(container, &prepare, log))
            .and_then(|()| {
                container
                    .exec(
                        &["timeout", "120", "./eval.sh"],
                        &ExecOptions::workdir("/testbed"),
                    )
                    .map_err(message)
            })?;
        eval.log(log, false).ok();
        write(&eval_result_file, eval.text())?;
        Ok(if eval.success() {
            "resolved"
        } else {
            "unresolved"
        }
        .to_owned())
    }

    fn attempt(
        &self,
        job: &Job,
        container: &mut Option<Container>,
        model_patch: &mut Option<String>,
        log: &Log,
    ) -> Result<Ending, String> {
        let spec = self.spec(job.entry);
        let run_id = container_run_id();
        self.docker
            .remove_existing_container(&spec.container_name(&run_id), log)
            .map_err(message)?;
        let mounts = key_mount(&self.service);
        let images = Images {
            docker: &self.docker,
            root: &self.root,
            read_only_mounts: &mounts,
        };
        let started = container.insert(images.build_container(&spec, &run_id, log, true)?);
        self.setup(job, started, log)?;
        if !self.solve(job, started, log)? {
            *model_patch = Some(salvage(
                started,
                &spec.instance_id,
                &text(job.entry, "base_commit"),
                job.out_dname,
                log,
            ));
            return Ok(Ending::TimedOut);
        }
        let patch = model_patch.insert(self.collect(job, started, log)?);
        let eval_result = if patch.is_empty() {
            "empty_patch".to_owned()
        } else {
            self.evaluate_patch(job, started, log)?
        };
        let out_fname = job.out_dname.join(format!("{}.json", spec.instance_id));
        write(
            &out_fname,
            result(
                job,
                patch,
                PyValue::List(Vec::new()),
                State::Scored(&eval_result),
            ),
        )?;
        Ok(Ending::Scored(eval_result))
    }

    fn process_entry(&self, job: &Job) -> Outcome {
        let instance_id = text(job.entry, "instance_id");
        let out_fname = job.out_dname.join(format!("{instance_id}.json"));
        if out_fname.exists() {
            println!("Skipping existing entry {instance_id}");
            let saved: Value = fs::read_to_string(&out_fname)
                .ok()
                .and_then(|t| serde_json::from_str(&t).ok())
                .unwrap_or_default();
            let eval_result = text(&saved, "eval_result");
            let success = saved["success"].as_bool().unwrap_or_default();
            return Outcome {
                instance_id,
                success,
                error: (!success).then(|| format!("recorded earlier as {eval_result}")),
                eval_result,
            };
        }
        let log = Log::to_file(&job.out_dname.join(format!("{instance_id}_docker.log")))
            .unwrap_or_else(|_| Log::discard());
        let mut container = None;
        let mut model_patch = None;
        let attempted = self.attempt(job, &mut container, &mut model_patch, &log);
        let outcome = match attempted {
            Ok(Ending::Scored(eval_result)) => Outcome {
                instance_id,
                success: true,
                eval_result,
                error: None,
            },
            Ok(Ending::TimedOut) => {
                let patch = model_patch.unwrap_or_default();
                if let Err(write_error) = write(
                    &out_fname,
                    result(job, &patch, PyValue::List(Vec::new()), State::TimedOut),
                ) {
                    println!("Error writing result for {instance_id}: {write_error}");
                }
                let e = format!("timed out after {} s", self.timeout);
                log.error(format!("Error processing entry {instance_id}: {e}"));
                Outcome {
                    instance_id,
                    success: false,
                    eval_result: "incomplete".to_owned(),
                    error: Some(e),
                }
            }
            Err(e) => {
                let (patch, proposed, eval_result) = match model_patch {
                    None => (String::new(), PyValue::str(""), "incomplete"),
                    Some(patch) => (patch, PyValue::List(Vec::new()), "error"),
                };
                if let Err(write_error) = write(
                    &out_fname,
                    result(job, &patch, proposed, State::Failed(eval_result)),
                ) {
                    println!("Error writing result for {instance_id}: {write_error}");
                }
                log.error(format!("Error processing entry {instance_id}: {e}"));
                Outcome {
                    instance_id,
                    success: false,
                    eval_result: eval_result.to_owned(),
                    error: Some(e),
                }
            }
        };
        if let Some(c) = &container {
            c.cleanup_within(15, &log);
        }
        outcome
    }

    pub fn evaluate(&self, entries: &[Value], out_dname: &Path, run: &Run) -> Vec<Outcome> {
        let jobs: Vec<Job> = entries
            .iter()
            .map(|entry| Job {
                entry,
                out_dname,
                run,
            })
            .collect();
        rustyhgm_core::pool::map(&jobs, run.max_workers, |job| {
            let outcome = self.process_entry(job);
            match &outcome.error {
                None => println!(
                    "Successfully processed entry {} for eval 0",
                    outcome.instance_id
                ),
                Some(e) => println!(
                    "Failed to process entry {} for eval 0: {e}",
                    outcome.instance_id
                ),
            }
            outcome
        })
    }

    pub fn run_tasks<S: AsRef<str>>(
        &self,
        test_task_list: &[S],
        run: &Run,
        pred_dname: &Path,
        output_dir: &Path,
    ) -> Result<PathBuf, String> {
        let dataset: Vec<Value> = fs::read_to_string(&self.dataset)
            .map_err(|_| format!("Dataset file not found: {}", self.dataset.display()))
            .and_then(|t| serde_json::from_str(&t).map_err(|e| e.to_string()))?;
        fs::create_dir_all(pred_dname).map_err(|e| e.to_string())?;
        let wanted: HashSet<&str> = test_task_list.iter().map(AsRef::as_ref).collect();
        let entries: Vec<Value> = dataset
            .iter()
            .filter(|e| {
                e["instance_id"]
                    .as_str()
                    .is_some_and(|id| wanted.contains(id))
            })
            .cloned()
            .collect();
        let specs: Vec<_> = entries.iter().map(|e| self.spec(e)).collect();
        let images = Images {
            docker: &self.docker,
            root: &self.root,
            read_only_mounts: &[],
        };
        images.build_env_images(&specs, run.max_workers)?;
        let name = format!("{}_0", run.model_name_or_path);
        let out_dname = pred_dname.join(&name);
        fs::create_dir_all(&out_dname).map_err(|e| e.to_string())?;
        let inst = Run {
            model_name_or_path: name,
            ..run.clone()
        };
        println!("Starting evaluation 0 for model {}", run.model_name_or_path);
        let outcomes = self.evaluate(&entries, &out_dname, &inst);
        println!(
            "All evaluations completed for model {}",
            run.model_name_or_path
        );
        let report = report(dataset.len(), &outcomes);
        println!("{}", dumps(&report, None));
        let report_file = output_dir.join(format!(
            "{} {}.json",
            run.model_name_or_path.replace('/', "__"),
            Local::now().format("%Y%m%d_%H%M%S_%6f")
        ));
        write(&report_file, dumps(&report, Some(4)) + "\n")?;
        println!("Report written to {}", report_file.display());
        Ok(report_file)
    }
}

fn sorted_ids<'a>(outcomes: impl Iterator<Item = &'a Outcome>) -> Vec<String> {
    let mut ids: Vec<String> = outcomes.map(|o| o.instance_id.clone()).collect();
    ids.sort();
    ids
}

pub fn report(total_instances: usize, outcomes: &[Outcome]) -> PyValue {
    let completed = || outcomes.iter().filter(|o| o.success);
    let with = |result: &'static str| completed().filter(move |o| o.eval_result == result);
    let resolved = sorted_ids(with("resolved"));
    let unresolved = sorted_ids(with("unresolved"));
    let empty_patch = sorted_ids(with("empty_patch"));
    let error =
        sorted_ids(completed().filter(|o| {
            !["resolved", "unresolved", "empty_patch"].contains(&o.eval_result.as_str())
        }));
    let count = |n: usize| PyValue::Int(i64::try_from(n).unwrap_or(i64::MAX));
    let ids = |ids: &[String]| PyValue::list_of_str(ids);
    let none: [String; 0] = [];
    PyValue::dict([
        ("total_instances", count(total_instances)),
        ("submitted_instances", count(outcomes.len())),
        ("completed_instances", count(completed().count())),
        ("resolved_instances", count(resolved.len())),
        ("unresolved_instances", count(unresolved.len())),
        ("empty_patch_instances", count(empty_patch.len())),
        ("error_instances", count(error.len())),
        ("unstopped_instances", count(0)),
        ("completed_ids", ids(&sorted_ids(completed()))),
        (
            "incomplete_ids",
            ids(&sorted_ids(outcomes.iter().filter(|o| !o.success))),
        ),
        ("empty_patch_ids", ids(&empty_patch)),
        ("submitted_ids", ids(&sorted_ids(outcomes.iter()))),
        ("resolved_ids", ids(&resolved)),
        ("unresolved_ids", ids(&unresolved)),
        ("error_ids", ids(&error)),
        ("unstopped_containers", ids(&none)),
        ("unremoved_images", ids(&none)),
        ("schema_version", PyValue::Int(2)),
    ])
}

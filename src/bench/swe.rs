use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::Local;
use hgm_agent::testrepo::{TestRepo, get_test_description};
use hgm_core::log::Log;
use hgm_core::pyjson::{PyValue, dumps};
use hgm_sandbox::{Container, Docker, ExecOptions, RunOptions};
use serde_json::Value;

use crate::dataset::{self, Hub, SWE_BENCH};
use crate::swebench::{Swebench, TestSpec};

pub const AGENT_FILES: [(&str, &str); 10] = [
    ("coding_agent.py", "/hgm/coding_agent.py"),
    ("requirements.txt", "/hgm/requirements.txt"),
    ("pytest.ini", "/hgm/pytest.ini"),
    ("tools/", "/hgm/tools/"),
    ("utils/", "/hgm/utils/"),
    ("tests/", "/hgm/tests/"),
    ("prompts/", "/hgm/prompts/"),
    ("llm.py", "/hgm/llm.py"),
    ("llm_withtools.py", "/hgm/llm_withtools.py"),
    ("LICENSE.txt", "/hgm/LICENSE.txt"),
];

pub const AGENT_ENV: [&str; 7] = [
    "ANTHROPIC_API_KEY",
    "AWS_REGION",
    "AWS_REGION_NAME",
    "AWS_ACCESS_KEY_ID",
    "AWS_SECRET_ACCESS_KEY",
    "OPENAI_API_KEY",
    "OpenRouter_API_KEY",
];

#[derive(Clone, Debug)]
pub struct Harness {
    pub docker: Docker,
    pub swebench: Swebench,
    pub hub: Hub,
    pub llm: String,
    pub timeout: u64,
}

#[derive(Clone, Debug)]
pub struct Run {
    pub model_name_or_path: String,
    pub model_patch_paths: Vec<PathBuf>,
    pub init_agent_path: PathBuf,
    pub max_workers: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntryResult {
    pub instance_id: String,
    pub error: Option<String>,
}

struct Job<'a> {
    entry: &'a Value,
    spec: &'a TestSpec,
    out_dname: &'a Path,
    run: &'a Run,
}

fn text(entry: &Value, key: &str) -> String {
    entry[key].as_str().unwrap_or_default().to_owned()
}

pub fn prediction(
    model_name_or_path: &str,
    instance_id: &str,
    model_patch: &str,
    proposed: &[String],
) -> String {
    dumps(
        &PyValue::dict([
            ("instance_id", PyValue::str(instance_id)),
            ("model_name_or_path", PyValue::str(model_name_or_path)),
            ("model_patch", PyValue::str(model_patch)),
            ("proposed_model_patches", PyValue::list_of_str(proposed)),
        ]),
        Some(4),
    )
}

fn message(e: hgm_sandbox::Error) -> String {
    e.to_string()
}

fn run_logged(
    container: &Container,
    argv: &[&str],
    opts: &ExecOptions,
    log: &Log,
) -> Result<String, hgm_sandbox::Error> {
    container
        .exec(argv, opts)
        .and_then(|r| r.log(log, true).map(|()| r.text()))
}

enum Step {
    Info(&'static str),
    CopyIn(PathBuf, &'static str),
    Run(&'static [&'static str], &'static str),
}

fn run_step(container: &Container, step: &Step, log: &Log) -> Result<(), hgm_sandbox::Error> {
    match step {
        Step::Info(message) => {
            log.info(*message);
            Ok(())
        }
        Step::CopyIn(source, dest) => container.copy_in(source, dest, log),
        Step::Run(argv, workdir) => {
            run_logged(container, argv, &ExecOptions::workdir(*workdir), log).map(drop)
        }
    }
}

impl Harness {
    fn start(&self, job: &Job, log: &Log) -> Result<Container, String> {
        let run_id = Local::now().format("%Y%m%d_%H%M%S_%6f").to_string();
        let name = job.spec.container_name(&run_id);
        self.docker
            .remove_existing_container(&name, log)
            .map_err(message)?;
        let opts = RunOptions {
            user: Some(job.spec.user.clone()),
            platform: Some(job.spec.platform.clone()),
            nano_cpus: job.spec.nano_cpus,
            command: ["tail", "-f", "/dev/null"].map(str::to_owned).to_vec(),
        };
        self.docker
            .run_detached_as(&job.spec.image, &name, &opts, log)
            .map_err(message)
    }

    fn setup(&self, job: &Job, container: &Container, log: &Log) -> Result<(), String> {
        let eval_file = job
            .out_dname
            .join(format!("{}_eval.sh", job.spec.instance_id));
        fs::write(&eval_file, &job.spec.eval_script).map_err(|e| e.to_string())?;
        let mut steps: Vec<Step> = AGENT_FILES
            .iter()
            .map(|(source, dest)| Step::CopyIn(job.run.init_agent_path.join(source), dest))
            .collect();
        steps.extend([
            Step::Info("Setting up environment"),
            Step::CopyIn(eval_file, "/eval.sh"),
            Step::Run(&["/bin/bash", "/eval.sh"], "/"),
            Step::Run(&["rm", "/eval.sh"], "/"),
        ]);
        if !job.run.model_patch_paths.is_empty() {
            steps.push(Step::Info("Applying model patches"));
        }
        for patch in &job.run.model_patch_paths {
            steps.extend([
                Step::CopyIn(patch.clone(), "/hgm/parent_patch.txt"),
                Step::Run(
                    &["/bin/sh", "-c", "patch -p1 < /hgm/parent_patch.txt"],
                    "/hgm",
                ),
                Step::Run(&["rm", "/hgm/parent_patch.txt"], "/hgm"),
            ]);
        }
        steps.extend([
            Step::Info("Installing more requirements"),
            Step::Run(
                &[
                    "python",
                    "-m",
                    "pip",
                    "install",
                    "-r",
                    "/hgm/requirements.txt",
                ],
                "/",
            ),
        ]);
        steps
            .iter()
            .try_for_each(|step| run_step(container, step, log))
            .map_err(message)
    }

    fn solve(&self, job: &Job, container: &Container, log: &Log) -> Result<(), String> {
        let instance_id = &job.spec.instance_id;
        let chat_history = format!("/hgm/{instance_id}.md");
        let timeout = self.timeout.to_string();
        let problem_statement = text(job.entry, "problem_statement");
        let base_commit = text(job.entry, "base_commit");
        let test_description = get_test_description(&job.spec.eval_script, TestRepo::Swe);
        log.info("Running the agent");
        let argv = [
            "timeout",
            &timeout,
            "python",
            "/hgm/coding_agent.py",
            "--problem_statement",
            &problem_statement,
            "--git_dir",
            "/testbed/",
            "--chat_history_file",
            &chat_history,
            "--base_commit",
            &base_commit,
            "--outdir",
            "/hgm/",
            "--test_description",
            &test_description,
            "--instance_id",
            instance_id,
            "--model",
            &self.llm,
            "--timeout",
            &timeout,
        ];
        let env = ExecOptions::workdir("/").passthrough_env(&AGENT_ENV);
        run_logged(container, &argv, &env, log)
            .map(drop)
            .map_err(message)
    }

    fn collect(&self, job: &Job, container: &Container, log: &Log) -> Result<String, String> {
        let instance_id = &job.spec.instance_id;
        log.info("Copying output files back to host");
        let md_pattern = format!("{instance_id}_*.md");
        let argv = [
            "find",
            "/hgm/",
            "-name",
            &md_pattern,
            "-o",
            "-name",
            "model_patch_*.diff",
        ];
        let found =
            run_logged(container, &argv, &ExecOptions::workdir("/"), log).map_err(message)?;
        let (mds, diffs): (Vec<&str>, Vec<&str>) =
            found.split_whitespace().partition(|f| f.ends_with(".md"));
        let chat_history = format!("/hgm/{instance_id}.md");
        std::iter::once(chat_history.as_str())
            .chain(mds)
            .try_for_each(|file| {
                let name = file.rsplit('/').next().unwrap_or(file);
                container.copy_out(file, &job.out_dname.join(name), log)
            })
            .map_err(message)?;
        log.info("Getting model_patch");
        let none = ExecOptions::default();
        let mut patches = std::iter::once("/hgm/model_patch.diff")
            .chain(diffs)
            .map(|file| run_logged(container, &["cat", file], &none, log))
            .collect::<Result<Vec<String>, _>>()
            .map_err(message)?;
        let model_patch = patches.remove(0);
        Ok(prediction(
            &job.run.model_name_or_path,
            instance_id,
            &model_patch,
            &patches,
        ))
    }

    fn attempt(
        &self,
        job: &Job,
        slot: &mut Option<Container>,
        log: &Log,
    ) -> Result<String, String> {
        let container = slot.insert(self.start(job, log)?);
        self.setup(job, container, log)?;
        self.solve(job, container, log)?;
        self.collect(job, container, log)
    }

    fn process_entry(&self, job: &Job) -> EntryResult {
        let instance_id = job.spec.instance_id.clone();
        let out_fname = job.out_dname.join(format!("{instance_id}.json"));
        if out_fname.exists() {
            println!("Skipping existing entry {instance_id}");
            return EntryResult {
                instance_id,
                error: None,
            };
        }
        let log = Log::to_file(&job.out_dname.join(format!("{instance_id}_docker.log")))
            .unwrap_or_else(|_| Log::discard());
        let mut container = None;
        let outcome = self
            .attempt(job, &mut container, &log)
            .and_then(|result| fs::write(&out_fname, result).map_err(|e| e.to_string()));
        if let Some(c) = &container {
            c.cleanup(&log);
        }
        match outcome {
            Ok(()) => EntryResult {
                instance_id,
                error: None,
            },
            Err(e) => {
                println!("Error processing entry {instance_id}: {e}");
                let empty = prediction(&job.run.model_name_or_path, &instance_id, "", &[]);
                if let Err(write_error) = fs::write(&out_fname, empty) {
                    println!("Error writing empty result for {instance_id}: {write_error}");
                }
                EntryResult {
                    instance_id,
                    error: Some(e),
                }
            }
        }
    }

    pub fn evaluate(
        &self,
        entries: &[Value],
        specs: &[TestSpec],
        out_dname: &Path,
        run: &Run,
    ) -> Vec<EntryResult> {
        let jobs: Vec<Job> = entries
            .iter()
            .zip(specs)
            .map(|(entry, spec)| Job {
                entry,
                spec,
                out_dname,
                run,
            })
            .collect();
        hgm_core::pool::map(&jobs, run.max_workers, |job| {
            let result = self.process_entry(job);
            match &result.error {
                None => println!(
                    "Successfully processed entry {} for eval 0",
                    result.instance_id
                ),
                Some(e) => println!(
                    "Failed to process entry {} for eval 0: {e}",
                    result.instance_id
                ),
            }
            result
        })
    }

    pub fn harness<S: AsRef<str>>(
        &self,
        test_task_list: &[S],
        run: &Run,
        pred_dname: &Path,
    ) -> Result<Vec<PathBuf>, String> {
        let wanted: HashSet<&str> = test_task_list.iter().map(AsRef::as_ref).collect();
        let entries: Vec<Value> = self
            .hub
            .load(SWE_BENCH, "test")
            .map_err(|e| e.to_string())?
            .into_iter()
            .filter(|e| {
                e["instance_id"]
                    .as_str()
                    .is_some_and(|id| wanted.contains(id))
            })
            .collect();
        fs::create_dir_all(pred_dname).map_err(|e| e.to_string())?;
        let ids = dataset::instance_ids(&entries);
        self.swebench
            .prepare_images(SWE_BENCH, &ids, run.max_workers)
            .map_err(|e| e.to_string())?;
        let specs = self.swebench.specs(&entries).map_err(|e| e.to_string())?;
        let name = next_run_name(pred_dname, &run.model_name_or_path);
        let out_dname = pred_dname.join(&name);
        fs::create_dir_all(&out_dname).map_err(|e| e.to_string())?;
        let inst = Run {
            model_name_or_path: name,
            ..run.clone()
        };
        println!("Starting evaluation 0 for model {}", run.model_name_or_path);
        self.evaluate(&entries, &specs, &out_dname, &inst);
        println!(
            "All evaluations completed for model {}",
            run.model_name_or_path
        );
        Ok(vec![out_dname])
    }
}

pub fn next_run_name(pred_dname: &Path, model_name_or_path: &str) -> String {
    let taken = fs::read_dir(pred_dname)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|d| d.path().is_dir())
        .filter(|d| {
            d.file_name()
                .to_string_lossy()
                .starts_with(model_name_or_path)
        })
        .count();
    format!("{model_name_or_path}_{}", taken + 1)
}

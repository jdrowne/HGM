use std::fmt;
use std::io::{self, Write};
use std::path::Path;
use std::process::{Command, Stdio};

use serde::Deserialize;
use serde_json::Value;

const SPECS: &str = r#"
import json, sys
from swebench.harness.constants import MAP_REPO_VERSION_TO_SPECS
from swebench.harness.test_spec import make_test_spec
for line in sys.stdin:
    if not line.strip():
        continue
    spec = make_test_spec(json.loads(line))
    config = MAP_REPO_VERSION_TO_SPECS[spec.repo][spec.version]
    print(json.dumps({
        "instance_id": spec.instance_id,
        "eval_script": spec.eval_script,
        "image": spec.instance_image_key,
        "platform": spec.platform,
        "user": "nonroot" if config.get("execute_test_as_nonroot", False) else "root",
        "nano_cpus": config.get("nano_cpus"),
    }))
"#;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct TestSpec {
    pub instance_id: String,
    pub eval_script: String,
    pub image: String,
    pub platform: String,
    pub user: String,
    pub nano_cpus: Option<u64>,
}

impl TestSpec {
    pub fn container_name(&self, run_id: &str) -> String {
        format!("sweb.eval.{}.{run_id}", self.instance_id)
    }
}

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Failed { what: String, output: String },
    Output(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "running swebench: {e}"),
            Self::Failed { what, output } => write!(f, "swebench {what} failed: {}", output.trim()),
            Self::Output(e) => write!(f, "unexpected swebench output: {e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

#[derive(Clone, Debug)]
pub struct Swebench {
    pub python: String,
}

impl Swebench {
    pub fn specs(&self, entries: &[Value]) -> Result<Vec<TestSpec>, Error> {
        let mut input = String::new();
        for entry in entries {
            input.push_str(&entry.to_string());
            input.push('\n');
        }
        let out = Command::new(&self.python)
            .args(["-c", SPECS])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .and_then(|mut child| {
                let stdin = child.stdin.take();
                let writer =
                    std::thread::spawn(move || stdin.map(|mut s| s.write_all(input.as_bytes())));
                let out = child.wait_with_output();
                drop(writer.join());
                out
            })?;
        if !out.status.success() {
            return Err(Error::Failed {
                what: "test specs".to_owned(),
                output: String::from_utf8_lossy(&out.stderr).into_owned(),
            });
        }
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str(l).map_err(|e| Error::Output(format!("{e}: {l}"))))
            .collect()
    }

    fn module(
        &self,
        what: &str,
        module: &str,
        args: &[String],
        cwd: Option<&Path>,
    ) -> Result<(), Error> {
        let mut cmd = Command::new(&self.python);
        cmd.arg("-m").arg(module).args(args);
        if let Some(dir) = cwd {
            cmd.current_dir(dir);
        }
        let status = cmd.status()?;
        if status.success() {
            Ok(())
        } else {
            Err(Error::Failed {
                what: what.to_owned(),
                output: format!("{status}"),
            })
        }
    }

    pub fn prepare_images<S: AsRef<str>>(
        &self,
        dataset_name: &str,
        instance_ids: &[S],
        max_workers: usize,
    ) -> Result<(), Error> {
        if instance_ids.is_empty() {
            return Ok(());
        }
        let mut args = vec![
            "--dataset_name".to_owned(),
            dataset_name.to_owned(),
            "--split".to_owned(),
            "test".to_owned(),
            "--max_workers".to_owned(),
            max_workers.to_string(),
            "--instance_ids".to_owned(),
        ];
        args.extend(instance_ids.iter().map(|i| i.as_ref().to_owned()));
        self.module(
            "image build",
            "swebench.harness.prepare_images",
            &args,
            None,
        )
    }

    pub fn run_evaluation(&self, run: &EvaluationRun) -> Result<(), Error> {
        let args = [
            "--dataset_name".to_owned(),
            run.dataset_name.clone(),
            "--predictions_path".to_owned(),
            run.predictions_path.to_string_lossy().into_owned(),
            "--max_workers".to_owned(),
            run.max_workers.to_string(),
            "--run_id".to_owned(),
            run.run_id.clone(),
        ];
        self.module(
            "evaluation",
            "swebench.harness.run_evaluation",
            &args,
            Some(&run.output_dir),
        )
    }
}

#[derive(Clone, Debug)]
pub struct EvaluationRun {
    pub dataset_name: String,
    pub predictions_path: std::path::PathBuf,
    pub max_workers: usize,
    pub run_id: String,
    pub output_dir: std::path::PathBuf,
}

use std::ffi::OsStr;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use hgm_core::log::Log;

use crate::error::Error;

#[derive(Clone, Debug)]
pub struct Docker {
    program: String,
}

impl Default for Docker {
    fn default() -> Self {
        Docker {
            program: std::env::var("HGM_DOCKER").unwrap_or_else(|_| "docker".to_owned()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecResult {
    pub exit_code: i32,
    pub output: Vec<u8>,
}

impl ExecResult {
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.output).into_owned()
    }

    pub fn success(&self) -> bool {
        self.exit_code == 0
    }

    pub fn log(&self, log: &Log, raise_error: bool) -> Result<(), Error> {
        log.info(format!("Container output: {}", self.text()));
        if raise_error && self.exit_code != 0 {
            let e = Error::ExitCode(self.exit_code);
            log.error(&e);
            return Err(e);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default)]
pub struct ExecOptions {
    pub workdir: Option<String>,
    pub env: Vec<(String, String)>,
}

impl ExecOptions {
    pub fn workdir(dir: impl Into<String>) -> Self {
        ExecOptions {
            workdir: Some(dir.into()),
            env: Vec::new(),
        }
    }

    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }
}

pub struct BuildOptions<'a> {
    pub platform: Option<&'a str>,
    pub no_cache: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RunOptions {
    pub user: Option<String>,
    pub platform: Option<String>,
    pub nano_cpus: Option<u64>,
    pub read_only_mounts: Vec<(PathBuf, String)>,
    pub command: Vec<String>,
}

struct Output {
    code: Option<i32>,
    bytes: Vec<u8>,
    args: Vec<String>,
}

impl Output {
    fn into_error(self) -> Error {
        Error::Command {
            output: String::from_utf8_lossy(&self.bytes).into_owned(),
            args: self.args,
            code: self.code,
        }
    }
}

fn combined_output(cmd: Command) -> io::Result<(Option<i32>, Vec<u8>)> {
    let mut output = Vec::new();
    let status = io::pipe()
        .and_then(|(reader, writer)| writer.try_clone().map(|out| (reader, out, writer)))
        .and_then(|(mut reader, out, err)| {
            let mut cmd = cmd;
            let child = cmd.stdin(Stdio::null()).stdout(out).stderr(err).spawn();
            drop(cmd);
            child.and_then(|mut child| reader.read_to_end(&mut output).and_then(|_| child.wait()))
        })?;
    Ok((status.code(), output))
}

impl Docker {
    pub fn new(program: impl Into<String>) -> Self {
        Docker {
            program: program.into(),
        }
    }

    fn command<I, S>(&self, args: I) -> (Command, Vec<String>)
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut cmd = Command::new(&self.program);
        let mut shown = vec![self.program.clone()];
        for a in args {
            shown.push(a.as_ref().to_string_lossy().into_owned());
            cmd.arg(a);
        }
        (cmd, shown)
    }

    fn output<I, S>(&self, args: I) -> Result<Output, Error>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let (cmd, args) = self.command(args);
        let (code, bytes) = combined_output(cmd)?;
        Ok(Output { code, bytes, args })
    }

    fn run<I, S>(&self, args: I) -> Result<String, Error>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let out = self.output(args)?;
        if out.code == Some(0) {
            Ok(String::from_utf8_lossy(&out.bytes).into_owned())
        } else {
            Err(out.into_error())
        }
    }

    pub fn available(&self) -> bool {
        self.run(["version", "--format", "{{.Server.Version}}"])
            .is_ok()
    }

    pub fn image_exists(&self, image: &str) -> Result<bool, Error> {
        self.inspect_image(image, "{{.Id}}").map(|id| id.is_some())
    }

    pub fn inspect_image(&self, image: &str, format: &str) -> Result<Option<String>, Error> {
        let out = self.output(["image", "inspect", "--format", format, image])?;
        Ok((out.code == Some(0)).then(|| String::from_utf8_lossy(&out.bytes).trim().to_owned()))
    }

    pub fn images(&self) -> Result<Vec<(String, String)>, Error> {
        let out = self.run([
            "image",
            "ls",
            "--no-trunc",
            "--format",
            "{{.ID}} {{.Repository}}:{{.Tag}}",
        ])?;
        let mut images: Vec<(String, String)> = Vec::new();
        for line in out.lines() {
            let (id, tag) = line.split_once(' ').unwrap_or((line, "<none>"));
            if images.iter().any(|(seen, _)| seen == id) {
                continue;
            }
            let name = if tag.contains("<none>") { id } else { tag };
            images.push((id.to_owned(), name.to_owned()));
        }
        Ok(images)
    }

    pub fn history(&self, image: &str) -> Result<Vec<String>, Error> {
        let out = self.run(["history", "-q", "--no-trunc", image])?;
        Ok(out.lines().map(str::to_owned).collect())
    }

    pub fn container_exists(&self, name: &str) -> Result<bool, Error> {
        let out = self.output(["container", "inspect", "--format", "{{.Id}}", name])?;
        Ok(out.code == Some(0))
    }

    pub fn build(&self, context: &Path, tag: &str, no_cache: bool, log: &Log) -> Result<(), Error> {
        let opts = BuildOptions {
            platform: None,
            no_cache,
        };
        self.build_for(context, tag, &opts, log)
    }

    pub fn build_for(
        &self,
        context: &Path,
        tag: &str,
        opts: &BuildOptions,
        log: &Log,
    ) -> Result<(), Error> {
        let mut args: Vec<&OsStr> = vec!["build".as_ref(), "-t".as_ref(), tag.as_ref()];
        if let Some(platform) = opts.platform {
            args.extend([OsStr::new("--platform"), OsStr::new(platform)]);
        }
        if opts.no_cache {
            args.push("--no-cache".as_ref());
        }
        args.push(context.as_os_str());
        log.info("Building the Docker image...");
        let out = self.output(args)?;
        for line in String::from_utf8_lossy(&out.bytes).lines() {
            log.info(line.trim());
        }
        if out.code != Some(0) {
            let e = out.into_error();
            log.error(format!("Error while building the Docker image: {e}"));
            return Err(e);
        }
        log.info("Image built successfully.");
        Ok(())
    }

    pub fn ensure_image(
        &self,
        context: &Path,
        tag: &str,
        force_rebuild: bool,
        log: &Log,
    ) -> Result<(), Error> {
        if !force_rebuild && self.image_exists(tag)? {
            log.info(format!(
                "Docker image '{tag}' already exists. Skipping build."
            ));
            return Ok(());
        }
        self.build(context, tag, false, log)
    }

    pub fn remove_image(&self, image: &str) -> Result<(), Error> {
        self.run(["image", "rm", "-f", image]).map(|_| ())
    }

    pub fn image_tags(&self) -> Result<Vec<String>, Error> {
        let out = self.run(["image", "ls", "--format", "{{.Repository}}:{{.Tag}}"])?;
        Ok(out
            .lines()
            .filter(|l| !l.contains("<none>"))
            .map(str::to_owned)
            .collect())
    }

    pub fn run_detached(&self, image: &str, name: &str, log: &Log) -> Result<Container, Error> {
        self.run_detached_with(image, name, &[], log)
    }

    pub fn run_detached_with(
        &self,
        image: &str,
        name: &str,
        command: &[&str],
        log: &Log,
    ) -> Result<Container, Error> {
        let mut args = vec!["run", "-d", "--name", name, image];
        args.extend_from_slice(command);
        match self.run(args) {
            Ok(_) => {
                log.info(format!("Container '{name}' started successfully."));
                Ok(self.container(name))
            }
            Err(e) => {
                log.error(format!("Error while starting the container: {e}"));
                Err(e)
            }
        }
    }

    pub fn run_detached_as(
        &self,
        image: &str,
        name: &str,
        opts: &RunOptions,
        log: &Log,
    ) -> Result<Container, Error> {
        let mut args = vec![
            "run".to_owned(),
            "-d".to_owned(),
            "--name".to_owned(),
            name.to_owned(),
        ];
        if let Some(user) = &opts.user {
            args.extend(["--user".to_owned(), user.clone()]);
        }
        if let Some(platform) = &opts.platform {
            args.extend(["--platform".to_owned(), platform.clone()]);
        }
        if let Some(nano) = opts.nano_cpus {
            args.extend(["--cpus".to_owned(), format!("{}", nano as f64 / 1e9)]);
        }
        for (source, target) in &opts.read_only_mounts {
            args.extend([
                "--mount".to_owned(),
                format!(
                    "type=bind,source={},target={target},readonly",
                    source.display()
                ),
            ]);
        }
        args.push(image.to_owned());
        args.extend(opts.command.iter().cloned());
        match self.run(&args) {
            Ok(_) => {
                log.info(format!("Container for {name} created and started."));
                Ok(self.container(name))
            }
            Err(e) => {
                log.error(format!("Error creating container {name}: {e}"));
                Err(e)
            }
        }
    }

    pub fn container(&self, name: &str) -> Container {
        Container {
            docker: self.clone(),
            name: name.to_owned(),
        }
    }

    pub fn remove_existing_container(&self, name: &str, log: &Log) -> Result<(), Error> {
        if !self.container_exists(name)? {
            log.info(format!("No existing container with name {name} found."));
            return Ok(());
        }
        log.info(format!("Removing existing container with name {name}"));
        self.container(name).remove().inspect_err(|e| {
            log.error(format!("Error removing existing container {name}: {e}"));
        })
    }
}

#[derive(Clone, Debug)]
pub struct Container {
    docker: Docker,
    name: String,
}

impl Container {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn exec(&self, argv: &[&str], opts: &ExecOptions) -> Result<ExecResult, Error> {
        let mut args: Vec<String> = vec!["exec".into()];
        if let Some(dir) = &opts.workdir {
            args.push("-w".into());
            args.push(dir.clone());
        }
        for (k, v) in &opts.env {
            args.push("-e".into());
            args.push(format!("{k}={v}"));
        }
        args.push(self.name.clone());
        args.extend(argv.iter().map(|s| (*s).to_owned()));
        let out = self.docker.output(&args)?;
        match out.code {
            Some(exit_code) => Ok(ExecResult {
                exit_code,
                output: out.bytes,
            }),
            None => Err(out.into_error()),
        }
    }

    pub fn exec_str(&self, cmd: &str, opts: &ExecOptions) -> Result<ExecResult, Error> {
        let argv = shlex::split(cmd).ok_or_else(|| Error::Quoting(cmd.to_owned()))?;
        let argv: Vec<&str> = argv.iter().map(String::as_str).collect();
        self.exec(&argv, opts)
    }

    pub fn sh(&self, script: &str, opts: &ExecOptions) -> Result<ExecResult, Error> {
        self.exec(&["/bin/sh", "-c", script], opts)
    }

    pub fn copy_in(&self, source: &Path, dest: &str, log: &Log) -> Result<(), Error> {
        let result = self.copy_in_inner(source, dest, log);
        if let Err(e) = &result {
            log.error(format!("Error copying to container: {e}"));
        }
        result
    }

    fn copy_in_inner(&self, source: &Path, dest: &str, log: &Log) -> Result<(), Error> {
        let meta = fs::metadata(source)
            .map_err(|_| Error::NotFound(format!("Source path {}", source.display())))?;
        let dest = dest.trim_end_matches('/');
        let dest = if dest.is_empty() { "/" } else { dest };
        let (mkdir, from) = if meta.is_dir() {
            (dest.to_owned(), source.join("."))
        } else {
            let parent = match dest.rsplit_once('/') {
                Some((p, _)) if !p.is_empty() => p.to_owned(),
                Some(_) => "/".to_owned(),
                None => ".".to_owned(),
            };
            (parent, source.to_path_buf())
        };
        self.exec(&["mkdir", "-p", &mkdir], &ExecOptions::default())?;
        log.info(format!(
            "Copying {} to container at {dest}",
            source.display()
        ));
        let target = format!("{}:{dest}", self.name);
        self.docker
            .run([OsStr::new("cp"), from.as_os_str(), OsStr::new(&target)])?;
        log.info(format!(
            "Successfully copied {} to container",
            source.display()
        ));
        Ok(())
    }

    pub fn copy_out(&self, source: &str, dest: &Path, log: &Log) -> Result<(), Error> {
        let result = self.copy_out_inner(source, dest, log);
        if let Err(e) = &result {
            log.error(format!("Error copying from container: {e}"));
        }
        result
    }

    fn copy_out_inner(&self, source: &str, dest: &Path, log: &Log) -> Result<(), Error> {
        let probe = self.exec(&["test", "-e", source], &ExecOptions::default())?;
        if !probe.success() {
            return Err(Error::NotFound(format!(
                "Source path {source} in container"
            )));
        }
        if let Some(parent) = dest.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::create_dir_all(parent)?;
        }
        log.info(format!(
            "Copying from container {source} to local path {}",
            dest.display()
        ));
        let from = format!("{}:{source}", self.name);
        self.docker
            .run([OsStr::new("cp"), OsStr::new(&from), dest.as_os_str()])?;
        log.info(format!(
            "Successfully copied from container to {}",
            dest.display()
        ));
        Ok(())
    }

    pub fn read_file(&self, path: &str) -> Result<Vec<u8>, Error> {
        let r = self.exec(&["cat", path], &ExecOptions::default())?;
        if r.success() {
            Ok(r.output)
        } else {
            Err(Error::NotFound(format!("{path} in container")))
        }
    }

    pub fn stop(&self) -> Result<(), Error> {
        self.stop_within(10)
    }

    pub fn stop_within(&self, seconds: u32) -> Result<(), Error> {
        self.docker
            .run(["stop", "-t", &seconds.to_string(), &self.name])
            .map(|_| ())
    }

    pub fn remove(&self) -> Result<(), Error> {
        self.docker.run(["rm", "-f", &self.name]).map(|_| ())
    }

    pub fn cleanup(&self, log: &Log) {
        self.cleanup_within(10, log);
    }

    pub fn cleanup_within(&self, seconds: u32, log: &Log) {
        log.info(format!("Stopping container '{}'...", self.name));
        if let Err(e) = self.stop_within(seconds) {
            log.error(format!("Failed to stop container {}: {e}", self.name));
        }
        match self.remove() {
            Ok(()) => log.info(format!("Container '{}' removed.", self.name)),
            Err(e) => log.error(format!("Failed to remove container {}: {e}", self.name)),
        }
    }
}

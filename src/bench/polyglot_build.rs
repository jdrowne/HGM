use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use rustyhgm_core::log::Log;
use rustyhgm_sandbox::{BuildOptions, Container, Docker, RunOptions};

use crate::polyglot_spec::TestSpec;

pub const BASE_IMAGE_BUILD_DIR: &str = "logs/build_images/base";
pub const ENV_IMAGE_BUILD_DIR: &str = "logs/build_images/env";
pub const INSTANCE_IMAGE_BUILD_DIR: &str = "logs/build_images/instances";

fn message(e: impl std::fmt::Display) -> String {
    e.to_string()
}

pub fn copy_tree(from: &Path, to: &Path) -> io::Result<()> {
    fs::create_dir_all(to)
        .and_then(|()| fs::read_dir(from))
        .and_then(|entries| {
            entries.into_iter().try_for_each(|entry| {
                entry.and_then(|entry| {
                    let path = entry.path();
                    let target = to.join(entry.file_name());
                    if path.is_dir() {
                        copy_tree(&path, &target)
                    } else {
                        fs::copy(&path, &target).map(drop)
                    }
                })
            })
        })
}

pub fn build_dir(root: &Path, kind: &str, image: &str) -> PathBuf {
    root.join(kind).join(image.replace(':', "__"))
}

pub struct ImageBuild<'a> {
    pub name: &'a str,
    pub setup_scripts: Vec<(&'static str, String)>,
    pub dockerfile: String,
    pub platform: &'a str,
    pub build_dir: PathBuf,
    pub repo: Option<(PathBuf, &'a str)>,
    pub nocache: bool,
}

fn stage(build: &ImageBuild, log: &Log) -> io::Result<()> {
    if let Some((source, rel)) = &build.repo
        && source.is_dir()
    {
        let target = build.build_dir.join(rel);
        copy_tree(source, &target)?;
        log.info(format!(
            "Copied folder {} to {}",
            source.display(),
            target.display()
        ));
    }
    build
        .setup_scripts
        .iter()
        .try_for_each(|(name, script)| fs::write(build.build_dir.join(name), script))
        .and_then(|()| fs::write(build.build_dir.join("Dockerfile"), &build.dockerfile))
}

pub fn build_image(docker: &Docker, build: &ImageBuild) -> Result<(), String> {
    let log = fs::create_dir_all(&build.build_dir)
        .and_then(|()| Log::to_file(&build.build_dir.join("build_image.log")))
        .map_err(message)?;
    log.info(format!(
        "Building image {}\nUsing dockerfile:\n{}\nAdding ({}) setup scripts to image build repo",
        build.name,
        build.dockerfile,
        build.setup_scripts.len()
    ));
    for (name, script) in &build.setup_scripts {
        log.info(format!("[SETUP SCRIPT] {name}:\n{script}"));
    }
    let built = stage(build, &log).map_err(message).and_then(|()| {
        log.info(format!(
            "Building docker image {} in {} with platform {}",
            build.name,
            build.build_dir.display(),
            build.platform
        ));
        let opts = BuildOptions {
            platform: Some(build.platform),
            no_cache: build.nocache,
        };
        docker
            .build_for(&build.build_dir, build.name, &opts, &log)
            .map_err(message)
    });
    match built {
        Ok(()) => {
            log.info("Image built successfully!");
            Ok(())
        }
        Err(e) => {
            log.error(format!("Error building image {}: {e}", build.name));
            Err(format!("Error building image {}: {e}\n", build.name))
        }
    }
}

pub fn remove_image_quietly(docker: &Docker, image: &str) -> Result<(), String> {
    if docker.image_exists(image).map_err(message)? {
        docker.remove_image(image).map_err(message)?;
    }
    Ok(())
}

fn derives_from(docker: &Docker, id: &str, base_id: &str) -> Result<bool, String> {
    match docker.history(id) {
        Ok(layers) => Ok(layers.iter().any(|layer| layer == base_id)),
        Err(_) if matches!(docker.image_exists(id), Ok(false)) => Ok(false),
        Err(e) => Err(message(e)),
    }
}

pub fn find_dependent_images(docker: &Docker, image: &str) -> Result<Vec<String>, String> {
    let Some(base_id) = docker.inspect_image(image, "{{.Id}}").map_err(message)? else {
        println!("Base image {image} not found.");
        return Ok(Vec::new());
    };
    let mut dependents = Vec::new();
    for (id, name) in docker.images().map_err(message)? {
        if id != base_id && derives_from(docker, &id, &base_id)? {
            dependents.push(name);
        }
    }
    Ok(dependents)
}

fn created(docker: &Docker, image: &str) -> Result<Option<String>, String> {
    docker.inspect_image(image, "{{.Created}}").map_err(message)
}

fn unique<T>(items: impl IntoIterator<Item = (String, T)>) -> Vec<(String, T)> {
    let mut out: Vec<(String, T)> = Vec::new();
    for (key, value) in items {
        if !out.iter().any(|(seen, _)| *seen == key) {
            out.push((key, value));
        }
    }
    out
}

pub struct Images<'a> {
    pub docker: &'a Docker,
    pub root: &'a Path,
    pub read_only_mounts: &'a [(PathBuf, String)],
}

impl Images<'_> {
    pub fn build_base_images(&self, specs: &[TestSpec]) -> Result<(), String> {
        let bases = unique(specs.iter().map(|s| (s.base_image_key(), s)));
        for (image, spec) in bases {
            if self.docker.image_exists(&image).map_err(message)? {
                println!("Base image {image} already exists, skipping build.");
                continue;
            }
            println!("Building base image ({image})");
            build_image(
                self.docker,
                &ImageBuild {
                    name: &image,
                    setup_scripts: Vec::new(),
                    dockerfile: spec.base_dockerfile(),
                    platform: spec.platform(),
                    build_dir: build_dir(self.root, BASE_IMAGE_BUILD_DIR, &image),
                    repo: None,
                    nocache: false,
                },
            )?;
        }
        println!("Base images built successfully.");
        Ok(())
    }

    pub fn env_images_to_build<'s>(
        &self,
        specs: &'s [TestSpec],
    ) -> Result<Vec<(String, &'s TestSpec)>, String> {
        let mut needed = Vec::new();
        for spec in specs {
            let base = spec.base_image_key();
            let env = spec.env_image_key();
            let Some(base_created) = created(self.docker, &base)? else {
                return Err(format!(
                    "Base image {base} not found for {env}\n.Please build the base images first."
                ));
            };
            let stale = match created(self.docker, &env)? {
                None => true,
                Some(env_created) if env_created < base_created => {
                    for dependent in find_dependent_images(self.docker, &env)? {
                        remove_image_quietly(self.docker, &dependent)?;
                    }
                    remove_image_quietly(self.docker, &env)?;
                    true
                }
                Some(_) => false,
            };
            if stale {
                needed.push((env, spec));
            }
        }
        Ok(unique(needed))
    }

    pub fn build_env_images(
        &self,
        specs: &[TestSpec],
        max_workers: usize,
    ) -> Result<(Vec<String>, Vec<String>), String> {
        self.build_base_images(specs)?;
        let configs = self.env_images_to_build(specs)?;
        if configs.is_empty() {
            println!("No environment images need to be built.");
            return Ok((Vec::new(), Vec::new()));
        }
        println!("Total environment images to build: {}", configs.len());
        let results = rustyhgm_core::pool::map(&configs, max_workers, |(image, spec)| {
            build_image(
                self.docker,
                &ImageBuild {
                    name: image,
                    setup_scripts: vec![("setup_env.sh", spec.setup_env_script())],
                    dockerfile: spec.env_dockerfile(),
                    platform: spec.platform(),
                    build_dir: build_dir(self.root, ENV_IMAGE_BUILD_DIR, image),
                    repo: None,
                    nocache: false,
                },
            )
        });
        let (mut successful, mut failed) = (Vec::new(), Vec::new());
        for ((image, _), result) in configs.iter().zip(results) {
            match result {
                Ok(()) => successful.push(image.clone()),
                Err(e) => {
                    println!("BuildImageError {image}\n{e}");
                    failed.push(image.clone());
                }
            }
        }
        if failed.is_empty() {
            println!("All environment images built successfully.");
        } else {
            println!("{} environment images failed to build.", failed.len());
        }
        Ok((successful, failed))
    }

    pub fn build_instance_image(
        &self,
        spec: &TestSpec,
        log: &Log,
        nocache: bool,
    ) -> Result<(), String> {
        let image = spec.instance_image_key();
        let env = spec.env_image_key();
        let id = &spec.instance_id;
        let Some(env_created) = created(self.docker, &env)? else {
            return Err(format!(
                "Error building image {id}: Environment image {env} not found for {id}\n"
            ));
        };
        log.info(format!(
            "Environment image {env} found for {id}\nBuilding instance image {image} for {id}"
        ));
        let exists = match created(self.docker, &image)? {
            Some(instance_created) if instance_created < env_created => {
                remove_image_quietly(self.docker, &image)?;
                false
            }
            found => found.is_some(),
        };
        if exists {
            log.info(format!("Image {image} already exists, skipping build."));
            return Ok(());
        }
        build_image(
            self.docker,
            &ImageBuild {
                name: &image,
                setup_scripts: vec![("setup_repo.sh", spec.install_repo_script())],
                dockerfile: spec.instance_dockerfile(),
                platform: spec.platform(),
                build_dir: build_dir(self.root, INSTANCE_IMAGE_BUILD_DIR, &image),
                repo: Some((self.root.join(&spec.repo), &spec.repo)),
                nocache,
            },
        )
    }

    pub fn build_container(
        &self,
        spec: &TestSpec,
        run_id: &str,
        log: &Log,
        nocache: bool,
    ) -> Result<Container, String> {
        self.build_instance_image(spec, log, nocache)?;
        let id = &spec.instance_id;
        log.info(format!("Creating container for {id}..."));
        let opts = RunOptions {
            user: Some("root".to_owned()),
            platform: Some(spec.platform().to_owned()),
            nano_cpus: None,
            read_only_mounts: self.read_only_mounts.to_vec(),
            command: ["tail", "-f", "/dev/null"].map(str::to_owned).to_vec(),
        };
        self.docker
            .run_detached_as(
                &spec.instance_image_key(),
                &spec.container_name(run_id),
                &opts,
                log,
            )
            .map_err(|e| {
                log.error(format!("Error creating container for {id}: {e}"));
                format!("Error building image {id}: {e}\n")
            })
    }
}

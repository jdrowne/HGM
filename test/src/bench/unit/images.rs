use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use hgm_bench::polyglot_build::{
    ENV_IMAGE_BUILD_DIR, INSTANCE_IMAGE_BUILD_DIR, ImageBuild, Images, build_dir, build_image,
    find_dependent_images, remove_image_quietly,
};
use hgm_bench::polyglot_spec::{TestSpec, make_test_spec};
use hgm_core::log::Log;
use hgm_sandbox::Docker;
use serde_json::json;

const FAKE: &str = r#"#!/bin/sh
state=$(dirname "$0")/state
echo "$*" >> "$state/calls"
case "$1 $2" in
  "image inspect")
    case "$5" in "$(cat "$state/vanish" 2>/dev/null)") rm -f "$0" ;; esac
    f="$state/images/$5"
    [ -f "$f" ] || exit 1
    case "$4" in
      *Created*) sed -n 2p "$f" ;;
      *) sed -n 1p "$f" ;;
    esac ;;
  "image rm")
    [ -e "$state/rmfail" ] && { echo cannot remove; exit 1; }
    rm -f "$state/images/$4" ;;
  "image ls")
    [ -e "$state/lsfail" ] && exit 1
    cat "$state/ls" 2>/dev/null ;;
  "history -q")
    f="$state/history/$(echo "$4" | tr : _)"
    [ -f "$f" ] || exit 1
    cat "$f" ;;
  "build -t")
    [ -e "$state/buildfail" ] && { echo build exploded; exit 1; }
    printf 'sha256:built-%s\n9999\n' "$3" > "$state/images/$3"
    echo step one ;;
  "run -d")
    [ -e "$state/runfail" ] && { echo no such image; exit 125; }
    echo cid ;;
  *) exit 0 ;;
esac
"#;

struct Fake {
    _tmp: tempfile::TempDir,
    root: PathBuf,
    state: PathBuf,
    docker: Docker,
}

impl Fake {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("root");
        let state = tmp.path().join("state");
        fs::create_dir_all(state.join("images")).unwrap();
        fs::create_dir_all(state.join("history")).unwrap();
        fs::create_dir_all(&root).unwrap();
        let program = tmp.path().join("docker");
        fs::write(&program, FAKE).unwrap();
        fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
        Fake {
            docker: Docker::new(program.to_string_lossy()),
            _tmp: tmp,
            root,
            state,
        }
    }

    fn image(&self, name: &str, id: &str, created: &str) {
        fs::write(
            self.state.join("images").join(name),
            format!("{id}\n{created}\n"),
        )
        .unwrap();
    }

    fn has(&self, name: &str) -> bool {
        self.state.join("images").join(name).exists()
    }

    fn flag(&self, name: &str) {
        fs::write(self.state.join(name), "").unwrap();
    }

    fn vanish_after(&self, image: &str) {
        fs::write(self.state.join("vanish"), image).unwrap();
    }

    fn unflag(&self, name: &str) {
        fs::remove_file(self.state.join(name)).unwrap();
    }

    fn calls(&self) -> String {
        fs::read_to_string(self.state.join("calls")).unwrap_or_default()
    }

    fn images(&self) -> Images<'_> {
        Images {
            docker: &self.docker,
            root: &self.root,
        }
    }
}

fn spec(id: &str, language: &str) -> TestSpec {
    make_test_spec(
        &json!({
            "instance_id": id,
            "language": language,
            "repo": format!("polyglot/polyglot-benchmark/{language}/exercises/practice/{id}"),
            "base_commit": "c",
            "test_patch": "",
        }),
        "x86_64",
    )
}

fn plain_build<'a>(dir: &Path, repo: Option<(PathBuf, &'a str)>) -> ImageBuild<'a> {
    ImageBuild {
        name: "img:latest",
        setup_scripts: vec![("setup_env.sh", "echo hi\n".to_owned())],
        dockerfile: "FROM scratch\nCOPY setup_env.sh /\n".to_owned(),
        platform: "linux/x86_64",
        build_dir: dir.to_path_buf(),
        repo,
        nocache: true,
    }
}

#[test]
fn an_image_build_stages_its_context_and_logs_the_output() {
    let fake = Fake::new();
    let source = fake.root.join("task");
    fs::create_dir_all(source.join(".git/objects")).unwrap();
    fs::write(source.join(".git/objects/o"), "object").unwrap();
    fs::write(source.join("a.py"), "x").unwrap();
    let dir = fake.root.join("build");
    build_image(
        &fake.docker,
        &plain_build(&dir, Some((source, "polyglot/task"))),
    )
    .unwrap();
    assert_eq!(
        fs::read_to_string(dir.join("setup_env.sh")).unwrap(),
        "echo hi\n"
    );
    assert!(
        fs::read_to_string(dir.join("Dockerfile"))
            .unwrap()
            .starts_with("FROM scratch")
    );
    assert_eq!(
        fs::read_to_string(dir.join("polyglot/task/.git/objects/o")).unwrap(),
        "object"
    );
    let log = fs::read_to_string(dir.join("build_image.log")).unwrap();
    for line in [
        "Building image img:latest\nUsing dockerfile:\nFROM scratch",
        "Adding (1) setup scripts to image build repo",
        "[SETUP SCRIPT] setup_env.sh:\necho hi",
        "Copied folder",
        "with platform linux/x86_64",
        "step one",
        "Image built successfully!",
    ] {
        assert!(log.contains(line), "{line}\n{log}");
    }
    assert!(
        fake.calls()
            .contains("build -t img:latest --platform linux/x86_64 --no-cache"),
        "{}",
        fake.calls()
    );
    let missing = fake.root.join("missing");
    build_image(
        &fake.docker,
        &plain_build(&dir, Some((missing, "polyglot/gone"))),
    )
    .unwrap();
    assert!(!dir.join("polyglot/gone").exists());
}

#[test]
fn every_failing_build_stage_is_an_image_error() {
    let fake = Fake::new();
    let blocker = fake.root.join("file");
    fs::write(&blocker, "").unwrap();
    let e = build_image(&fake.docker, &plain_build(&blocker.join("dir"), None)).unwrap_err();
    assert!(!e.starts_with("Error building image"), "{e}");
    let source = fake.root.join("task");
    fs::create_dir_all(&source).unwrap();
    let dir = fake.root.join("build");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("polyglot"), "").unwrap();
    let e = build_image(
        &fake.docker,
        &plain_build(&dir, Some((source.clone(), "polyglot/t"))),
    )
    .unwrap_err();
    assert!(e.starts_with("Error building image img:latest: "), "{e}");
    assert!(e.ends_with('\n'));
    for blocked in ["setup_env.sh", "Dockerfile"] {
        let dir = fake.root.join(format!("blocked-{blocked}"));
        fs::create_dir_all(dir.join(blocked)).unwrap();
        assert!(build_image(&fake.docker, &plain_build(&dir, None)).is_err());
    }
    let unreadable = fake.root.join("unreadable");
    fs::create_dir_all(unreadable.join("inner")).unwrap();
    fs::set_permissions(unreadable.join("inner"), fs::Permissions::from_mode(0o000)).unwrap();
    let dir = fake.root.join("build-unreadable");
    let result = build_image(
        &fake.docker,
        &plain_build(&dir, Some((unreadable.clone(), "u"))),
    );
    fs::set_permissions(unreadable.join("inner"), fs::Permissions::from_mode(0o755)).unwrap();
    assert!(result.is_err());
    fake.flag("buildfail");
    let dir = fake.root.join("build-fails");
    let e = build_image(&fake.docker, &plain_build(&dir, None)).unwrap_err();
    assert!(e.contains("build exploded"), "{e}");
    let log = fs::read_to_string(dir.join("build_image.log")).unwrap();
    assert!(
        log.contains("ERROR - Error building image img:latest"),
        "{log}"
    );
}

#[test]
fn quiet_removal_skips_missing_images_and_reports_failures() {
    let fake = Fake::new();
    remove_image_quietly(&fake.docker, "absent:latest").unwrap();
    assert!(!fake.calls().contains("image rm"));
    fake.image("present:latest", "sha256:p", "1");
    remove_image_quietly(&fake.docker, "present:latest").unwrap();
    assert!(!fake.has("present:latest"));
    fake.image("stuck:latest", "sha256:s", "1");
    fake.flag("rmfail");
    assert!(remove_image_quietly(&fake.docker, "stuck:latest").is_err());
    let gone = Docker::new("hgm-no-such-docker-program");
    assert!(remove_image_quietly(&gone, "x").is_err());
}

#[test]
fn dependents_are_images_whose_history_holds_the_base() {
    let fake = Fake::new();
    assert_eq!(
        find_dependent_images(&fake.docker, "env:latest").unwrap(),
        Vec::<String>::new()
    );
    fake.image("env:latest", "sha256:env", "1");
    fs::write(
        fake.state.join("ls"),
        "sha256:env env:latest\nsha256:child child:latest\nsha256:child child:other\nsha256:bare <none>:<none>\nsha256:solo\nsha256:other other:latest\n",
    )
    .unwrap();
    let history = fake.state.join("history");
    fs::write(history.join("sha256_child"), "sha256:child\nsha256:env\n").unwrap();
    fs::write(history.join("sha256_bare"), "sha256:bare\nsha256:env\n").unwrap();
    fs::write(history.join("sha256_solo"), "sha256:env\n").unwrap();
    fs::write(history.join("sha256_other"), "sha256:other\n").unwrap();
    assert_eq!(
        find_dependent_images(&fake.docker, "env:latest").unwrap(),
        ["child:latest", "sha256:bare", "sha256:solo"]
    );
    fs::remove_file(history.join("sha256_other")).unwrap();
    assert_eq!(
        find_dependent_images(&fake.docker, "env:latest").unwrap(),
        ["child:latest", "sha256:bare", "sha256:solo"]
    );
    fake.image("sha256:other", "sha256:other", "1");
    assert!(find_dependent_images(&fake.docker, "env:latest").is_err());
    fake.flag("lsfail");
    assert!(find_dependent_images(&fake.docker, "env:latest").is_err());
    let gone = Docker::new("hgm-no-such-docker-program");
    assert!(find_dependent_images(&gone, "env:latest").is_err());
}

#[test]
fn base_images_are_built_once_and_only_when_missing() {
    let fake = Fake::new();
    let specs = [spec("a", "go"), spec("b", "rust")];
    fake.images().build_base_images(&specs).unwrap();
    assert!(fake.has("pb.base.x86_64:latest"));
    assert_eq!(fake.calls().matches("build -t").count(), 1);
    let dir = build_dir(
        &fake.root,
        "logs/build_images/base",
        "pb.base.x86_64:latest",
    );
    assert!(dir.ends_with("logs/build_images/base/pb.base.x86_64__latest"));
    assert!(
        fs::read_to_string(dir.join("Dockerfile"))
            .unwrap()
            .contains("buildpack-deps")
    );
    fake.images().build_base_images(&specs).unwrap();
    assert_eq!(fake.calls().matches("build -t").count(), 1);
    let fresh = Fake::new();
    fresh.flag("buildfail");
    assert!(fresh.images().build_base_images(&specs).is_err());
    let gone = Docker::new("hgm-no-such-docker-program");
    let images = Images {
        docker: &gone,
        root: &fake.root,
    };
    assert!(images.build_base_images(&specs).is_err());
}

#[test]
fn env_images_are_rebuilt_when_missing_or_older_than_their_base() {
    let fake = Fake::new();
    let python = spec("p", "python");
    let go = spec("g", "go");
    let rust = spec("r", "rust");
    let specs = [python.clone(), go.clone(), rust.clone()];
    let e = fake.images().env_images_to_build(&specs).unwrap_err();
    assert_eq!(
        e,
        format!(
            "Base image pb.base.x86_64:latest not found for {}\n.Please build the base images first.",
            python.env_image_key()
        )
    );
    fake.image("pb.base.x86_64:latest", "sha256:base", "2024-02");
    let needed: Vec<String> = fake
        .images()
        .env_images_to_build(&specs)
        .unwrap()
        .into_iter()
        .map(|(key, _)| key)
        .collect();
    assert_eq!(needed, [python.env_image_key(), go.env_image_key()]);
    fake.image(&python.env_image_key(), "sha256:penv", "2024-03");
    fake.image(&go.env_image_key(), "sha256:genv", "2024-01");
    fake.image("pb.eval.x86_64.g:latest", "sha256:geval", "2024-01");
    fs::write(
        fake.state.join("ls"),
        "sha256:geval pb.eval.x86_64.g:latest\n",
    )
    .unwrap();
    fs::write(fake.state.join("history/sha256_geval"), "sha256:genv\n").unwrap();
    let needed = fake.images().env_images_to_build(&specs).unwrap();
    assert_eq!(needed.len(), 1);
    assert_eq!(needed[0].0, go.env_image_key());
    assert!(!fake.has("pb.eval.x86_64.g:latest"));
    assert!(!fake.has(&go.env_image_key()));
    fake.image(&go.env_image_key(), "sha256:genv", "2024-01");
    fake.flag("rmfail");
    assert!(fake.images().env_images_to_build(&specs).is_err());
    fake.image("pb.eval.x86_64.g:latest", "sha256:geval", "2024-01");
    assert!(fake.images().env_images_to_build(&specs).is_err());
    fake.unflag("rmfail");
    fake.flag("lsfail");
    assert!(fake.images().env_images_to_build(&specs).is_err());
    let gone = Docker::new("hgm-no-such-docker-program");
    let images = Images {
        docker: &gone,
        root: &fake.root,
    };
    assert!(images.env_images_to_build(&specs).is_err());
    let vanishing = Fake::new();
    vanishing.image("pb.base.x86_64:latest", "sha256:base", "2024-02");
    vanishing.vanish_after("pb.base.x86_64:latest");
    assert!(vanishing.images().env_images_to_build(&specs).is_err());
}

#[test]
fn env_image_builds_report_successes_and_failures() {
    let fake = Fake::new();
    let specs = [spec("p", "python"), spec("g", "go")];
    let (ok, failed) = fake.images().build_env_images(&specs, 2).unwrap();
    assert_eq!(ok.len(), 2);
    assert!(failed.is_empty());
    let dir = build_dir(&fake.root, ENV_IMAGE_BUILD_DIR, &specs[0].env_image_key());
    assert_eq!(
        fs::read_to_string(dir.join("setup_env.sh")).unwrap(),
        specs[0].setup_env_script()
    );
    let (ok, failed) = fake.images().build_env_images(&specs, 2).unwrap();
    assert!(ok.is_empty() && failed.is_empty());
    let fresh = Fake::new();
    fresh.image("pb.base.x86_64:latest", "sha256:base", "1");
    fresh.flag("buildfail");
    let (ok, failed) = fresh.images().build_env_images(&specs, 1).unwrap();
    assert!(ok.is_empty());
    assert_eq!(failed.len(), 2);
    let missing = Fake::new();
    missing.flag("buildfail");
    assert!(missing.images().build_env_images(&specs, 1).is_err());
    let stuck = Fake::new();
    stuck.image("pb.base.x86_64:latest", "sha256:base", "2024-02");
    stuck.image(&specs[1].env_image_key(), "sha256:genv", "2024-01");
    stuck.flag("rmfail");
    assert!(stuck.images().build_env_images(&specs, 1).is_err());
}

#[test]
fn instance_images_need_their_env_image_and_are_rebuilt_when_stale() {
    let fake = Fake::new();
    let s = spec("t", "go");
    let log = Log::discard();
    let e = fake
        .images()
        .build_instance_image(&s, &log, true)
        .unwrap_err();
    assert_eq!(
        e,
        format!(
            "Error building image t: Environment image {} not found for t\n",
            s.env_image_key()
        )
    );
    fake.image(&s.env_image_key(), "sha256:env", "2024-02");
    let repo = fake.root.join(&s.repo);
    fs::create_dir_all(&repo).unwrap();
    fs::write(repo.join("t.go"), "package t").unwrap();
    fake.images().build_instance_image(&s, &log, true).unwrap();
    let dir = build_dir(
        &fake.root,
        INSTANCE_IMAGE_BUILD_DIR,
        &s.instance_image_key(),
    );
    assert!(dir.join(&s.repo).join("t.go").is_file());
    assert_eq!(
        fs::read_to_string(dir.join("setup_repo.sh")).unwrap(),
        s.install_repo_script()
    );
    assert_eq!(fake.calls().matches("build -t").count(), 1);
    fake.images().build_instance_image(&s, &log, false).unwrap();
    assert_eq!(fake.calls().matches("build -t").count(), 1);
    fake.image(&s.instance_image_key(), "sha256:old", "2024-01");
    fake.images().build_instance_image(&s, &log, false).unwrap();
    assert_eq!(fake.calls().matches("build -t").count(), 2);
    fake.image(&s.instance_image_key(), "sha256:old", "2024-01");
    fake.flag("rmfail");
    assert!(fake.images().build_instance_image(&s, &log, false).is_err());
    let gone = Docker::new("hgm-no-such-docker-program");
    let images = Images {
        docker: &gone,
        root: &fake.root,
    };
    assert!(images.build_instance_image(&s, &log, false).is_err());
    let vanishing = Fake::new();
    vanishing.image(&s.env_image_key(), "sha256:env", "2024-02");
    vanishing.vanish_after(&s.env_image_key());
    assert!(
        vanishing
            .images()
            .build_instance_image(&s, &log, false)
            .is_err()
    );
}

#[test]
fn containers_start_from_the_instance_image() {
    let fake = Fake::new();
    let s = spec("t", "go");
    fake.image(&s.env_image_key(), "sha256:env", "2024-02");
    fake.image(&s.instance_image_key(), "sha256:i", "2024-03");
    let dir = tempfile::tempdir().unwrap();
    let log_path = dir.path().join("log");
    let log = Log::to_file(&log_path).unwrap();
    let container = fake.images().build_container(&s, "r1", &log, true).unwrap();
    assert_eq!(container.name(), "pb.eval.t.r1");
    assert!(
        fake.calls().contains(
            "run -d --name pb.eval.t.r1 --user root --platform linux/x86_64 pb.eval.x86_64.t:latest tail -f /dev/null"
        ),
        "{}",
        fake.calls()
    );
    fake.flag("runfail");
    let e = fake
        .images()
        .build_container(&s, "r2", &log, true)
        .unwrap_err();
    assert!(e.starts_with("Error building image t: "), "{e}");
    let text = fs::read_to_string(&log_path).unwrap();
    assert!(text.contains("Creating container for t..."), "{text}");
    assert!(text.contains("Error creating container for t: "), "{text}");
    let empty = Fake::new();
    assert!(
        empty
            .images()
            .build_container(&s, "r3", &log, true)
            .is_err()
    );
}

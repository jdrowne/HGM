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

fn gone() -> Docker {
    Docker::new("hgm-no-such-docker-program")
}

fn staged() -> (Fake, PathBuf) {
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
    (fake, dir)
}

#[test]
fn an_image_build_stages_its_setup_scripts() {
    let (_fake, dir) = staged();
    assert_eq!(
        fs::read_to_string(dir.join("setup_env.sh")).unwrap(),
        "echo hi\n"
    );
}

#[test]
fn an_image_build_stages_its_dockerfile() {
    let (_fake, dir) = staged();
    assert!(
        fs::read_to_string(dir.join("Dockerfile"))
            .unwrap()
            .starts_with("FROM scratch")
    );
}

#[test]
fn an_image_build_stages_the_repo_with_its_git_objects() {
    let (_fake, dir) = staged();
    assert_eq!(
        fs::read_to_string(dir.join("polyglot/task/.git/objects/o")).unwrap(),
        "object"
    );
}

#[test]
fn an_image_build_logs_each_step() {
    let (_fake, dir) = staged();
    let log = fs::read_to_string(dir.join("build_image.log")).unwrap();
    let missing: Vec<&str> = [
        "Building image img:latest\nUsing dockerfile:\nFROM scratch",
        "Adding (1) setup scripts to image build repo",
        "[SETUP SCRIPT] setup_env.sh:\necho hi",
        "Copied folder",
        "with platform linux/x86_64",
        "step one",
        "Image built successfully!",
    ]
    .into_iter()
    .filter(|line| !log.contains(line))
    .collect();
    assert_eq!(missing, Vec::<&str>::new(), "{log}");
}

#[test]
fn an_image_build_runs_docker_build_without_the_cache() {
    let (fake, _dir) = staged();
    let calls = fake.calls();
    assert!(
        calls.contains("build -t img:latest --platform linux/x86_64 --no-cache"),
        "{calls}"
    );
}

#[test]
fn a_missing_repo_source_is_not_staged() {
    let fake = Fake::new();
    let dir = fake.root.join("build");
    build_image(
        &fake.docker,
        &plain_build(&dir, Some((fake.root.join("missing"), "polyglot/gone"))),
    )
    .unwrap();
    assert!(!dir.join("polyglot/gone").exists());
}

#[test]
fn a_build_dir_under_a_file_fails_before_building() {
    let fake = Fake::new();
    let blocker = fake.root.join("file");
    fs::write(&blocker, "").unwrap();
    let e = build_image(&fake.docker, &plain_build(&blocker.join("dir"), None)).unwrap_err();
    assert!(!e.starts_with("Error building image"), "{e}");
}

fn blocked_repo_copy() -> String {
    let fake = Fake::new();
    let source = fake.root.join("task");
    fs::create_dir_all(&source).unwrap();
    let dir = fake.root.join("build");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("polyglot"), "").unwrap();
    build_image(
        &fake.docker,
        &plain_build(&dir, Some((source, "polyglot/t"))),
    )
    .unwrap_err()
}

#[test]
fn a_blocked_repo_copy_names_the_image() {
    let e = blocked_repo_copy();
    assert!(e.starts_with("Error building image img:latest: "), "{e}");
}

#[test]
fn a_blocked_repo_copy_error_ends_with_a_newline() {
    assert!(blocked_repo_copy().ends_with('\n'));
}

fn blocked(entry: &str) -> Result<(), String> {
    let fake = Fake::new();
    let dir = fake.root.join("blocked");
    fs::create_dir_all(dir.join(entry)).unwrap();
    build_image(&fake.docker, &plain_build(&dir, None))
}

#[test]
fn a_blocked_setup_script_fails_the_build() {
    assert!(blocked("setup_env.sh").is_err());
}

#[test]
fn a_blocked_dockerfile_fails_the_build() {
    assert!(blocked("Dockerfile").is_err());
}

#[test]
fn an_unreadable_repo_source_fails_the_build() {
    let fake = Fake::new();
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
}

fn failed_build() -> (Fake, PathBuf, String) {
    let fake = Fake::new();
    fake.flag("buildfail");
    let dir = fake.root.join("build-fails");
    let e = build_image(&fake.docker, &plain_build(&dir, None)).unwrap_err();
    (fake, dir, e)
}

#[test]
fn a_failing_docker_build_reports_its_output() {
    let (_fake, _dir, e) = failed_build();
    assert!(e.contains("build exploded"), "{e}");
}

#[test]
fn a_failing_docker_build_is_logged() {
    let (_fake, dir, _) = failed_build();
    let log = fs::read_to_string(dir.join("build_image.log")).unwrap();
    assert!(
        log.contains("ERROR - Error building image img:latest"),
        "{log}"
    );
}

#[test]
fn quiet_removal_skips_missing_images() {
    let fake = Fake::new();
    remove_image_quietly(&fake.docker, "absent:latest").unwrap();
    assert!(!fake.calls().contains("image rm"));
}

#[test]
fn quiet_removal_removes_a_present_image() {
    let fake = Fake::new();
    fake.image("present:latest", "sha256:p", "1");
    remove_image_quietly(&fake.docker, "present:latest").unwrap();
    assert!(!fake.has("present:latest"));
}

#[test]
fn quiet_removal_reports_a_failed_removal() {
    let fake = Fake::new();
    fake.image("stuck:latest", "sha256:s", "1");
    fake.flag("rmfail");
    assert!(remove_image_quietly(&fake.docker, "stuck:latest").is_err());
}

#[test]
fn quiet_removal_reports_a_missing_docker() {
    assert!(remove_image_quietly(&gone(), "x").is_err());
}

#[test]
fn a_missing_base_has_no_dependents() {
    let fake = Fake::new();
    assert_eq!(
        find_dependent_images(&fake.docker, "env:latest").unwrap(),
        Vec::<String>::new()
    );
}

fn with_dependents() -> Fake {
    let fake = Fake::new();
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
    fake
}

#[test]
fn dependents_are_images_whose_history_holds_the_base() {
    let fake = with_dependents();
    assert_eq!(
        find_dependent_images(&fake.docker, "env:latest").unwrap(),
        ["child:latest", "sha256:bare", "sha256:solo"]
    );
}

fn without_other_history() -> Fake {
    let fake = with_dependents();
    fs::remove_file(fake.state.join("history/sha256_other")).unwrap();
    fake
}

#[test]
fn images_that_vanished_are_skipped() {
    let fake = without_other_history();
    assert_eq!(
        find_dependent_images(&fake.docker, "env:latest").unwrap(),
        ["child:latest", "sha256:bare", "sha256:solo"]
    );
}

#[test]
fn an_image_without_a_history_is_an_error() {
    let fake = without_other_history();
    fake.image("sha256:other", "sha256:other", "1");
    assert!(find_dependent_images(&fake.docker, "env:latest").is_err());
}

#[test]
fn a_failing_image_listing_is_an_error() {
    let fake = with_dependents();
    fake.flag("lsfail");
    assert!(find_dependent_images(&fake.docker, "env:latest").is_err());
}

#[test]
fn finding_dependents_without_docker_is_an_error() {
    assert!(find_dependent_images(&gone(), "env:latest").is_err());
}

fn two_specs() -> [TestSpec; 2] {
    [spec("a", "go"), spec("b", "rust")]
}

fn base_built() -> Fake {
    let fake = Fake::new();
    fake.images().build_base_images(&two_specs()).unwrap();
    fake
}

fn base_dir(fake: &Fake) -> PathBuf {
    build_dir(
        &fake.root,
        "logs/build_images/base",
        "pb.base.x86_64:latest",
    )
}

#[test]
fn the_base_image_is_built() {
    assert!(base_built().has("pb.base.x86_64:latest"));
}

#[test]
fn the_base_image_is_built_once() {
    assert_eq!(base_built().calls().matches("build -t").count(), 1);
}

#[test]
fn base_images_build_under_their_own_folder() {
    assert!(base_dir(&Fake::new()).ends_with("logs/build_images/base/pb.base.x86_64__latest"));
}

#[test]
fn the_base_dockerfile_starts_from_buildpack_deps() {
    let fake = base_built();
    assert!(
        fs::read_to_string(base_dir(&fake).join("Dockerfile"))
            .unwrap()
            .contains("buildpack-deps")
    );
}

#[test]
fn a_present_base_image_is_not_rebuilt() {
    let fake = base_built();
    fake.images().build_base_images(&two_specs()).unwrap();
    assert_eq!(fake.calls().matches("build -t").count(), 1);
}

#[test]
fn a_failing_base_build_is_an_error() {
    let fake = Fake::new();
    fake.flag("buildfail");
    assert!(fake.images().build_base_images(&two_specs()).is_err());
}

#[test]
fn building_base_images_without_docker_is_an_error() {
    let fake = Fake::new();
    let docker = gone();
    let images = Images {
        docker: &docker,
        root: &fake.root,
    };
    assert!(images.build_base_images(&two_specs()).is_err());
}

fn three_specs() -> [TestSpec; 3] {
    [spec("p", "python"), spec("g", "go"), spec("r", "rust")]
}

#[test]
fn env_images_need_the_base_image() {
    let fake = Fake::new();
    let specs = three_specs();
    assert_eq!(
        fake.images().env_images_to_build(&specs).unwrap_err(),
        format!(
            "Base image pb.base.x86_64:latest not found for {}\n.Please build the base images first.",
            specs[0].env_image_key()
        )
    );
}

fn needed_keys(fake: &Fake) -> Vec<String> {
    fake.images()
        .env_images_to_build(&three_specs())
        .unwrap()
        .into_iter()
        .map(|(key, _)| key)
        .collect()
}

fn with_base() -> Fake {
    let fake = Fake::new();
    fake.image("pb.base.x86_64:latest", "sha256:base", "2024-02");
    fake
}

#[test]
fn missing_env_images_are_built_once_each() {
    let [python, go, _] = three_specs();
    assert_eq!(
        needed_keys(&with_base()),
        [python.env_image_key(), go.env_image_key()]
    );
}

fn stale_go_env() -> Fake {
    let fake = with_base();
    let [python, go, _] = three_specs();
    fake.image(&python.env_image_key(), "sha256:penv", "2024-03");
    fake.image(&go.env_image_key(), "sha256:genv", "2024-01");
    fake.image("pb.eval.x86_64.g:latest", "sha256:geval", "2024-01");
    fs::write(
        fake.state.join("ls"),
        "sha256:geval pb.eval.x86_64.g:latest\n",
    )
    .unwrap();
    fs::write(fake.state.join("history/sha256_geval"), "sha256:genv\n").unwrap();
    fake
}

#[test]
fn an_env_image_older_than_its_base_is_rebuilt() {
    assert_eq!(
        needed_keys(&stale_go_env()),
        [three_specs()[1].env_image_key()]
    );
}

#[test]
fn a_stale_env_image_takes_its_instance_images_with_it() {
    let fake = stale_go_env();
    needed_keys(&fake);
    assert!(!fake.has("pb.eval.x86_64.g:latest"));
}

#[test]
fn a_stale_env_image_is_removed() {
    let fake = stale_go_env();
    needed_keys(&fake);
    assert!(!fake.has(&three_specs()[1].env_image_key()));
}

fn restaled() -> Fake {
    let fake = stale_go_env();
    needed_keys(&fake);
    fake.image(&three_specs()[1].env_image_key(), "sha256:genv", "2024-01");
    fake
}

#[test]
fn a_stale_env_image_that_cannot_be_removed_is_an_error() {
    let fake = restaled();
    fake.flag("rmfail");
    assert!(fake.images().env_images_to_build(&three_specs()).is_err());
}

#[test]
fn a_dependent_image_that_cannot_be_removed_is_an_error() {
    let fake = restaled();
    fake.flag("rmfail");
    fake.image("pb.eval.x86_64.g:latest", "sha256:geval", "2024-01");
    assert!(fake.images().env_images_to_build(&three_specs()).is_err());
}

#[test]
fn a_failing_listing_of_dependents_is_an_error() {
    let fake = restaled();
    fake.image("pb.eval.x86_64.g:latest", "sha256:geval", "2024-01");
    fake.flag("lsfail");
    assert!(fake.images().env_images_to_build(&three_specs()).is_err());
}

#[test]
fn checking_env_images_without_docker_is_an_error() {
    let fake = Fake::new();
    let docker = gone();
    let images = Images {
        docker: &docker,
        root: &fake.root,
    };
    assert!(images.env_images_to_build(&three_specs()).is_err());
}

#[test]
fn a_base_image_that_vanishes_is_an_error() {
    let fake = with_base();
    fake.vanish_after("pb.base.x86_64:latest");
    assert!(fake.images().env_images_to_build(&three_specs()).is_err());
}

type Built = (Vec<String>, Vec<String>);

fn env_specs() -> [TestSpec; 2] {
    [spec("p", "python"), spec("g", "go")]
}

fn env_built() -> (Fake, Built) {
    let fake = Fake::new();
    let built = fake.images().build_env_images(&env_specs(), 2).unwrap();
    (fake, built)
}

#[test]
fn env_image_builds_report_their_successes() {
    assert_eq!(env_built().1.0.len(), 2);
}

#[test]
fn successful_env_image_builds_report_no_failures() {
    assert!(env_built().1.1.is_empty());
}

#[test]
fn env_image_builds_stage_the_setup_script() {
    let (fake, _) = env_built();
    let specs = env_specs();
    let dir = build_dir(&fake.root, ENV_IMAGE_BUILD_DIR, &specs[0].env_image_key());
    assert_eq!(
        fs::read_to_string(dir.join("setup_env.sh")).unwrap(),
        specs[0].setup_env_script()
    );
}

fn rebuilt() -> Built {
    let (fake, _) = env_built();
    fake.images().build_env_images(&env_specs(), 2).unwrap()
}

#[test]
fn present_env_images_are_not_rebuilt() {
    assert!(rebuilt().0.is_empty());
}

#[test]
fn present_env_images_report_no_failures() {
    assert!(rebuilt().1.is_empty());
}

fn env_failed() -> Built {
    let fake = Fake::new();
    fake.image("pb.base.x86_64:latest", "sha256:base", "1");
    fake.flag("buildfail");
    fake.images().build_env_images(&env_specs(), 1).unwrap()
}

#[test]
fn failing_env_image_builds_report_no_successes() {
    assert!(env_failed().0.is_empty());
}

#[test]
fn failing_env_image_builds_report_each_failure() {
    assert_eq!(env_failed().1.len(), 2);
}

#[test]
fn env_image_builds_without_a_base_are_an_error() {
    let fake = Fake::new();
    fake.flag("buildfail");
    assert!(fake.images().build_env_images(&env_specs(), 1).is_err());
}

#[test]
fn env_image_builds_that_cannot_remove_a_stale_image_are_an_error() {
    let fake = Fake::new();
    let specs = env_specs();
    fake.image("pb.base.x86_64:latest", "sha256:base", "2024-02");
    fake.image(&specs[1].env_image_key(), "sha256:genv", "2024-01");
    fake.flag("rmfail");
    assert!(fake.images().build_env_images(&specs, 1).is_err());
}

fn go_spec() -> TestSpec {
    spec("t", "go")
}

#[test]
fn instance_images_need_their_env_image() {
    let fake = Fake::new();
    let s = go_spec();
    assert_eq!(
        fake.images()
            .build_instance_image(&s, &Log::discard(), true)
            .unwrap_err(),
        format!(
            "Error building image t: Environment image {} not found for t\n",
            s.env_image_key()
        )
    );
}

fn instance_built() -> Fake {
    let fake = Fake::new();
    let s = go_spec();
    fake.image(&s.env_image_key(), "sha256:env", "2024-02");
    let repo = fake.root.join(&s.repo);
    fs::create_dir_all(&repo).unwrap();
    fs::write(repo.join("t.go"), "package t").unwrap();
    fake.images()
        .build_instance_image(&s, &Log::discard(), true)
        .unwrap();
    fake
}

fn instance_dir(fake: &Fake) -> PathBuf {
    build_dir(
        &fake.root,
        INSTANCE_IMAGE_BUILD_DIR,
        &go_spec().instance_image_key(),
    )
}

#[test]
fn instance_images_stage_the_repo() {
    let fake = instance_built();
    assert!(
        instance_dir(&fake)
            .join(&go_spec().repo)
            .join("t.go")
            .is_file()
    );
}

#[test]
fn instance_images_stage_the_repo_setup_script() {
    let fake = instance_built();
    assert_eq!(
        fs::read_to_string(instance_dir(&fake).join("setup_repo.sh")).unwrap(),
        go_spec().install_repo_script()
    );
}

#[test]
fn an_instance_image_is_built_once() {
    assert_eq!(instance_built().calls().matches("build -t").count(), 1);
}

#[test]
fn a_fresh_instance_image_is_not_rebuilt() {
    let fake = instance_built();
    fake.images()
        .build_instance_image(&go_spec(), &Log::discard(), false)
        .unwrap();
    assert_eq!(fake.calls().matches("build -t").count(), 1);
}

fn stale_instance() -> Fake {
    let fake = instance_built();
    fake.image(&go_spec().instance_image_key(), "sha256:old", "2024-01");
    fake
}

#[test]
fn an_instance_image_older_than_its_env_is_rebuilt() {
    let fake = stale_instance();
    fake.images()
        .build_instance_image(&go_spec(), &Log::discard(), false)
        .unwrap();
    assert_eq!(fake.calls().matches("build -t").count(), 2);
}

#[test]
fn a_stale_instance_image_that_cannot_be_removed_is_an_error() {
    let fake = stale_instance();
    fake.flag("rmfail");
    assert!(
        fake.images()
            .build_instance_image(&go_spec(), &Log::discard(), false)
            .is_err()
    );
}

#[test]
fn building_an_instance_image_without_docker_is_an_error() {
    let fake = Fake::new();
    let docker = gone();
    let images = Images {
        docker: &docker,
        root: &fake.root,
    };
    assert!(
        images
            .build_instance_image(&go_spec(), &Log::discard(), false)
            .is_err()
    );
}

#[test]
fn an_env_image_that_vanishes_is_an_error() {
    let fake = Fake::new();
    let s = go_spec();
    fake.image(&s.env_image_key(), "sha256:env", "2024-02");
    fake.vanish_after(&s.env_image_key());
    assert!(
        fake.images()
            .build_instance_image(&s, &Log::discard(), false)
            .is_err()
    );
}

fn ready() -> Fake {
    let fake = Fake::new();
    let s = go_spec();
    fake.image(&s.env_image_key(), "sha256:env", "2024-02");
    fake.image(&s.instance_image_key(), "sha256:i", "2024-03");
    fake
}

#[test]
fn containers_are_named_for_the_instance_and_run() {
    let fake = ready();
    let container = fake
        .images()
        .build_container(&go_spec(), "r1", &[], &Log::discard(), true)
        .unwrap();
    assert_eq!(container.name(), "pb.eval.t.r1");
}

#[test]
fn containers_start_from_the_instance_image() {
    let fake = ready();
    fake.images()
        .build_container(&go_spec(), "r1", &[], &Log::discard(), true)
        .unwrap();
    let calls = fake.calls();
    assert!(
        calls.contains(
            "run -d --name pb.eval.t.r1 --user root --platform linux/x86_64 pb.eval.x86_64.t:latest tail -f /dev/null"
        ),
        "{calls}"
    );
}

fn failed_start() -> (tempfile::TempDir, String) {
    let fake = ready();
    fake.flag("runfail");
    let dir = tempfile::tempdir().unwrap();
    let log = Log::to_file(&dir.path().join("log")).unwrap();
    let e = fake
        .images()
        .build_container(&go_spec(), "r2", &[], &log, true)
        .unwrap_err();
    (dir, e)
}

fn failed_start_log() -> String {
    let (dir, _) = failed_start();
    fs::read_to_string(dir.path().join("log")).unwrap()
}

#[test]
fn a_container_that_fails_to_start_names_the_instance() {
    let (_dir, e) = failed_start();
    assert!(e.starts_with("Error building image t: "), "{e}");
}

#[test]
fn a_container_start_is_logged() {
    let text = failed_start_log();
    assert!(text.contains("Creating container for t..."), "{text}");
}

#[test]
fn a_container_that_fails_to_start_is_logged() {
    let text = failed_start_log();
    assert!(text.contains("Error creating container for t: "), "{text}");
}

#[test]
fn a_container_without_its_images_is_an_error() {
    assert!(
        Fake::new()
            .images()
            .build_container(&go_spec(), "r3", &[], &Log::discard(), true)
            .is_err()
    );
}

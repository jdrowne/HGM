# RustyHGM

RustyHGM is a Rust port of the [Huxley-Gödel Machine](https://github.com/metauto-ai/HGM) (HGM), a self-improving machine that uses clade-level statistics to approximate the Gödel machine, a program that rewrites its own code whenever it can prove that the change will make it better at its goal. HGM improves a coding harness by having the harness rewrite its own source, with every rewrite becoming a new version. The coding harness is the program that hands a task to a language model, runs the `bash` and `editor` tool calls the model sends back, and writes a patch; the model is a fixed input, and only the harness's source changes. It tests those versions on coding tasks and scores each by how often it succeeds, stopping after a set number of tests in total.

HGM is based on the paper [*Huxley-Gödel Machine: Human-Level Coding Agent Development by an Approximation of the Optimal Self-Improving Machine*](https://arxiv.org/abs/2510.21614) published by Wenyi Wang, Piotr Piękos, Li Nanbo, Firas Laakom, Yimeng Chen, Mateusz Ostaszewski, Mingchen Zhuge, and Jürgen Schmidhuber.

## Commands

RustyHGM is one command, `rustyhgm`, with no sub-commands. It is the machine: it builds versions of the coding harness, schedules and scores them, and picks the best, so you run it and get back a better harness. It is a single file that carries the progenitor, the harness every version descends from, and everything needed to build it, so it runs from any folder. It is released for Linux, as a package that also installs what it needs or as a bare file. Download it for your platform from the [latest release](https://github.com/jdrowne/RustyHGM/releases/latest):

| Platform | Debian, Ubuntu | Fedora, RHEL | Bare file |
| --- | --- | --- | --- |
| Linux x86_64 | `rustyhgm-linux-x86-64.deb` | `rustyhgm-linux-x86-64.rpm` | `rustyhgm-linux-x86-64` |
| Linux arm64 | `rustyhgm-linux-arm64.deb` | `rustyhgm-linux-arm64.rpm` | `rustyhgm-linux-arm64` |

The packages put `rustyhgm` in `/usr/bin` and pull in Docker, git and Python 3. On Debian and Ubuntu, any of `docker.io`, `docker-ce-cli`, `docker-cli` or `podman-docker` counts as Docker; on Fedora and RHEL, any package that provides `/usr/bin/docker` does.

```sh
curl -LO https://github.com/jdrowne/RustyHGM/releases/latest/download/rustyhgm-linux-x86-64.deb
sudo apt install ./rustyhgm-linux-x86-64.deb
rustyhgm [flags]
```

```sh
sudo dnf install https://github.com/jdrowne/RustyHGM/releases/latest/download/rustyhgm-linux-x86-64.rpm
rustyhgm [flags]
```

The bare file needs nothing installed with it, so its dependencies are up to you:

```sh
curl -L -o rustyhgm https://github.com/jdrowne/RustyHGM/releases/latest/download/rustyhgm-linux-x86-64
chmod +x rustyhgm
./rustyhgm [flags]
```

Or build it from a clone with `cargo build --release -p rustyhgm`, which puts it at `target/release/rustyhgm`.

Every folder and file `rustyhgm` uses comes from a flag. A relative path is taken from the folder you run it in, as usual, and nothing else is looked up there.

One run does the whole job:

1. **Fetches task data** it doesn't have yet. For `--polyglot` runs, that means cloning the [Aider Polyglot benchmark](https://github.com/Aider-AI/polyglot-benchmark) into `polyglot-benchmark` in the `--polyglot_dir` folder and writing its task list next to it, to `polyglot_benchmark_metadata.json`.
2. **Improves the harness** until the task budget is spent. A fresh run first writes out its built-in progenitor, builds it and measures it on every task in the pool.
3. **Picks the best version**: the one with the highest success rate it can trust, given how many tasks it was tried on.
4. **Writes that version's source** to `best_harness/` in the `--output_dir` folder and prints where it went. This folder is the improved harness: the progenitor with every patch on its line applied.
5. **Optionally scores it on held-out tasks** with `--holdout_split`. These are SWE-bench tasks it never practised on, so the score shows whether it really got better.

```sh
./rustyhgm \
  --self_improve_llm gpt-5-mini --self_improve_provider openai --self_improve_base_url https://api.openai.com/v1 \
  --downstream_llm gpt-5-mini --downstream_provider openai --downstream_base_url https://api.openai.com/v1 \
  --diagnose_llm gpt-5-mini --diagnose_provider openai --diagnose_base_url https://api.openai.com/v1 \
  --api_key_file ~/.config/rustyhgm/api_key \
  --docker docker --swebench_python python3 \
  --dataset_endpoint https://huggingface.co --dataset_dir ~/.cache/rustyhgm/datasets \
  --polyglot_repo https://github.com/Aider-AI/polyglot-benchmark.git --polyglot_dir rustyhgm/polyglot \
  --alpha 0.6 --beta 1.0 --eval_random_level 1.0 \
  --n_pseudo_descendant_evals 10000 --expansion_statistic clade --epsilon 0.01 \
  --max_workers 20 --max_task_evals 100 --self_improve_timeout 3600 --evaluation_timeout 3600 \
  --output_dir runs/first \
  --holdout_split Lite --holdout_tasks 50
```

`rustyhgm --help` lists the flags, and `rustyhgm --version` prints the version. Flag names use underscores, as in `--max_task_evals 100`, and a value can also be joined with `=`, as in `--max_task_evals=100`. If a run stops early, `--continue_from runs/first --output_dir runs/first` resumes it in the same folder.

### Before a run

- **Docker.** Every build, self-edit and task runs in a container. `--docker` names the program, `docker` or any compatible one.
- **A model API key in a file.** `--api_key_file` names a file that holds only the key. `rustyhgm` checks it can read the file before any work starts, and mounts it read-only into each container that runs the harness. Keep it outside the repository, so it never ends up in a harness's source or image.
- **Python 3 with the `swebench` package** for SWE-bench grading. `--swebench_python` names the interpreter. The packages install Python 3 but not `swebench`, which comes from `pip install swebench`.
- **git**, for fetching the Polyglot benchmark from `--polyglot_repo` and writing the best harness's source.
- **A dataset source and cache.** SWE-bench tasks come from the Hugging Face server at `--dataset_endpoint`, `https://huggingface.co` or a mirror, and are kept under `--dataset_dir`.

### Flags

Every setting that takes a value is a required flag with no default, so the command line alone says what a run will do. Leaving one out stops `rustyhgm` with an error that names it. Yes/no settings are off unless their flag is given, as in `--cool_down`, and take no value.

| Flag | What it sets |
| --- | --- |
| `--self_improve_llm <model>` | Model the harness uses while it edits itself |
| `--self_improve_provider <openai\|openrouter\|vllm>` | Service that serves `--self_improve_llm` |
| `--self_improve_base_url <url>` | Address of that service, as the harness's container reaches it |
| `--downstream_llm <model>` | Model the harness uses while it solves tasks |
| `--downstream_provider <openai\|openrouter\|vllm>` | Service that serves `--downstream_llm` |
| `--downstream_base_url <url>` | Address of that service, as the harness's container reaches it |
| `--diagnose_llm <model>` | Model that turns a failure into a change request |
| `--diagnose_provider <openai\|openrouter\|vllm>` | Service that serves `--diagnose_llm` |
| `--diagnose_base_url <url>` | Address of that service, as `rustyhgm` reaches it |
| `--api_key_file <file>` | File holding the key sent to every service |
| `--docker <program>` | Docker-compatible program that runs every container |
| `--swebench_python <program>` | Python interpreter with `swebench` that grades SWE-bench tasks |
| `--dataset_endpoint <url>` | Hugging Face server, or mirror, that SWE-bench tasks come from |
| `--dataset_dir <dir>` | Where downloaded datasets are kept |
| `--polyglot_repo <url>` | Clone URL for the Polyglot benchmark |
| `--polyglot_dir <dir>` | Where the Polyglot benchmark is cloned and its task list written |
| `--alpha <x>` | How bushy the tree grows: more children per evaluation as it rises |
| `--beta <x>` | How fast `--cool_down` sharpens; ignored without it |
| `--cool_down` | Sharpens every draw as the budget runs out |
| `--eval_random_level <p>` | Odds of picking an untried task at random, rather than the next one in pool order |
| `--n_pseudo_descendant_evals <n>` | Caps how many of a version's own results count in its clade pool |
| `--expansion_statistic <clade\|own>` | Whether a parent is judged on its whole clade or on its own results |
| `--epsilon <p>` | How cautious the final pick is: each version is scored by this percentile of its Beta posterior |
| `--max_workers <n>` | Ticks and task evaluations run at the same time |
| `--max_task_evals <n>` | Total task evaluations in the run's budget |
| `--self_improve_timeout <secs>` | Time limit for one self-edit |
| `--evaluation_timeout <secs>` | Time limit for one task attempt |
| `--full_eval` | Uses all 500 SWE-bench Verified tasks instead of the 60-task pool |
| `--polyglot` | Uses the Polyglot benchmark instead of SWE-bench |
| `--output_dir <dir>` | Where the run's files go; a fresh run needs it empty or missing |

These flags ask for an extra step, so they are given only when that step is wanted.

| Flag | What it does |
| --- | --- |
| `--continue_from <dir>` | Resumes an earlier run from its record |
| `--holdout_split <name>` | After the run, scores the best harness on `princeton-nlp/SWE-bench_<name>`, such as `Lite`; needs `--holdout_tasks` and is not allowed with `--polyglot` |
| `--holdout_tasks <n>` | Scores the first `n` held-out tasks; needs `--holdout_split` |

Runs share no results: only the downloaded data in `--dataset_dir` and `--polyglot_dir`, and the benchmarks' own Docker images, are reused between them. A fresh run writes the progenitor into `progenitor/` in the `--output_dir` folder, builds it and measures it on every task in the pool: 60 task runs with the default pool, paid by every fresh run and by no continued one. The progenitor is the `rustyhgm-coding-harness` source as it was when `rustyhgm` was built, with the builder `Dockerfile` beside it; it is the root of the tree, and a run records it under the commit id `progenitor`. Each run also gets an id of its own, kept in its record, and names the Docker images it builds for the coding harness after that id, so two runs never share one. The images are removed when the run ends, and a continued run rebuilds what it needs from its own folder. A fresh run stops with an error if `--output_dir` already holds files; `--continue_from` is the only way to pick up an earlier run's files. A run recorded by a release from before run ids cannot be continued.

The held-out check uses the `--downstream_llm` model, the `--evaluation_timeout` limit and `--max_workers`, and writes its results to `holdout/best_harness/` in the `--output_dir` folder.

### The coding harness

The machine also builds `rustyhgm-coding-harness`, the coding harness being evolved, and runs it inside containers. Releases carry only `rustyhgm`, so to watch one attempt by hand, run the harness from a clone of this repository with `cargo run -p rustyhgm-coding-harness -- <flags>`. It reads no environment variables. Every flag that takes a value is required except `--instance_id`, and the yes/no flags are off unless given.

| Flag | What it sets |
| --- | --- |
| `--problem_statement <text>` | The task or change request |
| `--git_dir <dir>` | The repository to work in |
| `--base_commit <sha>` | The commit the final diff is taken against |
| `--chat_history_file <file>` | Where the conversation log is written |
| `--outdir <dir>` | Where `model_patch.diff` is written; empty means the working directory |
| `--test_description <text>` | How to run the repository's tests |
| `--self_improve` | Whether it edits the harness's own source rather than solving a task |
| `--polyglot` | Whether it uses Polyglot prompts and tests |
| `--language <lang>` | Task language: `cpp`, `java`, `python`, `go`, `rust` or `javascript` |
| `--model <model>` | Model to use |
| `--provider <openai\|openrouter\|vllm>` | Service that serves the model |
| `--base_url <url>` | Address of that service |
| `--api_key_file <file>` | File holding the service's key |
| `--timeout <secs>` | Time limit for the attempt |
| `--instance_id <id>` | Optional: the task's ID, used for logging |

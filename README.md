# RustyHGM

RustyHGM is a Rust port of the [Huxley-Gödel Machine](https://github.com/metauto-ai/HGM) (HGM), a self-improving machine that uses clade-level statistics to approximate the Gödel machine, a program that rewrites its own code whenever it can prove that the change will make it better at its goal. HGM improves a coding agent by having the agent rewrite its own source, with every rewrite becoming a new version. It tests those versions on coding tasks and scores each by how often it succeeds, stopping after a set number of tests in total.

HGM is based on the paper [*Huxley-Gödel Machine: Human-Level Coding Agent Development by an Approximation of the Optimal Self-Improving Machine*](https://arxiv.org/abs/2510.21614) published by Wenyi Wang, Piotr Piękos, Li Nanbo, Firas Laakom, Yimeng Chen, Mateusz Ostaszewski, Mingchen Zhuge, and Jürgen Schmidhuber.

## Commands

RustyHGM is one command, `hgm`, with no sub-commands. You run it and get back a better agent. It is a single file that carries the seed agent and everything needed to build it, so it runs from any folder. It is released for Linux. Download it for your platform from the [latest release](https://github.com/jdrowne/RustyHGM/releases/latest):

| Platform | File |
| --- | --- |
| Linux x86_64 | `hgm-linux-x86-64` |
| Linux arm64 | `hgm-linux-arm64` |

```sh
curl -L -o hgm https://github.com/jdrowne/RustyHGM/releases/latest/download/hgm-linux-x86-64
chmod +x hgm
./hgm [flags]
```

Or build it from a clone with `cargo build --release -p hgm`, which puts it at `target/release/hgm`.

Every folder and file `hgm` uses comes from a flag. A relative path is taken from the folder you run it in, as usual, and nothing else is looked up there.

One run does the whole job:

1. **Fetches task data** it doesn't have yet. For `--polyglot` runs, that means cloning the [Aider Polyglot benchmark](https://github.com/Aider-AI/polyglot-benchmark) into `polyglot-benchmark` in the `--polyglot_dir` folder and writing its task list next to it, to `polyglot_benchmark_metadata.json`.
2. **Improves the agent** until the task budget is spent. On the first run with a given agent name, it also writes out its built-in seed agent, builds it and measures it on every task in the pool.
3. **Picks the best version**: the one with the highest success rate it can trust, given how many tasks it was tried on.
4. **Writes that version's source** to `best_agent/` in the `--output_dir` folder and prints where it went. This folder is the improved agent: the seed with every patch on its line applied.
5. **Optionally scores it on held-out tasks** with `--holdout_split`. These are SWE-bench tasks it never practised on, so the score shows whether it really got better.

```sh
./hgm \
  --self_improve_llm gpt-5-mini --self_improve_provider openai --self_improve_base_url https://api.openai.com/v1 \
  --downstream_llm gpt-5-mini --downstream_provider openai --downstream_base_url https://api.openai.com/v1 \
  --diagnose_llm gpt-5-mini --diagnose_provider openai --diagnose_base_url https://api.openai.com/v1 \
  --api_key_file ~/.config/hgm/api_key \
  --docker docker --swebench_python python3 \
  --dataset_endpoint https://huggingface.co --dataset_dir ~/.cache/hgm/datasets \
  --polyglot_repo https://github.com/Aider-AI/polyglot-benchmark.git --polyglot_dir hgm/polyglot \
  --seed_dir hgm/seeds \
  --alpha 0.6 --beta 1.0 --cool_down false --eval_random_level 1.0 \
  --n_pseudo_descendant_evals 10000 --expansion_statistic clade --epsilon 0.01 \
  --max_workers 20 --max_task_evals 100 --self_improve_timeout 3600 --evaluation_timeout 3600 \
  --full_eval false --polyglot false \
  --output_dir runs/first --initial_agent_name default_agent \
  --holdout_split Lite --holdout_tasks 50
```

`hgm --help` lists the flags, and `hgm --version` prints the version. Flag names use underscores, as in `--max_task_evals 100`, and a value can also be joined with `=`, as in `--max_task_evals=100`. If a run stops early, `--continue_from runs/first --output_dir runs/first` resumes it in the same folder.

### Before a run

- **Docker.** Every build, self-edit and task runs in a container. `--docker` names the program, `docker` or any compatible one.
- **A model API key in a file.** `--api_key_file` names a file that holds only the key. `hgm` checks it can read the file before any work starts, and mounts it read-only into each container that runs the agent. Keep it outside the repository, so it never ends up in an agent's source or image.
- **Python 3 with the `swebench` package** for SWE-bench grading. `--swebench_python` names the interpreter.
- **git**, for fetching the Polyglot benchmark from `--polyglot_repo` and writing the best agent's source.
- **A dataset source and cache.** SWE-bench tasks come from the Hugging Face server at `--dataset_endpoint`, `https://huggingface.co` or a mirror, and are kept under `--dataset_dir`.

### Flags

Every setting is a required flag with no default, so the command line alone says what a run will do. Leaving one out stops `hgm` with an error that names it. Yes/no settings take `true` or `false`, as in `--cool_down false`.

| Flag | What it sets |
| --- | --- |
| `--self_improve_llm <model>` | Model the agent uses while it edits itself |
| `--self_improve_provider <openai\|openrouter\|vllm>` | Service that serves `--self_improve_llm` |
| `--self_improve_base_url <url>` | Address of that service, as the agent's container reaches it |
| `--downstream_llm <model>` | Model the agent uses while it solves tasks |
| `--downstream_provider <openai\|openrouter\|vllm>` | Service that serves `--downstream_llm` |
| `--downstream_base_url <url>` | Address of that service, as the agent's container reaches it |
| `--diagnose_llm <model>` | Model that turns a failure into a change request |
| `--diagnose_provider <openai\|openrouter\|vllm>` | Service that serves `--diagnose_llm` |
| `--diagnose_base_url <url>` | Address of that service, as `hgm` reaches it |
| `--api_key_file <file>` | File holding the key sent to every service |
| `--docker <program>` | Docker-compatible program that runs every container |
| `--swebench_python <program>` | Python interpreter with `swebench` that grades SWE-bench tasks |
| `--dataset_endpoint <url>` | Hugging Face server, or mirror, that SWE-bench tasks come from |
| `--dataset_dir <dir>` | Where downloaded datasets are kept |
| `--polyglot_repo <url>` | Clone URL for the Polyglot benchmark |
| `--polyglot_dir <dir>` | Where the Polyglot benchmark is cloned and its task list written |
| `--seed_dir <dir>` | Where the seed agent is built and measured, and found again by later runs |
| `--alpha <x>` | How bushy the tree grows: more children per evaluation as it rises |
| `--beta <x>` | How fast `--cool_down` sharpens; ignored without it |
| `--cool_down <true\|false>` | Sharpens every draw as the budget runs out |
| `--eval_random_level <p>` | Odds of picking an untried task at random, rather than the next one in pool order |
| `--n_pseudo_descendant_evals <n>` | Caps how many of a version's own results count in its clade pool |
| `--expansion_statistic <clade\|own>` | Whether a parent is judged on its whole clade or on its own results |
| `--epsilon <p>` | How cautious the final pick is: each version is scored by this percentile of its Beta posterior |
| `--max_workers <n>` | Ticks and task evaluations run at the same time |
| `--max_task_evals <n>` | Total task evaluations in the run's budget |
| `--self_improve_timeout <secs>` | Time limit for one self-edit |
| `--evaluation_timeout <secs>` | Time limit for one task attempt |
| `--full_eval <true\|false>` | Uses all 500 SWE-bench Verified tasks instead of the 60-task pool |
| `--polyglot <true\|false>` | Uses the Polyglot benchmark instead of SWE-bench |
| `--output_dir <dir>` | Where the run record goes |
| `--initial_agent_name <name>` | Name of the seed agent's folder and Docker image |

These flags ask for an extra step, so they are given only when that step is wanted.

| Flag | What it does |
| --- | --- |
| `--continue_from <dir>` | Resumes an earlier run from its record |
| `--holdout_split <name>` | After the run, scores the best agent on `princeton-nlp/SWE-bench_<name>`, such as `Lite`; needs `--holdout_tasks` and is not allowed with `--polyglot true` |
| `--holdout_tasks <n>` | Scores the first `n` held-out tasks; needs `--holdout_split` |

The first run with a given agent name writes the seed into `initial_swe/<name>/` or `initial_polyglot/<name>/` in the `--seed_dir` folder and measures it on every task in the pool. The seed is the `hgm-coding-agent` source as it was when `hgm` was built, with the builder `Dockerfile` beside it. Later runs with the same `--seed_dir` reuse that folder and its results, so delete it to measure a newer seed. A seed folder written by a release from before the agent was named `hgm-coding-agent` holds the old name and paths, which this `hgm` cannot build or patch, so delete it too.

The held-out check uses the `--downstream_llm` model, the `--evaluation_timeout` limit and `--max_workers`, and writes its results to `holdout/best_agent/` in the `--output_dir` folder.

### The coding agent

The harness also builds `hgm-coding-agent`, the agent being evolved, and runs it inside containers. It can be run by hand to watch one attempt, with `cargo run -p hgm-coding-agent -- <flags>`. It reads no environment variables, and every flag but `--instance_id` is required.

| Flag | What it sets |
| --- | --- |
| `--problem_statement <text>` | The task or change request |
| `--git_dir <dir>` | The repository to work in |
| `--base_commit <sha>` | The commit the final diff is taken against |
| `--chat_history_file <file>` | Where the conversation log is written |
| `--outdir <dir>` | Where `model_patch.diff` is written; empty means the working directory |
| `--test_description <text>` | How to run the repository's tests |
| `--self_improve <true\|false>` | Whether it edits the agent's own source rather than solving a task |
| `--polyglot <true\|false>` | Whether it uses Polyglot prompts and tests |
| `--language <lang>` | Task language: `cpp`, `java`, `python`, `go`, `rust` or `javascript` |
| `--model <model>` | Model to use |
| `--provider <openai\|openrouter\|vllm>` | Service that serves the model |
| `--base_url <url>` | Address of that service |
| `--api_key_file <file>` | File holding the service's key |
| `--timeout <secs>` | Time limit for the attempt |
| `--instance_id <id>` | Optional: the task's ID, used for logging |

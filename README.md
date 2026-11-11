# RustyHGM

RustyHGM is a Rust port of the [Huxley-Gödel Machine](https://github.com/metauto-ai/HGM) (HGM), a self-improving machine that uses clade-level statistics to approximate the Gödel machine, a program that rewrites its own code whenever it can prove that the change will make it better at its goal. HGM improves a coding agent by having the agent rewrite its own source, with every rewrite becoming a new version. It tests those versions on coding tasks and scores each by how often it succeeds, stopping after a set number of tests in total.

HGM is based on the paper [*Huxley-Gödel Machine: Human-Level Coding Agent Development by an Approximation of the Optimal Self-Improving Machine*](https://arxiv.org/abs/2510.21614) published by Wenyi Wang, Piotr Piękos, Li Nanbo, Firas Laakom, Yimeng Chen, Mateusz Ostaszewski, Mingchen Zhuge, and Jürgen Schmidhuber.

## Commands

RustyHGM is one command, `hgm`, with no sub-commands. You run it and get back a better agent. Build it, then run it from the repository root, since it looks for `config.yaml`, `Dockerfile`, `polyglot/` and `output_hgm/` relative to the working directory.

```sh
cargo build --release -p hgm
target/release/hgm [flags]
```

One run does the whole job:

1. **Fetches task data** it doesn't have yet. For `--polyglot` runs, that means cloning the [Aider Polyglot benchmark](https://github.com/Aider-AI/polyglot-benchmark) into `polyglot/polyglot-benchmark` and writing its task list to `polyglot/polyglot_benchmark_metadata.json`.
2. **Improves the agent** until the task budget is spent. On the first run with a given agent name, it also copies the seed agent and measures it on every task in the pool.
3. **Picks the best version**: the one with the highest success rate it can trust, given how many tasks it was tried on.
4. **Writes that version's source** to `output_hgm/<run>/best_agent/` and prints where it went. This folder is the improved agent: the seed with every patch on its line applied.
5. **Optionally scores it on held-out tasks** with `--holdout_split`. These are SWE-bench tasks it never practised on, so the score shows whether it really got better.

```sh
target/release/hgm --max_task_evals 100 --holdout_split Lite --holdout_tasks 50
```

`hgm --help` lists the flags, and `hgm --version` prints the version. Flag names use underscores, as in `--max_task_evals 100`, and a value can also be joined with `=`, as in `--max_task_evals=100`. If a run stops early, `--continue_from output_hgm/<run>` resumes it.

### Before a run

- **Docker.** Every build, self-edit and task runs in a container. `HGM_DOCKER` names a different Docker-compatible program.
- **A model API key.** Models whose names contain `gpt` or start with `o` use `OPENAI_API_KEY`. Every other model goes through OpenRouter with `OpenRouter_API_KEY`.
- **Python 3 with the `swebench` package** for SWE-bench grading. `HGM_SWEBENCH_PYTHON` picks the interpreter.
- **git**, for fetching the Polyglot benchmark and writing the best agent's source.

### Flags

Settings are read from `config.yaml` first, and each flag with a config key replaces that key. The defaults shown are the ones in the repository's `config.yaml`.

| Flag | Config key | Default | What it sets |
| --- | --- | --- | --- |
| `--config <file>` | | `config.yaml` | The settings file to read |
| `--max_task_evals <n>` | `execution.max_task_evals` | `800` | Total task evaluations in the run's budget |
| `--max_workers <n>` | `execution.max_workers` | `20` | Ticks and task evaluations run at the same time |
| `--self_improve_timeout <secs>` | `execution.self_improve_timeout` | `3600` | Time limit for one self-edit |
| `--evaluation_timeout <secs>` | `execution.evaluation_timeout` | `3600` | Time limit for one task attempt |
| `--self_improve_llm <model>` | `llm.self_improve_llm` | `gpt-5-mini` | Model the agent uses while it edits itself |
| `--downstream_llm <model>` | `llm.downstream_llm` | `gpt-5-mini` | Model the agent uses while it solves tasks |
| `--diagnose_llm <model>` | `llm.diagnose_llm` | `gpt-5-mini` | Model that turns a failure into a change request |
| `--alpha <x>` | `optimization.alpha` | `0.6` | How bushy the tree grows: more children per evaluation as it rises |
| `--cool_down`, `--no_cool_down` | `optimization.cool_down` | off | Sharpens every draw as the budget runs out |
| `--beta <x>` | `optimization.beta` | `1.0` | How fast `cool_down` sharpens; ignored without it |
| `--expansion_statistic <clade\|own>` | `optimization.expansion_statistic` | `clade` | Whether a parent is judged on its whole clade or on its own results |
| `--n_pseudo_descendant_evals <n>` | `optimization.n_pseudo_descendant_evals` | `10000` | Caps how many of a version's own results count in its clade pool |
| `--eval_random_level <p>` | `optimization.eval_random_level` | `1.0` | Odds of picking an untried task at random, rather than the next one in pool order |
| `--polyglot`, `--no_polyglot` | `evaluation.polyglot` | off | Uses the Polyglot benchmark instead of SWE-bench |
| `--full_eval` | `evaluation.full_eval` | off | Uses all 500 SWE-bench Verified tasks instead of the 60-task pool |
| `--output_dir <dir>` | `paths.output_dir` | `output_hgm/<timestamp>` | Where the run record goes |
| `--continue_from <dir>` | `paths.continue_from` | none | An earlier run to resume; the record continues in `output_hgm/<its folder name>` unless `--output_dir` is given |
| `--initial_agent_name <name>` | `paths.initial_agent_name` | `default_agent` | Name of the seed agent's folder and Docker image |
| `--epsilon <p>` | | `0.01` | How cautious the final pick is: each version is scored by this percentile of its Beta posterior |
| `--holdout_split <name>` | | none | After the run, scores the best agent on `princeton-nlp/SWE-bench_<name>`, such as `Lite`; not allowed with `--polyglot` |
| `--holdout_tasks <n>` | | all | Scores only the first `n` held-out tasks |

The first run with a given agent name copies the seed into `initial_swe/<name>/` or `initial_polyglot/<name>/` and measures it on every task in the pool. Later runs reuse that folder and its results, so delete it to measure a changed seed again.

The held-out check uses the `--downstream_llm` model, the `--evaluation_timeout` limit and `--max_workers`, and writes its results to `output_hgm/<run>/holdout/best_agent/`.

### Environment variables

| Variable | What it sets |
| --- | --- |
| `OPENAI_API_KEY` | Key for OpenAI models; passed into the agent's containers |
| `OpenRouter_API_KEY` | Key for every other model; passed into the agent's containers |
| `ANTHROPIC_API_KEY`, `AWS_REGION`, `AWS_REGION_NAME`, `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY` | Passed into the agent's containers unchanged |
| `OPENAI_BASE_URL` | Endpoint for OpenAI models on the host |
| `HGM_LLM_BASE_URL` | One endpoint for every model on the host |
| `HGM_SWEBENCH_PYTHON` | Python interpreter that runs `swebench` grading, `python3` by default |
| `HGM_HF_ENDPOINT` | Hugging Face mirror for datasets, `https://huggingface.co` by default |
| `HGM_DATA_DIR` | Dataset cache, `~/.cache/hgm/datasets` by default |
| `HGM_DOCKER` | Container program, `docker` by default |
| `HGM_POLYGLOT_REPO` | Clone URL for the Polyglot benchmark |

### The agent binary

The harness also builds `hgm-agent`, the agent being evolved, and runs it inside containers. It can be run by hand to watch one attempt, with `cargo run -p hgm-agent -- <flags>`.

| Flag | Default | What it sets |
| --- | --- | --- |
| `--problem_statement <text>` | required | The task or change request |
| `--git_dir <dir>` | required | The repository to work in |
| `--base_commit <sha>` | required | The commit the final diff is taken against |
| `--chat_history_file <file>` | required | Where the conversation log is written |
| `--outdir <dir>` | `/hgm/` | Where `model_patch.diff` is written |
| `--model <model>` | `gpt-5`, or `anthropic/claude-sonnet-4` with `--polyglot` | Model to use |
| `--timeout <secs>` | `3600` | Time limit for the attempt |
| `--self_improve` | off | Edits the agent's own source rather than solving a task |
| `--polyglot` | off | Uses Polyglot prompts and tests |
| `--language <lang>` | `python` | Polyglot task language: `cpp`, `java`, `python`, `go`, `rust` or `javascript` |
| `--test_description <text>` | none | How to run the repository's tests |
| `--instance_id <id>` | none | The task's ID, used for logging |

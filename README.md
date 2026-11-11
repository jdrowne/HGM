# RustyHGM

RustyHGM is a Rust port of the [Huxley-Gödel Machine](https://github.com/metauto-ai/HGM) (HGM), a self-improving machine that uses clade-level statistics to approximate the Gödel machine, a program that rewrites its own code whenever it can prove that the change will make it better at its goal. HGM improves a coding agent by having the agent rewrite its own source, with every rewrite becoming a new version. It tests those versions on coding tasks and scores each by how often it succeeds, stopping after a set number of tests in total.

HGM is based on the paper [*Huxley-Gödel Machine: Human-Level Coding Agent Development by an Approximation of the Optimal Self-Improving Machine*](https://arxiv.org/abs/2510.21614) published by Wenyi Wang, Piotr Piękos, Li Nanbo, Firas Laakom, Yimeng Chen, Mateusz Ostaszewski, Mingchen Zhuge, and Jürgen Schmidhuber.

## Commands

RustyHGM is one command, `hgm`, with no sub-commands. You run it and get back a better agent. Build it, then run it from the repository root, since it looks for `Dockerfile` and `polyglot/` relative to the working directory.

```sh
cargo build --release -p hgm
target/release/hgm [flags]
```

One run does the whole job:

1. **Fetches task data** it doesn't have yet. For `--polyglot` runs, that means cloning the [Aider Polyglot benchmark](https://github.com/Aider-AI/polyglot-benchmark) into `polyglot/polyglot-benchmark` and writing its task list to `polyglot/polyglot_benchmark_metadata.json`.
2. **Improves the agent** until the task budget is spent. On the first run with a given agent name, it also copies the seed agent and measures it on every task in the pool.
3. **Picks the best version**: the one with the highest success rate it can trust, given how many tasks it was tried on.
4. **Writes that version's source** to `best_agent/` in the `--output_dir` folder and prints where it went. This folder is the improved agent: the seed with every patch on its line applied.
5. **Optionally scores it on held-out tasks** with `--holdout_split`. These are SWE-bench tasks it never practised on, so the score shows whether it really got better.

```sh
target/release/hgm \
  --self_improve_llm gpt-5-mini --downstream_llm gpt-5-mini --diagnose_llm gpt-5-mini \
  --alpha 0.6 --beta 1.0 --cool_down false --eval_random_level 1.0 \
  --n_pseudo_descendant_evals 10000 --expansion_statistic clade --epsilon 0.01 \
  --max_workers 20 --max_task_evals 100 --self_improve_timeout 3600 --evaluation_timeout 3600 \
  --full_eval false --polyglot false \
  --output_dir runs/first --initial_agent_name default_agent \
  --holdout_split Lite --holdout_tasks 50
```

`hgm --help` lists the flags, and `hgm --version` prints the version. Flag names use underscores, as in `--max_task_evals 100`, and a value can also be joined with `=`, as in `--max_task_evals=100`. If a run stops early, `--continue_from runs/first --output_dir runs/first` resumes it in the same folder.

### Before a run

- **Docker.** Every build, self-edit and task runs in a container. `HGM_DOCKER` names a different Docker-compatible program.
- **A model API key.** Models whose names contain `gpt` or start with `o` use `OPENAI_API_KEY`. Every other model goes through OpenRouter with `OpenRouter_API_KEY`.
- **Python 3 with the `swebench` package** for SWE-bench grading. `HGM_SWEBENCH_PYTHON` picks the interpreter.
- **git**, for fetching the Polyglot benchmark and writing the best agent's source.

### Flags

Every setting is a required flag with no default, so the command line alone says what a run will do. Leaving one out stops `hgm` with an error that names it. Yes/no settings take `true` or `false`, as in `--cool_down false`.

| Flag | What it sets |
| --- | --- |
| `--self_improve_llm <model>` | Model the agent uses while it edits itself |
| `--downstream_llm <model>` | Model the agent uses while it solves tasks |
| `--diagnose_llm <model>` | Model that turns a failure into a change request |
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

The first run with a given agent name copies the seed into `initial_swe/<name>/` or `initial_polyglot/<name>/` and measures it on every task in the pool. Later runs reuse that folder and its results, so delete it to measure a changed seed again.

The held-out check uses the `--downstream_llm` model, the `--evaluation_timeout` limit and `--max_workers`, and writes its results to `holdout/best_agent/` in the `--output_dir` folder.

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

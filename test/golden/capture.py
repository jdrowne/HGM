import ast
import contextlib
import copy
import io
import json
import math
import os
import random
import sys
import tempfile
from pathlib import Path
from types import SimpleNamespace

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO))
os.chdir(REPO)

import numpy as np
import pathspec

FIXTURES = REPO / "test" / "src"


def plain(obj):
    if isinstance(obj, dict):
        return {str(k): plain(v) for k, v in obj.items()}
    if isinstance(obj, (list, tuple)):
        return [plain(v) for v in obj]
    if isinstance(obj, (np.bool_,)):
        return bool(obj)
    if isinstance(obj, (np.integer,)):
        return int(obj)
    if isinstance(obj, (float, np.floating)):
        f = float(obj)
        if math.isnan(f):
            return "NaN"
        if math.isinf(f):
            return "Infinity" if f > 0 else "-Infinity"
        return f
    return obj


def write(crate, name, source, cases):
    path = FIXTURES / crate / "unit" / "fixtures" / f"{name}.json"
    path.parent.mkdir(parents=True, exist_ok=True)
    doc = {"source": source, "cases": plain(cases)}
    text = json.dumps(doc, indent=2, ensure_ascii=False, allow_nan=False) + "\n"
    path.write_text(text)
    print(f"wrote {path.relative_to(REPO)} ({len(cases)} cases)")


def write_text(crate, rel, text):
    path = FIXTURES / crate / "unit" / "fixtures" / rel
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text)
    print(f"wrote {path.relative_to(REPO)}")


def quiet(fn, *args, **kwargs):
    buf = io.StringIO()
    with contextlib.redirect_stdout(buf):
        result = fn(*args, **kwargs)
    return result, buf.getvalue()


def untmp(obj, tmp):
    tmp = str(tmp)
    if isinstance(obj, str):
        return obj.replace(os.path.realpath(tmp), "<TMP>").replace(tmp, "<TMP>")
    if isinstance(obj, dict):
        return {k: untmp(v, tmp) for k, v in obj.items()}
    if isinstance(obj, (list, tuple)):
        return [untmp(v, tmp) for v in obj]
    return obj


@contextlib.contextmanager
def sorted_listdir(*modules):
    real = os.listdir
    fake = SimpleNamespace(**{k: getattr(os, k) for k in dir(os) if not k.startswith("__")})
    fake.listdir = lambda p=".": sorted(real(p))
    saved = [(m, m.os) for m in modules]
    for m in modules:
        m.os = fake
    try:
        yield
    finally:
        for m, o in saved:
            m.os = o


def capture_config():
    import config as cfg

    cases = []
    yamls = {
        "shipped_config_yaml": (REPO / "config.yaml").read_text(),
        "empty_file": "",
        "null_document": "null\n",
        "partial_optimization": "optimization:\n  alpha: 0.3\n  cool_down: true\n",
        "partial_paths_null_output_dir": "paths:\n  output_dir: null\n  initial_agent_name: agent_x\n",
        "unknown_key_falls_back_to_defaults": "optimization:\n  alpha: 0.3\n  gamma: 2\n",
        "invalid_yaml_falls_back_to_defaults": "llm: [unclosed\n",
        "int_where_float_expected": "optimization:\n  alpha: 1\n  beta: 2\n",
        "string_where_int_expected": "execution:\n  max_workers: '8'\n",
    }
    with tempfile.TemporaryDirectory() as tmp:
        for name, text in yamls.items():
            p = Path(tmp) / "c.yaml"
            p.write_text(text)
            conf, _ = quiet(cfg.HGMConfig.from_yaml, str(p))
            cases.append({"name": name, "input": {"yaml": text}, "output": conf.to_dict()})
        conf, _ = quiet(cfg.HGMConfig.from_yaml, str(Path(tmp) / "missing.yaml"))
        cases.append({"name": "missing_file", "input": {"yaml": None}, "output": conf.to_dict()})

        p = Path(tmp) / "c.yaml"
        p.write_text((REPO / "config.yaml").read_text())
        override_sets = {
            "cli_style_overrides": {
                "execution.max_task_evals": 50,
                "execution.max_workers": 4,
                "paths.output_dir": "out/run1",
                "optimization.alpha": 0.7,
                "optimization.cool_down": True,
                "evaluation.polyglot": True,
                "paths.initial_agent_name": "default_agent",
            },
            "none_values_are_skipped": {"optimization.alpha": None, "llm.self_improve_llm": None},
            "unknown_paths_are_ignored": {"nope.alpha": 1, "optimization.nope": 2},
            "false_is_applied": {"optimization.cool_down": False, "evaluation.full_eval": False},
            "section_name_replaces_whole_section": {"optimization": 3},
        }
        for name, ov in override_sets.items():
            conf, _ = quiet(cfg.load_config, str(p), **ov)
            try:
                out = conf.to_dict()
            except Exception as e:
                out = {"error": type(e).__name__}
            cases.append(
                {
                    "name": name,
                    "input": {"yaml": "<shipped config.yaml>", "overrides": ov},
                    "output": out,
                }
            )

        out = Path(tmp) / "saved.yaml"
        cfg.HGMConfig().save_to_yaml(str(out))
        write_text("core", "config/defaults_saved.yaml", out.read_text())

    write(
        "core",
        "config",
        "config.py: HGMConfig.from_yaml, load_config (overrides), to_dict",
        cases,
    )


TREES = {
    "root_only_no_evals": [("initial", None, [])],
    "root_only_with_evals": [("initial", None, [1, 0, 1])],
    "chain": [
        ("initial", None, [1, 0]),
        ("c1", 0, [1, 1, 0]),
        ("c2", 1, [0]),
    ],
    "branching": [
        ("initial", None, [1, 0, 0, 1]),
        ("a", 0, [1]),
        ("b", 0, [0, 0]),
        ("a1", 1, [1, 1, 1]),
        ("a2", 1, []),
        ("b1", 2, [1, 0, 1, 0, 1]),
    ],
}


def build_tree(spec):
    import hgm_utils
    import tree

    hgm_utils.nodes = {}
    nodes = []
    for commit, parent, evals in spec:
        n = tree.Node(commit, utility_measures=list(evals), parent_id=parent)
        if parent is not None:
            hgm_utils.nodes[parent].add_child(n)
        nodes.append(n)
    return hgm_utils.nodes


def capture_tree():
    cases = []
    for name, spec in TREES.items():
        nodes = build_tree(spec)
        per_node = {}
        for nid, node in nodes.items():
            desc = {}
            for k in (0, 1, 2, 3, 10, 10000):
                fresh = build_tree(spec)
                desc[str(k)] = list(fresh[nid].get_decendant_evals(num_pseudo=k))
            nodes = build_tree(spec)
            node = nodes[nid]
            per_node[str(nid)] = {
                "num_evals": node.num_evals,
                "mean_utility": node.mean_utility,
                "sub_tree_ids": [n.id for n in node.get_sub_tree()],
                "save_as_dict": node.save_as_dict(),
                "decendant_evals_by_num_pseudo": desc,
            }
        cases.append(
            {
                "name": name,
                "input": [{"commit_id": c, "parent_id": p, "utility_measures": e} for c, p, e in spec],
                "output": per_node,
            }
        )
    write(
        "core",
        "tree",
        "tree.py: Node (num_evals, mean_utility, get_sub_tree, save_as_dict, get_decendant_evals)",
        cases,
    )


def load_ts_sample():
    src = (REPO / "hgm.py").read_text()
    mod = ast.parse(src)
    main = next(n for n in mod.body if isinstance(n, ast.FunctionDef) and n.name == "main")
    fn = next(n for n in ast.walk(main) if isinstance(n, ast.FunctionDef) and n.name == "TS_sample")
    return compile(ast.Module(body=[fn], type_ignores=[]), "hgm.py:TS_sample", "exec")


def capture_ts_sample():
    code = load_ts_sample()
    cases = []
    evals = [[1, 0, 1], [0, 0], [], [1, 1, 1, 1], [0.5, 1]]
    settings = [
        ("no_cool_down", False, 1.0, 800, 0),
        ("no_cool_down_midway", False, 1.0, 800, 400),
        ("cool_down_start", True, 1.0, 800, 0),
        ("cool_down_midway", True, 1.0, 800, 400),
        ("cool_down_beta_2", True, 2.0, 800, 600),
        ("cool_down_beta_half", True, 0.5, 800, 799),
        ("cool_down_at_budget", True, 1.0, 800, 800),
    ]
    for name, cool_down, beta, max_evals, n_evals in settings:
        recorded = {}

        def fake_beta(a, b):
            recorded["alphas"] = [float(x) for x in np.asarray(a, dtype=float)]
            recorded["betas"] = [float(x) for x in np.asarray(b, dtype=float)]
            return np.array([1.0] * len(recorded["alphas"]))

        ns = {
            "np": SimpleNamespace(
                sum=np.sum, array=np.array, argmax=np.argmax, random=SimpleNamespace(beta=fake_beta)
            ),
            "opt_cfg": SimpleNamespace(cool_down=cool_down, beta=beta),
            "exec_cfg": SimpleNamespace(max_task_evals=max_evals),
            "hgm_utils": SimpleNamespace(n_task_evals=n_evals),
        }
        exec(code, ns)
        idx = ns["TS_sample"](copy.deepcopy(evals))
        cases.append(
            {
                "name": name,
                "input": {
                    "evals": evals,
                    "cool_down": cool_down,
                    "beta": beta,
                    "max_task_evals": max_evals,
                    "n_task_evals": n_evals,
                },
                "output": {**recorded, "argmax_when_all_thetas_equal": int(idx)},
            }
        )
    write(
        "core",
        "ts_sample",
        "hgm.py: main.TS_sample, with np.random.beta replaced by a recorder",
        cases,
    )


def capture_run_metadata():
    import hgm
    import hgm_utils
    from utils import evo_utils

    with tempfile.TemporaryDirectory() as tmp:
        build_tree(TREES["branching"])
        hgm_utils.init_evaluated_tasks = ["t1", "t2"]
        hgm.update_metadata(tmp, 7)
        hgm_utils.nodes[5].utility_measures.append(1)
        hgm_utils.init_evaluated_tasks = ["t1", "t2", "t3"]
        hgm.update_metadata(tmp, 8)
        jsonl = (Path(tmp) / "hgm_metadata.jsonl").read_text()
        init = (Path(tmp) / "init_evaluated_tasks.json").read_text()
        write_text("core", "run_metadata/hgm_metadata.jsonl", jsonl)
        write_text("core", "run_metadata/init_evaluated_tasks.json", init)

        loaded_all = evo_utils.load_hgm_metadata(str(Path(tmp) / "hgm_metadata.jsonl"))
        loaded_last = evo_utils.load_hgm_metadata(str(Path(tmp) / "hgm_metadata.jsonl"), last_only=True)

    write(
        "core",
        "run_metadata",
        "hgm.py: update_metadata; utils/evo_utils.py: load_hgm_metadata",
        [
            {
                "name": "branching_tree_two_updates",
                "input": {
                    "tree": "branching",
                    "updates": [
                        {"n_task_evals": 7, "init_evaluated_tasks": ["t1", "t2"]},
                        {
                            "n_task_evals": 8,
                            "append_eval_to_node": 5,
                            "init_evaluated_tasks": ["t1", "t2", "t3"],
                        },
                    ],
                },
                "output": {
                    "jsonl_file": "run_metadata/hgm_metadata.jsonl",
                    "init_evaluated_tasks_file": "run_metadata/init_evaluated_tasks.json",
                    "load_all": loaded_all,
                    "load_last_only": loaded_last,
                },
            }
        ],
    )


def capture_evo_utils():
    from utils import evo_utils

    cases = []
    with tempfile.TemporaryDirectory() as tmp:
        d = Path(tmp) / "results"
        d.mkdir()
        files = {
            "abc_0.json": {
                "resolved_instances": 2,
                "submitted_instances": 3,
                "resolved_ids": ["r1", "r2"],
                "unresolved_ids": ["u1"],
                "empty_patch_ids": [],
                "submitted_ids": ["r1", "r2", "u1"],
            },
            "abc_1.json": {
                "resolved_instances": 0,
                "submitted_instances": 1,
                "empty_patch_ids": ["e1"],
                "submitted_ids": ["e1"],
            },
            "abc_2.json": {"note": "no counts at all"},
            "other_0.json": {"resolved_instances": 5, "submitted_instances": 5},
            "abc_notes.txt": "ignored, not .json",
        }
        for fname, content in files.items():
            (d / fname).write_text(json.dumps(content) if isinstance(content, dict) else content)
        with sorted_listdir(evo_utils):
            for kw in ("abc", "other", "missing"):
                (per_file, overall), _ = quiet(evo_utils.get_all_performance, kw, results_dir=str(d))
                cases.append(
                    {
                        "name": f"get_all_performance_{kw}",
                        "input": {"files": files, "run_keyword": kw},
                        "output": {"performance_results": per_file, "overall_performance": overall},
                    }
                )

        run = Path(tmp) / "run"
        chain = {"c1": "initial", "c2": "c1", "c3": "c2", "nopatch": "c1"}
        for commit, parent in chain.items():
            (run / commit).mkdir(parents=True)
            (run / commit / "metadata.json").write_text(json.dumps({"parent_commit": parent}))
            if commit != "nopatch":
                (run / commit / "model_patch.diff").write_text(f"patch {commit}\n")
        for start in ("initial", "c1", "c3", "nopatch"):
            paths, out = quiet(evo_utils.get_model_patch_paths, tmp, "run", start)
            cases.append(
                {
                    "name": f"get_model_patch_paths_{start}",
                    "input": {"parent_links": chain, "no_patch_file": ["nopatch"], "parent_commit": start},
                    "output": untmp(paths, tmp),
                }
            )

    perf = {
        "accuracy_score": 0.5,
        "total_resolved_ids": ["a"],
        "total_unresolved_ids": ["b"],
        "total_emptypatch_ids": [],
        "total_submitted_instances": 2,
    }
    compiled_cases = {
        "ok": ({"overall_performance": perf}, [2]),
        "fewer_than_expected": ({"overall_performance": perf}, [3]),
        "missing_key": ({"overall_performance": {k: v for k, v in perf.items() if k != "accuracy_score"}}, [1]),
        "no_overall_performance": ({}, [1]),
        "only_empty_patches": (
            {"overall_performance": {**perf, "total_resolved_ids": [], "total_unresolved_ids": []}},
            [1],
        ),
    }
    for name, (md, n) in compiled_cases.items():
        res, _ = quiet(evo_utils.is_compiled_self_improve, md, num_swe_issues=n)
        cases.append(
            {"name": f"is_compiled_self_improve_{name}", "input": {"metadata": md, "num_swe_issues": n}, "output": res}
        )

    write(
        "core",
        "evo_utils",
        "utils/evo_utils.py: get_all_performance, get_model_patch_paths, is_compiled_self_improve",
        cases,
    )


def capture_self_improvement_prompts():
    from prompts import self_improvement_prompt as sip

    cases = []
    for poly in (False, True):
        cases.append(
            {
                "name": f"get_problem_description_prompt_polyglot_{poly}",
                "input": {
                    "response_json": {"implementation_suggestion": "IMPL", "problem_description": "DESC"},
                    "is_polyglot": poly,
                },
                "output": sip.get_problem_description_prompt(
                    {"implementation_suggestion": "IMPL", "problem_description": "DESC"}, is_polyglot=poly
                ),
            }
        )
    long_log = "x" * 100005
    for name, args in {
        "all_empty": ([], [], [], []),
        "first_of_each": (["md1", "md2"], ["ev1"], ["p1"], ["r1"]),
        "long_md_log_clipped": ([long_log], [], [], []),
    }.items():
        out = sip.process_selfimprove_eval_logs(*args)
        if name == "long_md_log_clipped":
            args = (["x" * 100005 + " (100005 x characters)"], [], [], [])
            out = list(out)
            out[0] = {"len": len(out[0]), "tail": out[0][-20:]}
        cases.append({"name": f"process_selfimprove_eval_logs_{name}", "input": list(args), "output": out})

    status_cases = {
        "all_pass": {"FAIL_TO_PASS": {"success": ["t1", "t2"], "failure": []}, "PASS_TO_PASS": {"success": ["p1"], "failure": []}},
        "some_fail": {
            "FAIL_TO_PASS": {"success": ["t1"], "failure": ["t2"]},
            "PASS_TO_PASS": {"success": ["p1", "p2"], "failure": ["p3"]},
        },
        "empty": {},
        "null_and_empty_string": {"FAIL_TO_PASS": {"success": None, "failure": ""}, "PASS_TO_PASS": {"success": {}, "failure": None}},
        "strings_and_objects": {"FAIL_TO_PASS": {"success": "ab", "failure": {"k1": 1, "k2": 2}}, "PASS_TO_PASS": {"success": {"k": 1}, "failure": "xyz"}},
    }
    for name, ts in status_cases.items():
        eval_json = {"inst-1": {"tests_status": ts}}
        cases.append({"name": f"get_eval_log_text_{name}", "input": eval_json, "output": sip.get_eval_log_text(eval_json)})
    eval_json = {"inst-1": {}}
    cases.append({"name": "get_eval_log_text_no_tests_status", "input": eval_json, "output": sip.get_eval_log_text(eval_json)})

    with tempfile.TemporaryDirectory() as tmp:
        p = Path(tmp) / "log.md"
        text = "line one\nError in get_response_withtools: boom\n  Error in get_response_withtools: indented\nlast\n\n"
        p.write_text(text)
        for flt in (True, False):
            cases.append(
                {"name": f"read_mdlog_file_filter_{flt}", "input": {"text": text, "filter": flt}, "output": sip.read_mdlog_file(str(p), filter=flt)}
            )

    templates = {
        k: v
        for k, v in vars(sip).items()
        if isinstance(v, str) and not k.startswith("_") and len(v) > 40
    }
    for k in sorted(templates):
        write_text("core", f"prompts/{k}.txt", templates[k])

    write(
        "core",
        "self_improvement_prompt",
        "prompts/self_improvement_prompt.py: helpers; module-level templates in prompts/*.txt",
        cases,
    )


def capture_context_length():
    import hgm_utils

    err = "Error in get_response_withtools: Error code: 400 - {'message': 'Input is too long for requested model.'}"
    err2 = (
        "Error in get_response_withtools: Error code: 400 - {'object': 'error', 'message': \"This model's maximum "
        "context length is 32768 tokens. However, you requested 40000 tokens in the messages, Please reduce the "
        "length of the messages. None\", 'type': 'BadRequestError', 'param': None, 'code': 400}"
    )
    logs = {
        "twice_in_a_row": f"start\n{err}\n{err}\nend",
        "only_once": f"start\n{err}\nend",
        "twice_not_adjacent": f"{err}\nother\n{err}",
        "vllm_style_twice": f"{err2}\n{err2}\n",
        "none": "all good",
    }
    cases = []
    with tempfile.TemporaryDirectory() as tmp:
        for name, log in logs.items():
            pred = Path(tmp) / name / "predictions" / "run0"
            pred.mkdir(parents=True)
            (pred / "inst-1.md").write_text(log)
            hit = hgm_utils.any_exceeding_context_length(tmp, name, ["inst-1"])
            cases.append({"name": name, "input": {"md_log": log}, "output": hit})
    write(
        "core",
        "context_length",
        "hgm_utils.py: any_exceeding_context_length",
        cases,
    )


PYTEST_LOG = """\
============================= test session starts ==============================
collected 6 items

tests/test_a.py::test_one PASSED                                         [ 16%]
PASSED tests/test_a.py::test_one
PASSED tests/test_a.py::test_param[1-2]
FAILED tests/test_a.py::test_two - AssertionError: boom
ERROR tests/test_b.py::test_three - fixture 'x' not found
SKIPPED [1] tests/test_c.py:12: no reason
XFAIL tests/test_c.py::test_xfail - known
PASSED
FAILED
=========================== short test summary info ============================
"""

PYTEST_OPTIONS_LOG = """\
PASSED tests/test_x.py::test_path[/usr/lib/python3/site-packages/foo]
PASSED tests/test_x.py::test_path[//double/slash]
PASSED tests/test_x.py::test_path[/glob/*/star]
FAILED tests/test_x.py::test_plain - ValueError
PASSED tests/test_x.py::test_opt[a-b]
"""

DJANGO_LOG = """\
test_one (app.tests.TestA) ... ok
test_two (app.tests.TestA) ... OK
test_three (app.tests.TestA) ...  OK
test_skip (app.tests.TestA) ... skipped 'no db'
test_fail (app.tests.TestA) ... FAIL
test_err (app.tests.TestA) ... ERROR
FAIL: test_fail2 (app.tests.TestB)
ERROR: test_err2 (app.tests.TestB)
test_multi (app.tests.TestC) ... some output
ok
--version is equivalent to version
test_sys (app.tests.TestD) ... System check identified no issues (0 silenced)
ok
test_ise (app.tests.TestE) ... Internal Server Error: /path/x/
ok
Applying sites.0002_alter_domain_unique...test_no_migrations (migrations.test.T) ... ok
"""

PYTEST_V2_LOG = (
    "\x1b[32mPASSED\x1b[0m tests/test_a.py::test_color\n"
    "FAILED tests/test_a.py::test_f - boom\n"
    "tests/test_old.py::test_old PASSED\n"
    "tests/test_old.py::test_old_fail FAILED\n"
    "PASSED\x07 tests/test_a.py::test_bell\n"
)

SEABORN_LOG = """\
FAILED tests/test_s.py::test_fail - err
tests/test_s.py::test_pass PASSED [ 50%]
PASSED tests/test_s.py::test_pass2
something PASSED in middle
"""

SYMPY_LOG = """\
________________ sympy/core/tests/test_basic.py:test_fail ________________
test_ok ok
test_err E
test_f F
test_with_marker ok [OK]
test_with_fail F [FAIL]
  test_indented ok
"""

MATPLOTLIB_LOG = """\
PASSED lib/matplotlib/tests/test_x.py::test_click[MouseButton.LEFT]
FAILED lib/matplotlib/tests/test_x.py::test_click[MouseButton.RIGHT] - err
PASSED
"""


def capture_log_parsers():
    from utils import swe_log_parsers as slp

    parsers = {
        "parse_log_pytest": PYTEST_LOG,
        "parse_log_pytest_options": PYTEST_OPTIONS_LOG,
        "parse_log_django": DJANGO_LOG,
        "parse_log_pytest_v2": PYTEST_V2_LOG,
        "parse_log_seaborn": SEABORN_LOG,
        "parse_log_sympy": SYMPY_LOG,
        "parse_log_matplotlib": MATPLOTLIB_LOG,
    }
    cases = []
    for fname, log in parsers.items():
        fn = getattr(slp, fname)
        for lname, l in {"sample": log, "empty": ""}.items():
            try:
                out = {"ok": fn(l)}
            except Exception as e:
                out = {"error": type(e).__name__}
            cases.append({"name": f"{fname}_{lname}", "input": {"parser": fname, "log": l}, "output": out})
    for fname, l in {"parse_log_pytest_v2": "PASSED\n", "parse_log_seaborn": "x PASSED \n"}.items():
        try:
            out = {"ok": getattr(slp, fname)(l)}
        except Exception as e:
            out = {"error": type(e).__name__}
        cases.append({"name": f"{fname}_crash", "input": {"parser": fname, "log": l}, "output": out})

    repo_map = {repo: fn.__name__ for repo, fn in slp.MAP_REPO_TO_PARSER.items()}
    cases.append({"name": "map_repo_to_parser", "input": None, "output": repo_map})
    write(
        "agent",
        "swe_log_parsers",
        "utils/swe_log_parsers.py",
        cases,
    )


def capture_eval_utils():
    from utils import eval_utils

    cases = []
    for iid, log in {
        "hgm": PYTEST_LOG,
        "django__django-11099": DJANGO_LOG,
        "scikit-learn__scikit-learn-12421": PYTEST_V2_LOG,
        "pvlib__pvlib-python-1072": PYTEST_LOG,
        "unknown__repo-1": PYTEST_LOG,
        "nohyphen": PYTEST_LOG,
    }.items():
        cases.append(
            {"name": f"parse_eval_output_{iid}", "input": {"instance_id": iid, "eval_output": log}, "output": eval_utils.parse_eval_output(iid, log)}
        )
    for name, rep in {
        "half": {"a": "PASSED", "b": "FAILED"},
        "empty": {},
        "xfail_not_passed": {"a": "XFAIL", "b": "PASSED", "c": "PASSED"},
    }.items():
        cases.append({"name": f"get_report_score_{name}", "input": rep, "output": eval_utils.get_report_score(rep)})

    hist_openai = [
        {"role": "user", "content": "do it"},
        {"type": "function_call_output", "call_id": "c1", "output": "Tool Result? no: " + PYTEST_LOG},
        {"type": "function_call_output", "call_id": "c2", "output": "nothing parseable"},
        {"role": "assistant", "content": "done"},
    ]
    cases.append(
        {
            "name": "msg_history_to_report_openai",
            "input": {"instance_id": "hgm", "model": "gpt-5", "msg_history": hist_openai},
            "output": eval_utils.msg_history_to_report("hgm", hist_openai, model="gpt-5"),
        }
    )
    hist_or = [
        {"role": "user", "content": "do it"},
        {"role": "tool", "content": PYTEST_LOG},
        {"role": "tool", "content": "no tests here"},
    ]
    cases.append(
        {
            "name": "msg_history_to_report_open_router",
            "input": {"instance_id": "hgm", "model": "anthropic/claude-sonnet-4", "msg_history": hist_or},
            "output": eval_utils.msg_history_to_report("hgm", hist_or, model="anthropic/claude-sonnet-4"),
        }
    )
    write(
        "agent",
        "eval_utils",
        "utils/eval_utils.py: parse_eval_output, get_report_score, msg_history_to_report",
        cases,
    )


def capture_llm_json():
    from llm import extract_json_between_markers as ex

    inputs = {
        "fenced": 'text\n```json\n{"a": 1, "b": [1, 2]}\n```\nmore',
        "fenced_indented_markers": '  ```json\n{"a": 1}\n  ```',
        "fenced_invalid_then_none": "```json\n{not json}\n```",
        "fenced_control_chars": '```json\n{"a": "x\ty"}\n```',
        "fenced_two_blocks_first_wins": '```json\n{"a": 1}\n```\n```json\n{"a": 2}\n```',
        "fenced_unclosed": '```json\n{"a": 1}\n',
        "fenced_empty_falls_back": '```json\n```\nthen {"z": 9}',
        "fallback_bare_object": 'Answer: {"k": "v"} end',
        "fallback_nested_object_fails_then_next": '{"a": {"b": 1}} and {"c": 2}',
        "fallback_control_chars": '{"a": "line\nbreak"}',
        "no_json": "nothing here",
        "fallback_skips_invalid": '{bad} {"ok": true}',
    }
    cases = [{"name": k, "input": v, "output": ex(v)} for k, v in inputs.items()]
    write(
        "agent",
        "extract_json",
        "llm.py: extract_json_between_markers",
        cases,
    )


def capture_patch_filters():
    from utils.git_utils import filter_patch_by_files, remove_patch_by_files
    from swe_bench.report import remove_patches_to_tests

    patch = """\
diff --git a/src/affine_cipher.py b/src/affine_cipher.py
index 1..2 100644
--- a/src/affine_cipher.py
+++ b/src/affine_cipher.py
@@ -1 +1 @@
-old
+new
diff --git a/tests/test_affine.py b/tests/test_affine.py
--- a/tests/test_affine.py
+++ b/tests/test_affine.py
@@ -1 +1 @@
-t
+u
diff --git a/polyglot/harness.py b/polyglot/harness.py
--- a/polyglot/harness.py
+++ b/polyglot/harness.py
@@ -1 +1 @@
-h
+i
diff --git a/pkg/testing/x.py b/pkg/testing/x.py
+x
diff --git a/pkg/test_y.py b/pkg/test_y.py
+y
diff --git a/tox.ini b/tox.ini
+z
diff --git a/pkg/tox.ini b/pkg/tox.ini
+w
"""
    cases = []
    for targets in (["affine_cipher.py"], ["harness.py", "x.py"], ["nothing.py"], [], ["src/affine_cipher.py"], ["polyglot/harness.py", "tox.ini"]):
        cases.append(
            {"name": f"filter_patch_by_files_{'_'.join(targets) or 'none'}", "input": {"patch": patch, "target_files": targets}, "output": filter_patch_by_files(patch, targets)}
        )
    for kw in ("polyglot", "POLYGLOT", "test"):
        cases.append(
            {"name": f"remove_patch_by_files_{kw}", "input": {"patch": patch, "keyword": kw}, "output": remove_patch_by_files(patch, kw)}
        )
    cases.append({"name": "remove_patches_to_tests", "input": {"patch": patch}, "output": remove_patches_to_tests(patch)})
    cases.append({"name": "remove_patches_to_tests_crlf", "input": {"patch": patch.replace("\n", "\r\n")}, "output": remove_patches_to_tests(patch.replace("\n", "\r\n"))})
    write(
        "agent",
        "patch_filters",
        "utils/git_utils.py: filter_patch_by_files, remove_patch_by_files; swe_bench/report.py: remove_patches_to_tests",
        cases,
    )


def capture_tool_conversion():
    from llm_withtools import convert_tool_info, convert_msg_history
    from tools import bash, edit

    cases = []
    for model in ("gpt-5", "o3", "vllm-qwen", "anthropic/claude-sonnet-4"):
        for mod in (bash, edit):
            info = mod.tool_info()
            cases.append(
                {
                    "name": f"convert_tool_info_{info['name']}_{model}",
                    "input": {"model": model, "tool_info": info},
                    "output": convert_tool_info(copy.deepcopy(info), model=model),
                }
            )
    nested = {
        "name": "nested",
        "description": "d",
        "input_schema": {
            "type": "object",
            "properties": {
                "a": {"type": "string"},
                "b": {"type": ["integer", "string"]},
                "c": {"type": "object", "properties": {"x": {"type": "string"}}},
            },
            "required": ["a"],
        },
    }
    cases.append(
        {"name": "convert_tool_info_nested_gpt", "input": {"model": "gpt-5", "tool_info": nested}, "output": convert_tool_info(copy.deepcopy(nested), model="gpt-5")}
    )
    write(
        "agent",
        "tool_conversion",
        "llm_withtools.py: convert_tool_info",
        cases,
    )

    fn = SimpleNamespace(name="bash", arguments='{"command": "ls"}')
    tc = SimpleNamespace(name="bash", function=fn)
    hcases = []
    openai_hist = [
        {"role": "user", "content": "hi"},
        {"type": "function_call_output", "call_id": "x", "output": "out"},
        {"type": "function_call_output", "call_id": "y"},
        "ResponseFunctionToolCall(name='bash')",
    ]
    hcases.append({"name": "openai", "input": {"model": "gpt-5", "msg_history": openai_hist}, "output": convert_msg_history(openai_hist, model="gpt-5")})
    or_hist = [
        {"role": "user", "content": "hi"},
        {"role": "tool", "content": "out"},
        {"role": "assistant", "content": None},
        {"content": "no role"},
    ]
    hcases.append({"name": "open_router", "input": {"model": "qwen", "msg_history": or_hist}, "output": convert_msg_history(or_hist, model="qwen")})
    with_calls = [{"role": "assistant", "tool_calls": [tc]}]
    hcases.append(
        {
            "name": "open_router_tool_calls",
            "input": {"model": "qwen", "msg_history": [{"role": "assistant", "tool_calls": [{"name": "bash", "function": {"arguments": '{"command": "ls"}'}}]}]},
            "output": convert_msg_history(with_calls, model="qwen"),
        }
    )
    write(
        "agent",
        "msg_history",
        "llm_withtools.py: convert_msg_history (openai and open_router)",
        hcases,
    )


def capture_tools():
    from tools import bash, edit

    cases = [
        {"name": "bash_tool_info", "input": None, "output": bash.tool_info()},
        {"name": "editor_tool_info", "input": None, "output": edit.tool_info()},
    ]
    for name, content in {
        "simple": "line 1\nline 2",
        "trailing_newline": "a\nb\n",
        "tabs": "a\tb\n\tc",
        "empty": "",
        "long_truncated": "y" * 10010,
    }.items():
        out = edit.format_output(content, "/repo/f.py")
        if name == "long_truncated":
            content = "y * 10010"
            out = {"len": len(out), "tail": out[-40:]}
        cases.append({"name": f"format_output_{name}", "input": content, "output": out})

    with tempfile.TemporaryDirectory() as tmp:
        f = Path(tmp) / "f.txt"
        f.write_text("x\ny\n")
        calls = [
            ("view_file", "view", str(f), None),
            ("view_missing", "view", str(Path(tmp) / "nope"), None),
            ("relative_path", "view", "rel/path", None),
            ("create_existing", "create", str(f), "new"),
            ("create_new", "create", str(Path(tmp) / "g.txt"), "hello"),
            ("create_without_text", "create", str(Path(tmp) / "h.txt"), None),
            ("edit_file", "edit", str(f), "replaced"),
            ("edit_missing", "edit", str(Path(tmp) / "nope"), "x"),
            ("edit_directory", "edit", tmp, "x"),
            ("edit_without_text", "edit", str(f), None),
            ("unknown_command", "delete", str(f), None),
        ]
        for name, cmd, path, text in calls:
            out = edit.tool_function(cmd, path, file_text=text)
            cases.append(
                {"name": f"edit_tool_function_{name}", "input": untmp({"command": cmd, "path": path, "file_text": text}, tmp), "output": untmp(out, tmp)}
            )
        cases.append({"name": "edit_tool_function_files_after", "input": None, "output": {p.name: p.read_text() for p in sorted(Path(tmp).iterdir())}})

    for name, err in {
        "plain": "bash: foo: command not found\n",
        "ioctl": "bash: cannot set terminal process group (-1): Inappropriate ioctl for device\nbash: no job control in this shell\nx\ny\nreal error\nlast",
        "ioctl_with_exit": "a\nInappropriate ioctl for device\nb\nc\n<<exit>>\nd\ne\nf",
        "empty": "",
    }.items():
        cases.append({"name": f"bash_filter_error_{name}", "input": err, "output": bash.filter_error(err)})

    from prompts import testrepo_prompt, tooluse_prompt

    eval_script = "#!/bin/bash\nset -uxo pipefail\ncd /testbed\npytest -rA tests/test_a.py tests/test_b.py\necho done\n"
    eval_script_dotted = "#!/bin/bash\n./tests/runtests.py --verbosity 2 admin_views.tests\necho done\n"
    for name, s in {"pytest": eval_script, "django_dotted": eval_script_dotted}.items():
        cases.append({"name": f"get_test_command_{name}", "input": s, "output": list(testrepo_prompt.get_test_command(s))})
        cases.append({"name": f"get_test_description_swerepo_{name}", "input": s, "output": testrepo_prompt.get_test_description(s, swerepo=True)})
    cases.append({"name": "get_test_description_polyglot", "input": "cargo test", "output": testrepo_prompt.get_test_description("cargo test", polyglot=True)})
    cases.append({"name": "get_test_description_hgm", "input": "", "output": testrepo_prompt.get_test_description()})
    cases.append({"name": "get_tooluse_prompt", "input": None, "output": tooluse_prompt.get_tooluse_prompt()})
    write(
        "agent",
        "tools",
        "tools/bash.py, tools/edit.py, prompts/testrepo_prompt.py, prompts/tooluse_prompt.py",
        cases,
    )


def capture_dockerignore():
    from utils import docker_utils

    cases = []
    patterns, _ = quiet(docker_utils.read_dockerignore, str(REPO / ".dockerignore"))
    cases.append({"name": "read_dockerignore_repo", "input": (REPO / ".dockerignore").read_text(), "output": patterns})

    spec_lines = [
        "# comment",
        "<!-- html comment -->",
        "logs/",
        "*.pyc",
        "/top_only.txt",
        "docs/**/*.md",
        "!docs/keep.md",
        "build",
        "a/b/",
        "**/cache/",
        "  spaced.txt  ",
        "",
    ]
    files = [
        "main.py",
        "main.pyc",
        "logs/a.log",
        "src/logs/b.log",
        "top_only.txt",
        "sub/top_only.txt",
        "docs/x.md",
        "docs/deep/y.md",
        "docs/keep.md",
        "docs/readme.txt",
        "build/out.o",
        "build",
        "src/build/x",
        "a/b/c.txt",
        "x/a/b/c.txt",
        "deep/cache/z",
        "cache/z",
        "spaced.txt",
        ".hidden/f",
    ]
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp) / "root"
        for f in files:
            if f == "build":
                continue
            p = root / f
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text(f)
        (root / ".dockerignore").write_text("\n".join(spec_lines) + "\n")
        pats, _ = quiet(docker_utils.read_dockerignore, str(root / ".dockerignore"))
        spec = pathspec.PathSpec.from_lines("gitwildmatch", pats)
        kept = sorted(docker_utils.get_files_respecting_dockerignore(str(root), spec))
        cases.append({"name": "read_dockerignore_synthetic", "input": "\n".join(spec_lines) + "\n", "output": pats})
        cases.append({"name": "files_respecting_dockerignore", "input": {"patterns": pats, "files": files}, "output": kept})
        matches = {path: spec.match_file(path) for path in files + ["logs/", "docs/", "a/b/", "src/logs/"]}
        cases.append({"name": "gitwildmatch_match_file", "input": {"patterns": pats}, "output": matches})

    probes = ["a ", "a", "x", "ax", "bx", "]x", "a/x", "a/q/b", "a/q/r/b", "d/", "d/e", "e", "a]x", "z", "-"]
    for pattern in ["a\\ ", "*/", "a/*/b", "[]a]x", "[!a]x", "[^a]x", "[z-a]", "[abc", "ab\\", "/", "!", "!a", "**/", "/**"]:
        try:
            spec = pathspec.PathSpec.from_lines("gitwildmatch", [pattern])
            output = {path: spec.match_file(path) for path in probes}
        except Exception:
            output = "error"
        cases.append({"name": "gitwildmatch_edge", "input": pattern, "output": output})

    write(
        "sandbox",
        "dockerignore",
        "utils/docker_utils.py: read_dockerignore, get_files_respecting_dockerignore (pathspec gitwildmatch)",
        cases,
    )


def capture_task_order():
    from utils.common_utils import load_json_file

    tasks = load_json_file("./src/bench/subsets/swe/small.json") + load_json_file("./src/bench/subsets/swe/medium.json")
    random.seed(42)
    random.shuffle(tasks)
    poly = load_json_file("./src/bench/subsets/polyglot/medium.json") + load_json_file("./src/bench/subsets/polyglot/small.json")

    import hgm_utils

    hgm_utils.init(False, "", tasks + tasks[:3])
    swe_dedup = list(hgm_utils.total_tasks)
    hgm_utils.init(True, "", poly)
    poly_dedup = list(hgm_utils.total_tasks)

    from polyglot.harness import get_eval_script

    write(
        "bench",
        "task_order",
        "hgm.py: task list construction; hgm_utils.init (dedup); polyglot/harness.py: get_eval_script",
        [
            {"name": "swe_small_plus_medium_shuffled_seed_42", "input": "src/bench/subsets/swe/small.json + medium.json", "output": tasks},
            {"name": "swe_total_tasks_after_init_dedup", "input": "shuffled list + its first 3 again", "output": swe_dedup},
            {"name": "polyglot_medium_plus_small", "input": "src/bench/subsets/polyglot/medium.json + small.json", "output": poly},
            {"name": "polyglot_total_tasks_after_init_dedup", "input": "polyglot list", "output": poly_dedup},
            {"name": "get_eval_script", "input": ["cd /repo", "cargo test"], "output": get_eval_script(["cd /repo", "cargo test"])},
        ],
    )


def capture_testrepo_prompt():
    from prompts.testrepo_prompt import get_test_command, get_test_description

    scripts = {
        "pytest": "#!/bin/bash\nset -uxo pipefail\ncd /testbed\ngit checkout abc tests/test_a.py\npytest -rA tests/test_a.py tests/test_b.py\ngit checkout abc tests/test_a.py\n",
        "django": "#!/bin/bash\ncd /testbed\n./tests/runtests.py --verbosity 2 --settings=test_sqlite --parallel 1 admin_views.tests auth_tests.test_views\ngit checkout abc tests/admin_views/tests.py\n",
        "sympy": "#!/bin/bash\ncd /testbed\nPYTHONWARNINGS='ignore::UserWarning,ignore::SyntaxWarning' bin/test -C --verbose sympy/core/tests/test_basic.py\ngit checkout abc sympy/core/tests/test_basic.py\n",
        "no_paths": "#!/bin/bash\ntox -e py39\ntrue\n",
    }
    cases = []
    for name, script in scripts.items():
        cases.append({"name": f"get_test_command_{name}", "input": script, "output": list(get_test_command(script))})
        cases.append(
            {"name": f"get_test_description_swe_{name}", "input": script, "output": get_test_description(eval_script=script, swerepo=True)}
        )
    cmd = "set -e\n[ ! -d \"build\" ] && mkdir build\ncd build\nmake"
    cases.append({"name": "get_test_description_polyglot", "input": cmd, "output": get_test_description(cmd, polyglot=True)})
    cases.append({"name": "get_test_description_hgm", "input": "", "output": get_test_description(swerepo=False)})
    write(
        "agent",
        "testrepo_prompt",
        "prompts/testrepo_prompt.py: get_test_command, get_test_description",
        cases,
    )


def capture_polyglot_specs():
    import platform

    from polyglot import test_spec as ts

    test_patch = (
        "diff --git a/test_x.py b/test_x.py\nnew file mode 100644\n--- /dev/null\n+++ b/test_x.py\n@@ -0,0 +1 @@\n+t\n"
        "diff --git a/src/y.py b/src/y.py\n--- a/src/y.py\n+++ b/src/y.py\n@@ -1 +1 @@\n-a\n+b\n"
    )
    real = platform.machine
    cases = []
    try:
        for lang in ("python", "rust", "go", "javascript", "cpp", "java"):
            for machine in ("x86_64", "arm64"):
                platform.machine = lambda m=machine: m
                inst = {
                    "instance_id": f"{lang}__task-one",
                    "language": lang,
                    "repo": f"polyglot/polyglot-benchmark/{lang}/exercises/practice/task-one",
                    "base_commit": "abc123",
                    "test_patch": test_patch,
                }
                spec = ts.make_test_spec(inst)
                cases.append(
                    {
                        "name": f"make_test_spec_{lang}_{machine}",
                        "input": {"instance": inst, "machine": machine},
                        "output": {
                            "arch": spec.arch,
                            "platform": spec.platform,
                            "base_image_key": spec.base_image_key,
                            "env_image_key": spec.env_image_key,
                            "instance_image_key": spec.instance_image_key,
                            "container_name": spec.get_instance_container_name("r1"),
                            "setup_env_script": spec.setup_env_script,
                            "eval_script": spec.eval_script,
                            "install_repo_script": spec.install_repo_script,
                            "base_dockerfile": spec.base_dockerfile,
                            "env_dockerfile": spec.env_dockerfile,
                            "instance_dockerfile": spec.instance_dockerfile,
                        },
                    }
                )
    finally:
        platform.machine = real
    write(
        "bench",
        "polyglot_specs",
        "polyglot/test_spec.py: make_test_spec; polyglot/dockerfiles.py",
        cases,
    )


def capture_py_shuffle():
    cases = []
    for seed, n in ((42, 10), (0, 1000), (7, 33), (2**40 + 5, 50), (2**64 - 1, 20)):
        items = list(range(n))
        random.seed(seed)
        random.shuffle(items)
        cases.append({"name": f"seed_{seed}_n_{n}", "input": {"seed": seed, "n": n}, "output": items})
    write("bench", "py_shuffle", "random.seed(int); random.shuffle", cases)


def capture_dataset_rows():
    import pyarrow as pa
    import pyarrow.parquet as pq
    from datasets import load_dataset

    rows = [
        {"repo": "astropy/astropy", "instance_id": "astropy__astropy-12907", "version": "4.3",
         "FAIL_TO_PASS": "[\"a\"]", "hints_text": "", "problem_statement": "Caf\u00e9 \u2603\nline two\r\n"},
        {"repo": "django/django", "instance_id": "django__django-11099", "version": "3.0",
         "FAIL_TO_PASS": "[]", "hints_text": None, "problem_statement": "tab\there"},
        {"repo": "sympy/sympy", "instance_id": "sympy__sympy-20590", "version": "1.7",
         "FAIL_TO_PASS": "[\"b\", \"c\"]", "hints_text": "x" * 3000, "problem_statement": ""},
    ]
    path = FIXTURES / "bench" / "unit" / "fixtures" / "rows.parquet"
    pq.write_table(pa.Table.from_pylist(rows), path, compression="snappy", row_group_size=2)
    loaded = [dict(r) for r in load_dataset("parquet", data_files=str(path), cache_dir=tempfile.mkdtemp())["train"]]
    write(
        "bench",
        "dataset_rows",
        "datasets.load_dataset('parquet') over rows.parquet, written by pyarrow with snappy and two row groups",
        [{"name": "rows_parquet", "input": "rows.parquet", "output": loaded}],
    )
    import datetime

    typed = pa.table({
        "b": pa.array([True, None], pa.bool_()),
        "i8": pa.array([-1, 2], pa.int8()),
        "i16": pa.array([-300, 2], pa.int16()),
        "i32": pa.array([-70000, 2], pa.int32()),
        "i64": pa.array([-(2**40), 2], pa.int64()),
        "u8": pa.array([200, 2], pa.uint8()),
        "u16": pa.array([60000, 2], pa.uint16()),
        "u32": pa.array([4000000000, 2], pa.uint32()),
        "u64": pa.array([2**63, 2], pa.uint64()),
        "f32": pa.array([0.5, -1.25], pa.float32()),
        "f64": pa.array([0.1, -2.0], pa.float64()),
        "s": pa.array(["x", None], pa.string()),
        "l": pa.array([[1, 2], []], pa.list_(pa.int64())),
        "g": pa.array([{"k": 1, "v": "a"}, {"k": None, "v": None}], pa.struct([("k", pa.int64()), ("v", pa.string())])),
        "d": pa.array([datetime.date(2020, 1, 2), None], pa.date32()),
    })
    typed_path = FIXTURES / "bench" / "unit" / "fixtures" / "types.parquet"
    pq.write_table(typed, typed_path, compression="snappy")
    typed_rows = [dict(r) for r in load_dataset("parquet", data_files=str(typed_path), cache_dir=tempfile.mkdtemp())["train"]]
    write(
        "bench",
        "dataset_types",
        "datasets.load_dataset('parquet') over types.parquet: every column type the reader maps, dates shown with str()",
        [{"name": "types_parquet", "input": "types.parquet", "output": json.loads(json.dumps(typed_rows, default=str))}],
    )


def capture_swe_report():
    from swe_bench import report

    result = {
        "instance_id": "django__django-11099",
        "model_name_or_path": "agent_1",
        "model_patch": "diff --git a/x.py b/x.py\n+caf\u00e9 \u2603\n",
        "proposed_model_patches": ["p1", "p\t2"],
    }
    empty = dict(result, model_patch="", proposed_model_patches=[])
    patch = (
        "diff --git a/src/a.py b/src/a.py\n+a\n"
        "diff --git a/tests/test_a.py b/tests/test_a.py\n+t\n"
        "diff --git a/src/b.py b/src/b.py\n+b\r\n"
    )
    files = {
        "a.json": {"instance_id": "inst-a", "model_name_or_path": "m_1", "model_patch": patch},
        "b.json": {"model_name_or_path": "m_1", "model_patch": "x"},
        "c.json": {"instance_id": "inst-c", "model_name_or_path": "m_1", "model_patch": "\u00fc", "extra": 1},
        "notes.txt": {"instance_id": "ignored"},
    }
    with tempfile.TemporaryDirectory() as tmp:
        for name, body in files.items():
            Path(tmp, name).write_text(json.dumps(body))
        preds, printed = quiet(report.load_predictions, [tmp])
        jsonl = Path(report.preds_to_jsonl(tmp, preds)).read_text()
    write(
        "bench",
        "swe_report",
        "swe_bench/harness.py: json.dumps(result, indent=4); swe_bench/report.py: load_predictions, preds_to_jsonl",
        [
            {"name": "prediction_json", "input": result, "output": json.dumps(result, indent=4)},
            {"name": "empty_prediction_json", "input": empty, "output": json.dumps(empty, indent=4)},
            {"name": "preds_to_jsonl_sorted_lines", "input": files, "output": sorted(jsonl.splitlines(keepends=True))},
            {"name": "load_predictions_skips", "input": "b.json", "output": printed.count("Skipping json without instance_id")},
        ],
    )


DIAGNOSE_TREE = {
    "root/coding_agent.py": "def main():\n    pass\n",
    "root/coding_agent_polyglot.py": "def polyglot_main():\n    pass\n",
    "root/tools/edit.py": "EDIT = 1\n",
    "root/tools/sub/bash.py": "BASH = 2\n",
    "root/tools/README.md": "not python\n",
    "root/utils/common.py": "  padded  \n\n",
    "root/utils/evo_utils.py": "EVO = 3\n",
    "root/utils/eval_utils.py": "EVAL = 4\n",
    "root/utils/docker_utils.py": "DOCKER = 5\n",
    "out/c1/model_patch.diff": "diff --git a/x b/x\n+parent\n",
    "out/c1/predictions/run_a/inst-1.md": "chat for inst-1\\nError in get_response_withtools: boom\\nmore",
    "out/c1/predictions/run_a/inst-1.json": json.dumps({"instance_id": "inst-1", "model_patch": "diff --git a/y b/y\\n+fix\\n", "eval_result": "resolved"}),
    "out/c1/logs/run_evaluation/run_a/run_a/inst-1/report.json": json.dumps({"inst-1": {"tests_status": {"FAIL_TO_PASS": {"success": ["t1"], "failure": ["t2"]}, "PASS_TO_PASS": {"success": ["t3"], "failure": []}}}}),
    "out/c1/predictions/run_b/inst-2.md": "chat for inst-2",
    "out/c1/predictions/run_b/inst-2.json": json.dumps({"instance_id": "inst-2", "model_patch": "", "eval_result": "empty_patch"}),
    "out/c1/predictions/run_b/inst-2_eval.md": "  eval log for inst-2  \n",
    "out/c1/predictions/notes.txt": "a file, not a run folder",
}

DIAGNOSE_DATASET = [
    {"instance_id": "inst-1", "patch": "gold-1", "test_patch": "tests-1", "problem_statement": "Issue {one}"},
    {"instance_id": "inst-2", "patch": "gold-2", "test_patch": "tests-2", "problem_statement": "Issue two",
     "language": "python", "reference_answers": "ref-2", "reference_tests": "reftests-2"},
]


def capture_diagnose_prompts():
    import prompts.self_improvement_prompt as sip

    cases = []
    with tempfile.TemporaryDirectory() as tmp:
        for rel, text in DIAGNOSE_TREE.items():
            path = Path(tmp, rel)
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text)
        root, out = os.path.join(tmp, "root"), os.path.join(tmp, "out")
        patches = [os.path.join(out, "c1", "model_patch.diff")]
        for entry in ["inst-1", "inst-2", "solve_empty_patches", "solve_stochasticity", "solve_contextlength"]:
            (system, prompt), _ = quiet(sip.get_diagnose_prompt_swe, entry, "c1", root, out, DIAGNOSE_DATASET, patch_files=patches)
            cases.append({"name": f"swe_{entry}", "input": {"entry_id": entry, "patch_files": True}, "output": [system, prompt]})
        (system, prompt), _ = quiet(sip.get_diagnose_prompt_swe, "inst-1", "c1", root, out, DIAGNOSE_DATASET)
        cases.append({"name": "swe_inst-1_no_patches", "input": {"entry_id": "inst-1", "patch_files": False}, "output": [system, prompt]})
        for entry, draw in [("inst-1", 0.9), ("inst-2", 0.9), ("inst-2", 0.1)]:
            saved = random.random
            random.random = lambda: draw
            try:
                (system, prompt), _ = quiet(sip.get_diagnose_prompt_polyglot, entry, "c1", root, out, DIAGNOSE_DATASET, patch_files=patches)
            finally:
                random.random = saved
            cases.append({"name": f"polyglot_{entry}_draw_{draw}", "input": {"entry_id": entry, "draw": draw, "patch_files": True}, "output": [system, prompt]})
    write(
        "core",
        "diagnose_prompts",
        "prompts/self_improvement_prompt.py: get_diagnose_prompt_swe, get_diagnose_prompt_polyglot over DIAGNOSE_TREE",
        [{"name": "tree", "input": DIAGNOSE_TREE, "output": DIAGNOSE_DATASET}] + cases,
    )


def main():
    capture_config()
    capture_tree()
    capture_ts_sample()
    capture_run_metadata()
    capture_evo_utils()
    capture_self_improvement_prompts()
    capture_context_length()
    capture_log_parsers()
    capture_eval_utils()
    capture_llm_json()
    capture_patch_filters()
    capture_tool_conversion()
    capture_tools()
    capture_dockerignore()
    capture_task_order()
    capture_testrepo_prompt()
    capture_polyglot_specs()
    capture_py_shuffle()
    capture_dataset_rows()
    capture_swe_report()
    capture_diagnose_prompts()


if __name__ == "__main__":
    main()

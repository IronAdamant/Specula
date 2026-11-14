#!/usr/bin/env python3
"""Unit tests for scripts/leverage.py (no engine, no agent, nothing started): what its two
reviews probed by hand, kept. `cargo test -p specula --test leverage_data` runs this file.

    python scripts/leverage_test.py
"""
import argparse
import io
import json
import shutil
import sys
import tempfile
import unittest
from contextlib import redirect_stdout
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import leverage as lev  # noqa: E402

SETTINGS = lev.load_json(lev.DATA / "settings.json")


def changed(base, **changes):
    out = json.loads(json.dumps(base))
    for key, value in changes.items():
        at, keys = out, key.split("__")
        for k in keys[:-1]:
            at = at[k]
        at[keys[-1]] = value
    return out


def quiet(fn, *args):
    with redirect_stdout(io.StringIO()) as out:
        result = fn(*args)
    return result, out.getvalue()


class Scratch(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="leverage-test-"))
        self.root = self.tmp / "runs"

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)

    def run_dir(self, job, arm, n, phase="judge", **extra):
        d = self.root / f"{job}-{arm}-{n}"
        (d / "work").mkdir(parents=True)
        r = {"leverage": 1, "job": job, "arm": arm, "n": n, "phase": phase,
             "settings": {"limits": SETTINGS["jobs"][job], "judge": SETTINGS["judge"], "builder_model": "builder", "judge_model": "judge", "runs_per_cell": 2,
                          "token_weights": SETTINGS["token_weights"]}, **extra}
        (d / "run.json").write_text(json.dumps(r), encoding="utf-8")
        return d

    def verdict(self, job, failed=()):
        items = lev.job(job)["acceptance"]
        v = {"items": [{"id": a["id"], "pass": a["id"] not in failed, "evidence": "seen"} for a in items], "defects": [], "could_not_check": []}
        return v, lev.score(v, items)

    def builder(self, weighted, usd=1.0, complete=True, **kinds):
        by = {"input": 0, "cache_write": 0, "cache_read": 0, "output": 0, "requests": 5, "context_at_end": 1, **kinds}
        return {"tokens": 1, "tool_uses": 5, "duration_s": 60.0, "by_kind": by, "weighted": weighted, "cost_usd": usd, "ended": "done", "tokens_complete": complete}


class Data(unittest.TestCase):
    def test_the_data_is_good_and_check_holds_what_the_data_test_holds(self):
        self.assertEqual(lev.check_all(), [])
        self.assertEqual(lev.check_settings(SETTINGS), [])
        # (Review R2: each of these passed --check and failed crates/specula/tests/leverage_data.rs.)
        for bad in (dict(runner__command=["claude", "-p", "{prompt}"]), dict(jobs__small__token_cap=5), dict(judge__wall_s=1), dict(arms=["engine"]),
                    dict(rules=["Stop every process you started."]), dict(token_weights={"input": 1}), dict(runs_per_cell=0)):
            self.assertTrue(lev.check_settings(changed(SETTINGS, **bad)), bad)
        small = lev.load_json(lev.DATA / "jobs" / "small.json")
        self.assertEqual(lev.check_job(small, "small"), [])
        # (Review R1: a brief naming a tool, a list of three, a change of itself.)
        self.assertTrue(any("names no tool" in p for p in lev.check_job(changed(small, brief=small["brief"] + ["Build it with Specula."]), "small")))
        self.assertTrue(any("at least 8" in p for p in lev.check_job(changed(small, acceptance=small["acceptance"][:3]), "small")))
        self.assertTrue(any("another job" in p for p in lev.check_job(changed(small, change_of="small"), "small")))
        self.assertTrue(lev.check_job(changed(small, id="other"), "small"))

    def test_weighted_is_each_kind_times_its_weight(self):
        self.assertEqual(lev.weighted({"input": 10, "cache_write": 100, "cache_read": 1000, "output": 20}, {"input": 1.0, "cache_write": 1.25, "cache_read": 0.1, "output": 5.0}), 335)


class Harness(Scratch):
    def test_prepare_makes_nothing_before_it_has_checked(self):
        # (Review R1: without --exe it left a half-made folder that refused every retry.)
        a = argparse.Namespace(job="small", arm="engine", n=1, out=str(self.root), exe=None, port=None, from_run=None, build=None)
        with self.assertRaises(lev.Bad):
            lev.prepare(a)
        self.assertFalse((self.root / "small-engine-1").exists())
        a.arm = "plain"
        quiet(lev.prepare, a)
        run = lev.read_run(self.root / "small-plain-1")
        self.assertEqual(run["settings"]["token_weights"], SETTINGS["token_weights"], "a run keeps the weights its figures are made with")
        brief = (self.root / "small-plain-1" / "BRIEF.md").read_text(encoding="utf-8")
        self.assertIn("Do not use or read Specula", brief)
        with self.assertRaises(lev.Bad):
            lev.prepare(a)

    def test_a_judged_run_is_judged_again_only_with_its_first_judging_kept(self):
        self.run_dir("medium", "plain", 9)
        ch = self.run_dir("change", "plain", 9, **{"from": "medium-plain-9"})
        v, scored = self.verdict("change", failed=("c02",))
        (ch / "verdict.json").write_text(json.dumps(v), encoding="utf-8")
        lev.write_cost(ch, {"verdict": scored, "judge": {"weighted": 7, "cost_usd": 1.0}})
        # (Review R2: --again moved the first verdict away before the judge's page could refuse.)
        again = argparse.Namespace(run=str(ch), judge=True, again="the list changed", note=None, verdict=None)
        with self.assertRaises(lev.Bad) as refused:
            lev.run_cmd(again)
        self.assertIn("judge medium-plain-9 first", str(refused.exception))
        cost = lev.read_cost(ch)
        self.assertTrue(cost.get("verdict") and not cost.get("judged_before") and (ch / "verdict.json").is_file(), "nothing was moved")
        # By hand: a verdict is never written over unkept.
        by_hand = argparse.Namespace(run=str(ch), verdict=str(ch / "verdict.json"), again=None)
        with self.assertRaises(lev.Bad):
            lev.judge(by_hand)
        by_hand.again = "a second look"
        quiet(lev.judge, by_hand)
        cost = lev.read_cost(ch)
        self.assertEqual(len(cost["judged_before"]), 1)
        self.assertEqual(cost["judged_before"][0]["why"], "a second look")
        self.assertTrue((ch / "verdict.1.json").is_file() and (ch / "verdict.json").is_file())

    def test_a_changes_judge_is_told_what_the_game_passed_before(self):
        medium = self.run_dir("medium", "plain", 9)
        ch = self.run_dir("change", "plain", 9, **{"from": "medium-plain-9"})
        _, scored = self.verdict("medium", failed=("m10",))
        lev.write_cost(medium, {"verdict": scored})
        page = lev.judge_text(ch, lev.read_run(ch))
        self.assertIn("**m10**", page)
        line = next(x for x in page.splitlines() if x.startswith("- **m10**"))
        self.assertIn("FAILED before the change (not held against it)", line)
        self.assertIn("[passed before the change]", next(x for x in page.splitlines() if x.startswith("- **m01**")))

    def test_whether_an_engine_stopped_is_read_from_the_answer(self):
        # (Review R1: `specula stop` exits 1 for an engine that had stopped already and for one
        # that would not; every run recorded "not stopped".)
        run = {"arm": "engine", "port": 7700, "exe": "none"}
        real = lev.specula
        try:
            for answer, gone in (({"stopped": True}, True), ({"error": "not_listed"}, True), ({"error": "still_running"}, False), ({}, False)):
                lev.specula = lambda r, d, words, timeout=180, answer=answer: argparse.Namespace(stdout=json.dumps(answer), returncode=1)
                self.assertEqual(lev.stop_engine(run, self.tmp), gone, answer)
        finally:
            lev.specula = real
        self.assertTrue(lev.stop_engine({"arm": "plain"}, self.tmp))

    def test_a_stream_with_no_result_line_says_its_tokens_are_incomplete(self):
        def message(mid, out, tools=1):
            content = [{"type": "tool_use", "id": f"t{mid}{i}", "name": "Bash", "input": {"command": "ls"}} for i in range(tools)]
            return json.dumps({"type": "assistant", "message": {"id": mid, "content": content, "usage": {"input_tokens": 2, "cache_creation_input_tokens": 10, "cache_read_input_tokens": 100, "output_tokens": out}}})

        stream = self.tmp / "s.jsonl"
        stream.write_text("\n".join([message("a", 5), message("a", 9), message("b", 7), "not json"]) + "\n", encoding="utf-8")
        lev.seen_usage.clear()
        cut = lev.stream_usage(stream)
        self.assertFalse(cut["complete"])
        self.assertEqual((cut["by_kind"]["requests"], cut["by_kind"]["output"], cut["tool_uses"]), (2, 16, 3))
        result = {"type": "result", "total_cost_usd": 0.5, "num_turns": 3, "result": "done", "is_error": False,
                  "modelUsage": {"m": {"inputTokens": 4, "outputTokens": 40, "cacheReadInputTokens": 200, "cacheCreationInputTokens": 20}}}
        stream.write_text("\n".join([message("a", 5), message("b", 7), json.dumps(result)]) + "\n", encoding="utf-8")
        lev.seen_usage.clear()
        whole = lev.stream_usage(stream)
        self.assertTrue(whole["complete"])
        self.assertEqual((whole["by_kind"]["output"], whole["by_kind"]["cache_read"], whole["cost_usd"], whole["models"]), (40, 200, 0.5, ["m"]))

    def test_the_report_counts_what_was_run_and_every_judging(self):
        _, all_pass = self.verdict("small")
        _, one_miss = self.verdict("small", failed=("s05",))
        for n, weighted, build in ((1, 400_000, "first"), (2, 700_000, "first"), (3, 500_000, "second")):
            d = self.run_dir("small", "engine", n, engine_build=build)
            lev.write_cost(d, {"builder": self.builder(weighted), "verdict": all_pass, "judge": {"weighted": 100, "cost_usd": 0.5}})
        d = self.run_dir("small", "engine", 4, engine_build="second")
        # (A run stopped at its limit: its tokens are incomplete, and it is in no mean.)
        lev.write_cost(d, {"builder": {**self.builder(1, complete=False), "ended": "wall", "over_wall": True}, "verdict": all_pass})
        d = self.run_dir("small", "plain", 1)
        lev.write_cost(d, {"builder": self.builder(100_000), "verdict": one_miss, "judge": {"weighted": 100, "cost_usd": 0.5},
                           "judged_before": [{"why": "the list changed", "verdict": all_pass, "judge": {"weighted": 50, "cost_usd": 0.25}}]})
        _, text = quiet(lev.report, argparse.Namespace(root=str(self.root)))
        rows = {tuple(c.strip() for c in line.strip("|").split("|")[:4]): line for line in text.splitlines() if line.startswith("| small |")}
        self.assertIn(("small", "engine, build first", "2 of 2", "550,000"), rows)
        self.assertIn(("small", "engine, build second", "1 of 2", "500,000"), rows, "the incomplete run is left out of its cell's mean")
        self.assertIn("incomplete", text)
        # Past the cap by the weighted figure (600,000 for this job), whatever an older record said.
        self.assertIn("past the token cap", next(x for x in text.splitlines() if x.startswith("| small-engine-2 ")))
        self.assertNotIn("past the token cap", next(x for x in text.splitlines() if x.startswith("| small-engine-1 ")))
        # (Review R2: the judging total left out the earlier judgings.)
        self.assertIn("5 judging(s) of 4 run(s), 450 weighted tokens", text)
        self.assertIn("5.00 USD for the builds, 2.00 USD for the judging shown above, and 0.25 USD for the earlier judgings of the runs judged again (7.25 USD in all)", text)
        self.assertIn("small-plain-1: 14 / 14. Judged again because: the list changed", text)
        self.assertIn("small-plain-1 **s05**", text)

    def test_export_keeps_no_path_and_no_name_of_the_machine(self):
        home, user = Path.home(), Path.home().name
        d = self.run_dir("small", "plain", 1, exe=str(home / "bin" / "specula.exe"))
        v, scored = self.verdict("small")
        root = self.root.resolve()
        forms = [home.as_posix(), str(home), str(home).replace("\\", "\\\\"), home.as_posix().lower(), home.as_posix() + "x/y",
                 "/" + home.as_posix()[0].lower() + home.as_posix()[2:] if home.as_posix()[1:2] == ":" else home.as_posix(),
                 root.as_posix() + "/small-plain-1/work", str(root).upper(), lev.REPO.as_posix() + "/docs", f"file:///{home.as_posix()}", f"Users%5C{user}%5Cx"]
        scored["items"][0]["evidence"] = " | ".join(forms)
        lev.write_cost(d, {"builder": {**self.builder(700_000), "over_token_cap": False, "runner": str(home / "claude")}, "verdict": scored, "engine_stopped": False})
        dest = self.tmp / "kept"
        quiet(lev.export, argparse.Namespace(root=str(self.root), dest=str(dest)))
        for f in dest.iterdir():
            text = f.read_text(encoding="utf-8")
            # (Review R2: four backslashes, a longer name beginning with the home's, the name alone.)
            self.assertNotIn(user.lower(), text.lower(), f.name)
            self.assertNotIn(root.name + "/small", text.replace("\\", "/").lower(), f.name)
        kept = json.loads((dest / "small-plain-1.json").read_text(encoding="utf-8"))
        self.assertNotIn("exe", kept["run"])
        self.assertNotIn("runner", kept["costs"]["builder"])
        self.assertTrue(kept["costs"]["builder"]["over_token_cap"], "held against the weighted figure")
        self.assertNotIn("engine_stopped", kept["costs"], "what an older harness could not know is not said")

    def test_where_reads_a_builders_steps(self):
        repo = lev.REPO.as_posix()
        self.assertEqual(lev.step_kind("Write", {"file_path": "x.rs"}), "write code")
        self.assertEqual(lev.step_kind("Bash", {"command": f'python "{repo}/scripts/leverage.py" call "R" logic_build @a.json'}), "build the module")
        self.assertEqual(lev.step_kind("Bash", {"command": f'python "{repo}/scripts/leverage.py" tools "R" world_'}), "find tools and their arguments")
        self.assertEqual(lev.step_kind("Bash", {"command": f'python "{repo}/scripts/leverage.py" engine start "R"'}), "start or stop the engine")
        self.assertEqual(lev.step_kind("Bash", {"command": f'sed -n 1,80p "{repo}/docs/design/MODULES.md"'}), "read the engine's documents and source")
        self.assertEqual(lev.step_kind("Bash", {"command": "cat > game.py <<'EOF'\nprint(1)\nEOF"}), "write code")
        self.assertEqual(lev.step_kind("Bash", {"command": "python game.py --headless --ticks 60"}), "run its own checks")
        self.assertEqual(lev.step_kind("Read", {"file_path": "work/frame.png"}), "look at a frame")
        d = self.run_dir("small", "engine", 1)
        lev.write_cost(d, {"builder": self.builder(335, input=10, cache_write=100, cache_read=1000, output=20)})

        def use(tid, name, inp):
            return json.dumps({"type": "assistant", "message": {"id": tid, "content": [{"type": "tool_use", "id": tid, "name": name, "input": inp}]}})

        def answer(tid, text):
            return json.dumps({"type": "user", "message": {"content": [{"type": "tool_result", "tool_use_id": tid, "content": text}]}})

        build = {"command": f'python "{repo}/scripts/leverage.py" call "R" logic_build @a.json'}
        lines = [use("1", "Bash", {"command": f'cat "{repo}/README.md"'}), answer("1", "x" * 400), use("2", "Bash", build), answer("2", '{"ok": false, "errors": 2}'),
                 use("3", "Write", {"file_path": "m.rs", "content": "y" * 80}), answer("3", "ok"), use("4", "Bash", build), answer("4", '{"ok": true, "errors": 0}')]
        (lev.costs_dir(d) / "small-engine-1.builder.stream.jsonl").write_text("\n".join(lines) + "\n", encoding="utf-8")
        w = lev.where_of(d)
        self.assertEqual((w["steps"], w["steps_before_the_first_code"], w["builds"]), (4, 2, {"steps": 2, "failed": 1}))
        self.assertEqual(w["weighted_by_kind"], {"read_again": 100, "written_out": 100, "new_in_context": 135})
        self.assertEqual(sum(w["weighted_by_kind"].values()), w["weighted"])
        self.assertEqual(w["by_step"]["read the engine's documents and source"]["answered_tokens"], 100)
        self.assertIn("characters / 4", w["how"])


if __name__ == "__main__":
    unittest.main(verbosity=1)

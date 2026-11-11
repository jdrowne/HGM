use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::Duration;

use hgm_core::log::Log;
use hgm_core::metadata::update_metadata;
use hgm_core::pool;
use hgm_core::sampling::CoolDown;
use hgm_core::scheduler::{
    initial_expansions, pick_task, sample_budget, select_for_evaluation, select_for_expansion,
    should_expand,
};
use hgm_core::tree::Tree;
use rand::RngExt;
use rand::rngs::StdRng;

use crate::FAILED;
use crate::improve::Improver;

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub max_task_evals: usize,
    pub max_workers: usize,
    pub alpha: f64,
    pub beta: f64,
    pub cool_down: bool,
    pub n_pseudo_descendant_evals: usize,
    pub eval_random_level: f64,
    pub jitter: Duration,
}

#[derive(Debug)]
pub struct State {
    pub tree: Tree,
    pub submitted_ids: HashMap<usize, HashSet<String>>,
    pub n_task_evals: usize,
    pub init_evaluated_tasks: Vec<String>,
    pub n_pending_expands: usize,
    pub rng: StdRng,
}

pub type Evaluate<'a> = dyn Fn(&str, &[String]) -> Result<Vec<f64>, String> + Sync + 'a;

pub struct Search<'a> {
    pub settings: Settings,
    pub state: Mutex<State>,
    pub output_dir: &'a Path,
    pub total_tasks: &'a [String],
    pub improver: &'a dyn Improver,
    pub evaluate: &'a Evaluate<'a>,
    pub log: &'a Log,
}

fn commit_of(tree: &Tree, id: usize) -> String {
    tree.get(id)
        .map_or_else(String::new, |node| node.commit_id.clone())
}

impl Search<'_> {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn cool_down(&self, n_task_evals: usize) -> Option<CoolDown> {
        self.settings.cool_down.then_some(CoolDown {
            beta: self.settings.beta,
            max_task_evals: self.settings.max_task_evals,
            n_task_evals,
        })
    }

    fn record(&self, state: &State) -> Result<(), String> {
        update_metadata(
            self.output_dir,
            &state.tree,
            state.n_task_evals,
            &state.init_evaluated_tasks,
        )
        .map_err(|e| e.to_string())
    }

    fn exhausted(&self) -> bool {
        self.lock().n_task_evals >= self.settings.max_task_evals
    }

    pub fn expand(&self) -> Result<(), String> {
        let (parent, commit) = {
            let mut guard = self.lock();
            let state = &mut *guard;
            let cool_down = self.cool_down(state.n_task_evals);
            let parent = select_for_expansion(
                &state.tree,
                self.settings.n_pseudo_descendant_evals,
                cool_down,
                &mut state.rng,
            )
            .map_err(|e| e.to_string())?;
            (parent, commit_of(&state.tree, parent))
        };
        let child = self.improver.sample_child(&commit);
        let mut state = self.lock();
        if child == FAILED {
            return Ok(());
        }
        state.tree.add_child(parent, &child);
        self.record(&state)
    }

    fn measure(&self, mut guard: MutexGuard<'_, State>) -> Result<(), String> {
        let state = &mut *guard;
        let cool_down = self.cool_down(state.n_task_evals);
        let picked = select_for_evaluation(
            &state.tree,
            &state.submitted_ids,
            self.total_tasks.len(),
            cool_down,
            &mut state.rng,
        )
        .and_then(|id| {
            let submitted = state.submitted_ids.entry(id).or_default();
            pick_task(
                self.total_tasks,
                submitted,
                self.settings.eval_random_level,
                &mut state.rng,
            )
            .map(|task| (id, task))
        });
        let Some((id, task)) = picked else {
            return Ok(());
        };
        state
            .submitted_ids
            .entry(id)
            .or_default()
            .insert(task.clone());
        state.n_task_evals += 1;
        let commit = commit_of(&state.tree, id);
        drop(guard);
        let evals = (self.evaluate)(&commit, std::slice::from_ref(&task));
        let mut state = self.lock();
        let evals = evals?;
        state
            .tree
            .get_mut(id)
            .into_iter()
            .for_each(|node| node.utility_measures.extend(&evals));
        self.record(&state)
    }

    pub fn sample(&self) -> Result<(), String> {
        if self.exhausted() {
            return Ok(());
        }
        let pause = self
            .settings
            .jitter
            .mul_f64(self.lock().rng.random::<f64>());
        thread::sleep(pause);
        let mut state = self.lock();
        if state.n_task_evals >= self.settings.max_task_evals {
            return Ok(());
        }
        if !should_expand(
            state.n_task_evals,
            self.settings.alpha,
            state.tree.len(),
            state.n_pending_expands,
        ) {
            return self.measure(state);
        }
        state.n_pending_expands += 1;
        drop(state);
        let result = self.expand();
        self.lock().n_pending_expands -= 1;
        result
    }

    fn settle(&self, results: Vec<Result<(), String>>) -> Result<(), String> {
        let errors: Vec<String> = results.into_iter().filter_map(Result::err).collect();
        for e in &errors {
            self.log.error(format!("Error: {e}"));
            eprintln!("Error: {e}");
        }
        errors.into_iter().next().map_or(Ok(()), Err)
    }

    pub fn run(&self) -> Result<(), String> {
        let workers = self.settings.max_workers;
        let first = initial_expansions(self.lock().tree.len(), workers, self.settings.alpha);
        self.settle(pool::map(&vec![(); first], workers, |()| self.expand()))?;
        let budget = sample_budget(self.settings.max_task_evals);
        self.settle(pool::map(&vec![(); budget], workers, |()| self.sample()))
    }
}

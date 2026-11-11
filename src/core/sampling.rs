use rand::Rng;
use rand_distr::{Beta, Distribution};

pub const AT_BUDGET_TEMPERATURE: f64 = 10000.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CoolDown {
    pub beta: f64,
    pub max_task_evals: usize,
    pub n_task_evals: usize,
}

impl CoolDown {
    pub fn factor(&self) -> f64 {
        if self.n_task_evals >= self.max_task_evals {
            return AT_BUDGET_TEMPERATURE;
        }
        let max = self.max_task_evals as f64;
        let left = (self.max_task_evals - self.n_task_evals) as f64;
        max.powf(self.beta) / left.powf(self.beta)
    }
}

pub fn beta_params(evals: &[Vec<f64>], cool_down: Option<CoolDown>) -> (Vec<f64>, Vec<f64>) {
    let factor = cool_down.map(|c| c.factor());
    evals
        .iter()
        .map(|de| {
            let sum: f64 = de.iter().sum();
            let alpha = 1.0 + sum;
            let beta = (1 + de.len()) as f64 - sum;
            match factor {
                Some(f) => (alpha * f, beta * f),
                None => (alpha, beta),
            }
        })
        .unzip()
}

pub fn argmax(values: &[f64]) -> Option<usize> {
    let mut best: Option<usize> = None;
    for (i, v) in values.iter().enumerate() {
        match best {
            Some(b) if values[b] >= *v => {}
            _ => best = Some(i),
        }
    }
    best
}

pub fn thompson_sample<R: Rng + ?Sized>(
    evals: &[Vec<f64>],
    cool_down: Option<CoolDown>,
    rng: &mut R,
) -> Option<usize> {
    let (alphas, betas) = beta_params(evals, cool_down);
    let thetas: Vec<f64> = alphas
        .iter()
        .zip(&betas)
        .map(|(&a, &b)| {
            Beta::new(a, b)
                .expect("Beta parameters are positive and finite")
                .sample(rng)
        })
        .collect();
    argmax(&thetas)
}

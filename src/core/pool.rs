use std::sync::Mutex;

pub fn map<T, R, F>(items: &[T], workers: usize, f: F) -> Vec<R>
where
    T: Sync,
    R: Send,
    F: Fn(&T) -> R + Sync,
{
    let next = Mutex::new(0usize);
    let results: Mutex<Vec<Option<R>>> = Mutex::new((0..items.len()).map(|_| None).collect());
    let take = || {
        let mut i = next
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let current = *i;
        *i += 1;
        items.get(current).map(|item| (current, item))
    };
    let work = || {
        while let Some((i, item)) = take() {
            let r = f(item);
            results
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)[i] = Some(r);
        }
    };
    std::thread::scope(|scope| {
        for _ in 0..workers.clamp(1, items.len().max(1)) {
            scope.spawn(work);
        }
    });
    results
        .into_inner()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .into_iter()
        .flatten()
        .collect()
}

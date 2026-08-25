use std::sync::atomic::{AtomicBool, Ordering};

pub(crate) fn try_reserve_asset_run(active: &AtomicBool) -> bool {
    active
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
}

pub(crate) fn release_asset_run(active: &AtomicBool) {
    active.store(false, Ordering::Release);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_runs_are_single_flight() {
        let active = AtomicBool::new(false);
        assert!(try_reserve_asset_run(&active));
        assert!(!try_reserve_asset_run(&active));
        release_asset_run(&active);
        assert!(try_reserve_asset_run(&active));
    }
}

//! Graceful shutdown. Background loops check a [`Shutdown`] between passes
//! and stop there, so a pass, with its database transactions, always runs to
//! the end.

use std::time::Duration;

use tokio::sync::watch;

/// Requests shutdown from every [`Shutdown`] made with it.
#[derive(Debug)]
pub struct ShutdownTrigger(watch::Sender<bool>);

impl ShutdownTrigger {
    /// Asks every loop to stop after its current pass.
    pub fn trigger(&self) {
        self.0.send_replace(true);
    }
}

/// Tells a background loop when to stop.
#[derive(Debug, Clone)]
pub struct Shutdown(watch::Receiver<bool>);

/// A trigger and the [`Shutdown`] it controls.
#[must_use]
pub fn channel() -> (ShutdownTrigger, Shutdown) {
    let (sender, receiver) = watch::channel(false);
    (ShutdownTrigger(sender), Shutdown(receiver))
}

impl Shutdown {
    /// A shutdown that is never requested, for loops that run until dropped.
    #[must_use]
    pub fn never() -> Self {
        // With the trigger dropped, `requested` never resolves.
        channel().1
    }

    /// Whether shutdown has been requested.
    #[must_use]
    pub fn is_requested(&self) -> bool {
        *self.0.borrow()
    }

    /// Resolves once shutdown is requested.
    pub async fn requested(&mut self) {
        if self.0.wait_for(|requested| *requested).await.is_err() {
            std::future::pending::<()>().await;
        }
    }

    /// Sleeps for `duration` unless shutdown is requested first. Returns
    /// whether the loop should go on.
    pub async fn sleep(&mut self, duration: Duration) -> bool {
        if self.is_requested() {
            return false;
        }
        tokio::select! {
            () = tokio::time::sleep(duration) => !self.is_requested(),
            () = self.requested() => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{Shutdown, channel};

    #[tokio::test]
    async fn sleeping_ends_early_once_shutdown_is_requested() {
        let (trigger, mut shutdown) = channel();
        assert!(!shutdown.is_requested());
        let mut waiting = shutdown.clone();
        let sleeper = tokio::spawn(async move { waiting.sleep(Duration::from_secs(3600)).await });
        trigger.trigger();
        let keep_going = tokio::time::timeout(Duration::from_secs(5), sleeper)
            .await
            .expect("woke up")
            .expect("joined");
        assert!(!keep_going);
        assert!(shutdown.is_requested());
        assert!(!shutdown.sleep(Duration::from_secs(3600)).await);
    }

    #[tokio::test]
    async fn a_never_shutdown_keeps_sleeping_normally() {
        let mut shutdown = Shutdown::never();
        assert!(shutdown.sleep(Duration::from_millis(1)).await);
        assert!(!shutdown.is_requested());
    }
}

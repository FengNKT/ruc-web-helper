use super::*;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Trigger {
    Startup,
    Resume,
}
impl Trigger {
    pub fn timeout_message(self) -> &'static str {
        match self {
            Self::Startup => "启动登录未完成：等待校园网络超时，请检查网络后手动登录",
            Self::Resume => "唤醒后自动登录未完成：等待校园网络超时，请检查网络后手动登录",
        }
    }
}
#[derive(Clone, Copy)]
pub struct Attempt {
    generation: u64,
    pub deadline: Instant,
    pub trigger: Trigger,
}
#[derive(Default)]
pub struct AutomaticLogin {
    inner: Mutex<(u64, Option<Attempt>)>,
}
impl AutomaticLogin {
    pub fn begin(&self, trigger: Trigger, now: Instant) {
        let mut inner = self.inner.lock().unwrap();
        inner.0 += 1;
        inner.1 = Some(Attempt {
            generation: inner.0,
            deadline: now + Duration::from_secs(300),
            trigger,
        });
    }
    pub fn cancel(&self) {
        self.inner.lock().unwrap().1 = None;
    }
    pub fn current(&self) -> Option<Attempt> {
        self.inner.lock().unwrap().1
    }
    pub fn is_current(&self, attempt: Attempt) -> bool {
        self.current()
            .is_some_and(|task| task.generation == attempt.generation)
    }
    pub fn finish(&self, attempt: Attempt) -> bool {
        let mut inner = self.inner.lock().unwrap();
        if inner
            .1
            .is_some_and(|task| task.generation == attempt.generation)
        {
            inner.1 = None;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resume_renews_expired_startup_and_stale_completion_cannot_cancel_it() {
        let tasks = AutomaticLogin::default();
        let now = Instant::now();
        tasks.begin(Trigger::Startup, now);
        let startup = tasks.current().unwrap();
        let later = now + Duration::from_secs(600);
        assert!(startup.deadline < later);
        tasks.begin(Trigger::Resume, later);
        assert!(!tasks.is_current(startup));
        assert!(!tasks.finish(startup));
        let wake = tasks.current().unwrap();
        assert_eq!(wake.trigger, Trigger::Resume);
        assert_eq!(wake.deadline, later + Duration::from_secs(300));
        assert!(tasks.finish(wake));
        assert!(tasks.current().is_none());
    }
    #[test]
    fn manual_action_cancels_retries_until_a_new_lifecycle_trigger() {
        let tasks = AutomaticLogin::default();
        assert!(tasks.current().is_none());
        tasks.begin(Trigger::Resume, Instant::now());
        let wake = tasks.current().unwrap();
        tasks.cancel();
        assert!(!tasks.is_current(wake));
        assert!(tasks.current().is_none());
        tasks.begin(Trigger::Resume, Instant::now());
        assert!(tasks.current().is_some());
    }
}

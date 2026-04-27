pub const MODULE: &str = "shutdown";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeMode {
    Validate,
    Paper,
    Replay,
}

impl RuntimeMode {
    pub fn as_str(self) -> &'static str {
        match self {
            RuntimeMode::Validate => "validate",
            RuntimeMode::Paper => "paper",
            RuntimeMode::Replay => "replay",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownPhase {
    Running,
    Draining,
    Complete,
}

impl ShutdownPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            ShutdownPhase::Running => "running",
            ShutdownPhase::Draining => "draining",
            ShutdownPhase::Complete => "complete",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GracefulShutdownState {
    run_id: String,
    mode: RuntimeMode,
    phase: ShutdownPhase,
    accepting_new_work: bool,
    reason: Option<String>,
}

impl GracefulShutdownState {
    pub fn new(run_id: impl Into<String>, mode: RuntimeMode) -> Self {
        Self {
            run_id: run_id.into(),
            mode,
            phase: ShutdownPhase::Running,
            accepting_new_work: true,
            reason: None,
        }
    }

    pub fn request_shutdown(&mut self, reason: impl Into<String>) {
        self.reason = Some(reason.into());
        self.accepting_new_work = false;
        if self.phase == ShutdownPhase::Running {
            self.phase = ShutdownPhase::Draining;
        }
    }

    pub fn complete(&mut self) {
        self.accepting_new_work = false;
        self.phase = ShutdownPhase::Complete;
    }

    pub fn run_id(&self) -> &str {
        &self.run_id
    }

    pub fn mode(&self) -> RuntimeMode {
        self.mode
    }

    pub fn phase(&self) -> ShutdownPhase {
        self.phase
    }

    pub fn phase_name(&self) -> &'static str {
        self.phase.as_str()
    }

    pub fn accepting_new_work(&self) -> bool {
        self.accepting_new_work
    }

    pub fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shutdown_request_stops_new_work_before_completion() {
        let mut state = GracefulShutdownState::new("run-1", RuntimeMode::Paper);

        assert_eq!(state.phase(), ShutdownPhase::Running);
        assert!(state.accepting_new_work());

        state.request_shutdown("operator_signal");

        assert_eq!(state.phase(), ShutdownPhase::Draining);
        assert!(!state.accepting_new_work());
        assert_eq!(state.reason(), Some("operator_signal"));

        state.complete();

        assert_eq!(state.phase(), ShutdownPhase::Complete);
        assert_eq!(state.phase_name(), "complete");
    }

    #[test]
    fn runtime_mode_names_match_cli_modes() {
        assert_eq!(RuntimeMode::Validate.as_str(), "validate");
        assert_eq!(RuntimeMode::Paper.as_str(), "paper");
        assert_eq!(RuntimeMode::Replay.as_str(), "replay");
    }
}

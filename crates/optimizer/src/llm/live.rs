//! Live output from an in-flight request, for the thinking bubble.
//!
//! A thread-local sink, installed by the chat worker beside the cancellation
//! predicate (see [`super::cancel`]). The stream readers and the tool loop
//! call the free functions below; with no sink installed they are no-ops, so
//! every other caller — examples, tests, the `choya_live` harness — is
//! unchanged.

use std::cell::RefCell;

/// One stage of a chat request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Handshake,
    Reference,
    Lookup(usize),
    Scoring,
    Writing,
    Fallback,
}

/// What the expanded bubble has to show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LiveMode {
    /// The provider streams reasoning; show it.
    Reasoning,
    /// No reasoning, but the answer streams; show that.
    Content,
    /// Only tool activity so far.
    #[default]
    ToolsOnly,
    /// The provider neither streams nor reasons; text arrives at once.
    AtOnce,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LiveEvent {
    Step(Step),
    Reasoning(String),
    Content(String),
    ToolCall(String),
    Mode(LiveMode),
}

pub trait LiveSink {
    fn event(&self, event: LiveEvent);
}

thread_local! {
    static SINK: RefCell<Option<Box<dyn LiveSink>>> = const { RefCell::new(None) };
}

/// Installs a sink for the current thread and restores the previous one on
/// drop, the same shape as [`super::cancel::CancelScope`].
pub struct LiveScope {
    previous: Option<Box<dyn LiveSink>>,
}

impl LiveScope {
    pub fn new(sink: impl LiveSink + 'static) -> Self {
        let previous = SINK.with(|slot| slot.borrow_mut().replace(Box::new(sink)));
        Self { previous }
    }
}

impl Drop for LiveScope {
    fn drop(&mut self) {
        let previous = self.previous.take();
        SINK.with(|slot| *slot.borrow_mut() = previous);
    }
}

fn emit(event: LiveEvent) {
    SINK.with(|slot| {
        if let Some(sink) = slot.borrow().as_ref() {
            sink.event(event);
        }
    });
}

pub fn step(step: Step) {
    emit(LiveEvent::Step(step));
}

pub fn reasoning(delta: &str) {
    if !delta.is_empty() {
        emit(LiveEvent::Reasoning(delta.to_string()));
    }
}

pub fn content(delta: &str) {
    if !delta.is_empty() {
        emit(LiveEvent::Content(delta.to_string()));
    }
}

pub fn tool_call(name: &str) {
    emit(LiveEvent::ToolCall(name.to_string()));
}

pub fn mode(mode: LiveMode) {
    emit(LiveEvent::Mode(mode));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Rc;

    struct Recorder(Rc<RefCell<Vec<LiveEvent>>>);

    impl LiveSink for Recorder {
        fn event(&self, event: LiveEvent) {
            self.0.borrow_mut().push(event);
        }
    }

    #[test]
    fn no_sink_is_noop() {
        step(Step::Handshake);
        reasoning("x");
        content("y");
        tool_call("z");
        mode(LiveMode::AtOnce);
    }

    #[test]
    fn installed_sink_receives_in_order_and_is_cleared_after() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        {
            let _scope = LiveScope::new(Recorder(seen.clone()));
            step(Step::Lookup(2));
            reasoning("weighing");
            reasoning("");
            content("Minstrel");
            tool_call("score_build");
            mode(LiveMode::Reasoning);
        }
        step(Step::Writing);
        assert_eq!(
            *seen.borrow(),
            vec![
                LiveEvent::Step(Step::Lookup(2)),
                LiveEvent::Reasoning("weighing".into()),
                LiveEvent::Content("Minstrel".into()),
                LiveEvent::ToolCall("score_build".into()),
                LiveEvent::Mode(LiveMode::Reasoning),
            ]
        );
    }
}

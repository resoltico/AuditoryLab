//! Deterministic execution and private-state accessors for adapter integration tests.
use super::*;
use auditorylab_app::workspace::Response;
use auditorylab_app::workspace::{
    AudioCommand, Command, Executor, FileCommand, Job, JobKind, Runner, ShutdownError,
};
pub(crate) fn isolated_waker(ui: &MainWindow) -> std::sync::Arc<dyn Fn() + Send + Sync> {
    wake::Wake::new(ui).callback()
}
#[derive(Default)]
pub(crate) struct DeterministicExecutor {
    jobs: Vec<Job>,
}
impl Executor for DeterministicExecutor {
    fn start(&mut self, job: Job) {
        self.jobs.push(job);
    }
    fn poll(&mut self) -> Vec<Response> {
        let mut responses = Vec::new();
        for job in std::mem::take(&mut self.jobs) {
            if job.kind() == JobKind::File {
                let response = job.execute(|prepared| responses.push(prepared));
                responses.push(response);
            } else {
                self.jobs.push(job);
            }
        }
        responses
    }
    fn shutdown(mut self: Box<Self>, mut workspace: Workspace) -> Result<(), ShutdownError> {
        let _ = workspace.apply(Command::Audio(AudioCommand::Stop));
        let _ = workspace.apply(Command::File(FileCommand::CancelExport));
        let mut runner = Runner::new();
        for job in self.jobs.drain(..) {
            runner.start(job);
        }
        runner.shutdown(workspace)
    }
}
impl SlintAdapter {
    pub(crate) fn reverse_rendered_choices(&self) {
        use auditorylab_app::workspace::QueryFacet;
        let workspace = self.workspace.borrow();
        let workspace = workspace.as_ref().unwrap();
        let choices = [
            QueryFacet::Scope,
            QueryFacet::Sort,
            QueryFacet::Direction,
            QueryFacet::Group,
            QueryFacet::Band,
            QueryFacet::Method,
            QueryFacet::Population,
            QueryFacet::Role,
            QueryFacet::Analysis,
        ]
        .into_iter()
        .map(|facet| {
            let mut view = workspace.library_choices(facet);
            view.options.reverse();
            (facet, view)
        })
        .collect();
        let updating = self.updating.replace(true);
        self.project_choice_models(
            &self.main.upgrade().unwrap(),
            &workspace.library_facets(),
            choices,
        );
        self.updating.set(updating);
    }

    pub(crate) fn timer_running(&self) -> bool {
        self.timer.running()
    }
    pub(crate) fn test_waker(&self) -> std::sync::Arc<dyn Fn() + Send + Sync> {
        self.wake.callback()
    }
    pub(crate) fn take_jobs(&self) -> Vec<Job> {
        let mut executor = self.executor.borrow_mut();
        let driver = executor
            .as_mut()
            .expect("adapter driver remains until shutdown");
        let any = driver.as_mut() as &mut dyn std::any::Any;
        let driver = any
            .downcast_mut::<DeterministicExecutor>()
            .expect("test fixture installs deterministic execution");
        std::mem::take(&mut driver.jobs)
    }
    pub(crate) fn deliver(self: &Rc<Self>, response: Response) {
        if let Some(workspace) = self.workspace.borrow_mut().as_mut() {
            workspace.respond(response);
        }
        self.after_command();
    }
    pub(crate) fn inspect<R>(&self, f: impl FnOnce(&mut Workspace) -> R) -> R {
        f(self
            .workspace
            .borrow_mut()
            .as_mut()
            .expect("the workspace is open until shutdown"))
    }
    pub(crate) fn monitor_presented(&self) -> bool {
        self.monitor_window_visible.get()
    }
}
